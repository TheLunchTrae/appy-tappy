//! The automation editor's live reading: watching a saved value on its window
//! and telling the interface what it reads.

use std::collections::BTreeMap;
use std::fs;
use std::io;

use appytappy_config::automation_file::WhenDef;
use appytappy_config::signal::SignalName;
use appytappy_sources::screen::check::Check;
use appytappy_sources::screen::saved;
use appytappy_sources::screen::{Crop, Frame, Layout, OutsideWindow, Size};
use appytappy_sources::system::best_window;

use super::host::Host;
use super::messages::{Live, Message, Seen};
use super::{Owner, WATCH_INTERVAL};

/// The saved value the automation editor watches.
#[derive(Debug)]
pub(super) struct Watched {
    /// Its application's ID.
    app: String,
    /// Its ID.
    value: String,
    /// Its name, for messages.
    name: String,
    /// The window's size when its region was drawn.
    window_size: Size,
    /// The value's check, which reads the captured region.
    check: Check,
    /// What the newest frame showed, or `None` before one arrived.
    pub(super) seen: Option<Seen>,
    /// Why the capture stopped, once it has.
    pub(super) failure: Option<String>,
}

/// A watch's message for one frame. A window too small for the region is
/// another size, not a stopped capture: the capture goes on.
fn watch_message<H: Host>(generation: u64, frame: io::Result<Frame<'_>>) -> Message<H> {
    let seen = match frame {
        Ok(frame) => Ok(Seen::Region {
            window_size: frame.window_size,
            image: frame.image.clone(),
        }),
        Err(error) => OutsideWindow::in_error(&error)
            .map(Seen::Outside)
            .ok_or_else(|| error.to_string()),
    };
    Message::Watch { generation, seen }
}

impl<H: Host> Owner<H> {
    /// Captures the region of the saved value `value` on the window of the
    /// application `app`, for the automation editor, and answers the
    /// watch's number. Like [`Owner::pick`], it drops the previous window's
    /// capture, screenshots and check.
    pub(crate) fn watch(&mut self, app: &str, value: &str) -> Result<u64, String> {
        let stores = self.stores()?;
        let file = stores.app_file(app)?;
        let saved = file
            .values
            .iter()
            .find(|saved| saved.id == value)
            .ok_or_else(|| format!("{} no longer has that value.", file.name))?;
        let folder = stores.apps.folder(app);
        let check = saved::build(&saved.screen, &file.references, |name| {
            fs::read(folder.join(name))
        })
        .map_err(|error| format!("{} cannot be read: {error}.", saved.name))?;
        let processes = self.host.processes().map_err(|error| error.to_string())?;
        let windows = self.host.windows().map_err(|error| error.to_string())?;
        let window = best_window(&file.window.to_match(), processes, windows)
            .ok_or_else(|| format!("Open {} to see what {} reads.", file.name, saved.name))?;
        let watch = self.start(
            window,
            Crop::Layout(Layout::one(check.region)),
            WATCH_INTERVAL,
            watch_message,
        )?;
        self.watched = Some(Watched {
            app: app.to_owned(),
            value: value.to_owned(),
            name: saved.name.clone(),
            window_size: check.window_size,
            check: check.check,
            seen: None,
            failure: None,
        });
        Ok(watch)
    }

    /// What the watched value reads on the newest frame, and whether `when`
    /// holds for it; an error once the watch was stopped or replaced.
    pub(crate) fn watched_reading(
        &self,
        watch: u64,
        when: Option<&WhenDef>,
    ) -> Result<Live, String> {
        let watched = self
            .watched
            .as_ref()
            .filter(|_| watch == self.generation)
            .ok_or_else(|| "No value is watched.".to_owned())?;
        let name = &watched.name;
        if let Some(error) = &watched.failure {
            return Ok(Live::Lost {
                message: format!("{name} cannot be read: {error}."),
            });
        }
        let (window_size, image) = match &watched.seen {
            None => return Ok(Live::Waiting),
            Some(Seen::Region { window_size, image }) => (*window_size, Some(image)),
            Some(Seen::Outside(window_size)) => (*window_size, None),
        };
        let Some(image) = image.filter(|_| window_size == watched.window_size) else {
            return Ok(Live::Problem {
                message: format!(
                    "{name} was drawn when the window was {}, and it is {window_size} now. \
                     Resize the window to read it.",
                    watched.window_size
                ),
            });
        };
        let value = match watched.check.read(image) {
            Ok(value) => value,
            Err(error) => {
                return Ok(Live::Problem {
                    message: format!("{name} cannot be read now: {error}."),
                });
            },
        };
        let holds = when
            .filter(|when| when.app == watched.app && when.value == watched.value)
            .and_then(|when| {
                let signal = SignalName::new(&when.value).ok()?;
                let condition = when.condition(signal.clone()).ok()?;
                Some(condition.holds(&BTreeMap::from([(signal, value.clone())])))
            });
        Ok(Live::Read {
            reading: value.into(),
            holds,
        })
    }

    /// Stops watch `watch`, unless another capture took its place.
    pub(crate) fn stop_watching(&mut self, watch: u64) {
        if self.watched.is_some() && watch == self.generation {
            self.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use appytappy_config::app_file::WindowDef;
    use appytappy_config::automation_file::{ComparisonDef, TargetDef};
    use appytappy_sources::screen::{FrameHandler, Image, Region};

    use crate::check_spec::CheckSpec;
    use crate::owner::messages::Reading;
    use crate::owner::test_support::{
        FakeHost, Harness, STATE, WINDOW, add_example_app, frame_image, harness_with,
        scratch_store, spec, with_saved_bar,
    };

    use super::*;

    fn watch(app: &str, value: &str) -> impl FnOnce(&mut Owner<FakeHost>) -> Result<u64, String> {
        let (app, value) = (app.to_owned(), value.to_owned());
        move |owner: &mut Owner<FakeHost>| owner.watch(&app, &value)
    }

    fn watched(
        watch: u64,
        when: Option<WhenDef>,
    ) -> impl FnOnce(&mut Owner<FakeHost>) -> Result<Live, String> {
        move |owner: &mut Owner<FakeHost>| {
            owner.watched_reading(watch, when.map(Box::new).as_deref())
        }
    }

    /// The condition that `value` of `example` is at least `target`.
    fn at_least(value: &str, target: f64) -> WhenDef {
        WhenDef {
            app: "example".to_owned(),
            value: value.to_owned(),
            is: ComparisonDef::AtLeast,
            target: TargetDef::Number(target),
        }
    }

    /// Watches `Progress bar` and returns the watch and its capture's
    /// handler, checking it captures only the value's region.
    fn watch_bar(harness: &mut Harness) -> (u64, FrameHandler) {
        let watch = harness
            .ask(watch("example", "progress_bar"))
            .expect("watches");
        let started = harness.handlers.try_recv().expect("capture started");
        assert_eq!(
            (started.window, started.crop, started.interval),
            (
                7,
                Crop::Layout(Layout::one(Region::new(0, 0, 4, 2).expect("not empty"))),
                WATCH_INTERVAL
            )
        );
        (watch, started.handler)
    }

    #[test]
    fn the_editor_reads_a_saved_value_on_its_window_and_tests_the_condition() {
        let (mut harness, root) = with_saved_bar("watch");
        assert_eq!(
            harness.ask(watch("gone", "progress_bar")),
            Err("That application is no longer saved.".to_owned())
        );
        assert_eq!(
            harness.ask(watch("example", "gone")),
            Err("Example App no longer has that value.".to_owned())
        );
        assert_eq!(
            harness.ask(watched(0, None)),
            Err("No value is watched.".to_owned())
        );

        let (watch, mut handler) = watch_bar(&mut harness);
        assert_eq!(harness.closed.try_recv(), Ok(7), "the capture was replaced");
        assert_eq!(harness.ask(watched(watch, None)), Ok(Live::Waiting));
        harness.deliver_at(&mut handler, Ok((WINDOW, &frame_image())), WATCH_INTERVAL);
        let read = |holds| {
            Ok(Live::Read {
                reading: Reading::Number(50.0),
                holds,
            })
        };
        assert_eq!(harness.ask(watched(watch, None)), read(None));
        assert_eq!(
            harness.ask(watched(watch, Some(at_least("progress_bar", 50.0)))),
            read(Some(true))
        );
        assert_eq!(
            harness.ask(watched(watch, Some(at_least("progress_bar", 51.0)))),
            read(Some(false))
        );
        // A condition on another value says nothing about this one.
        assert_eq!(
            harness.ask(watched(watch, Some(at_least("other", 0.0)))),
            read(None)
        );
        let mut elsewhere = at_least("progress_bar", 0.0);
        "other".clone_into(&mut elsewhere.app);
        assert_eq!(harness.ask(watched(watch, Some(elsewhere))), read(None));
        // A setup check of the capture screen does not change what is read.
        let spec: CheckSpec = serde_json::from_str(
            r#"{"kind":"color","region":{"x":0,"y":0,"width":1,"height":1},
                "color":{"red":255,"green":0,"blue":0},"tolerance":0}"#,
        )
        .expect("valid JSON");
        harness
            .ask(|owner| owner.set_check(&spec))
            .expect("valid check");
        assert_eq!(harness.ask(watched(watch, None)), read(None));
        assert_eq!(
            harness.ask(watched(watch.wrapping_add(1), None)),
            Err("No value is watched.".to_owned()),
            "another watch's number reads nothing"
        );

        let resized = |width| {
            Ok(Live::Problem {
                message: format!(
                    "Progress bar was drawn when the window was 4x2, and it is {width}x2 now. \
                     Resize the window to read it."
                ),
            })
        };
        let wider = Size {
            width: 6,
            height: 2,
        };
        harness.deliver_at(&mut handler, Ok((wider, &frame_image())), WATCH_INTERVAL);
        assert_eq!(harness.ask(watched(watch, None)), resized(6));
        let narrower = Size {
            width: 3,
            height: 2,
        };
        harness.deliver_at(
            &mut handler,
            Err(OutsideWindow {
                window_size: narrower,
            }
            .into()),
            WATCH_INTERVAL,
        );
        assert_eq!(harness.ask(watched(watch, None)), resized(3));
        let white = Image::new(WINDOW, vec![255; 32]).expect("matching length");
        harness.deliver_at(&mut handler, Ok((WINDOW, &white)), WATCH_INTERVAL);
        let Ok(Live::Problem { message }) = harness.ask(watched(watch, None)) else {
            panic!("a bar of neither color cannot be read");
        };
        assert!(
            message.starts_with("Progress bar cannot be read now: ") && message.ends_with('.'),
            "{message}"
        );
        harness.deliver_at(
            &mut handler,
            Err(io::Error::other("the window closed")),
            WATCH_INTERVAL,
        );
        assert_eq!(
            harness.ask(watched(watch, None)),
            Ok(Live::Lost {
                message: "Progress bar cannot be read: the window closed.".to_owned()
            })
        );
        assert_eq!(
            harness.closed.try_recv(),
            Ok(7),
            "a failed capture is closed at once"
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_watch_stops_only_itself() {
        let (mut harness, root) = with_saved_bar("stop-watch");
        let (first, mut stale) = watch_bar(&mut harness);
        let (second, mut handler) = watch_bar(&mut harness);
        assert_eq!(harness.closed.try_recv(), Ok(7));
        assert_eq!(harness.closed.try_recv(), Ok(7), "the first watch stopped");
        assert_eq!(
            harness.ask(watched(first, None)),
            Err("No value is watched.".to_owned())
        );
        // A late stop of the first watch leaves the second running.
        harness.ask(|owner| owner.stop_watching(first));
        assert!(harness.closed.try_recv().is_err());
        assert_eq!(harness.ask(watched(second, None)), Ok(Live::Waiting));
        // Frames from the first watch's capture are dropped.
        harness.deliver_at(&mut stale, Err(io::Error::other("closed")), WATCH_INTERVAL);
        assert_eq!(harness.ask(watched(second, None)), Ok(Live::Waiting));
        harness.deliver_at(&mut handler, Ok((WINDOW, &frame_image())), WATCH_INTERVAL);
        // Picking a window for a new value stops the watch, and a stop of a
        // watch that a pick replaced leaves the pick's capture running.
        let _picked = harness.pick();
        assert_eq!(harness.closed.try_recv(), Ok(7));
        harness.ask(|owner| owner.stop_watching(second));
        assert!(harness.closed.try_recv().is_err());
        let (third, _handler) = watch_bar(&mut harness);
        assert_eq!(harness.closed.try_recv(), Ok(7));
        harness.ask(|owner| owner.stop_watching(third));
        assert_eq!(harness.closed.try_recv(), Ok(7));
        assert_eq!(
            harness.ask(watched(third, None)),
            Err("No value is watched.".to_owned())
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_value_that_cannot_be_watched_says_why() {
        let (stores, root) = scratch_store("unwatched");
        let apps = stores.apps.clone();
        let mut harness = harness_with(false, Ok(stores));
        add_example_app(&mut harness);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Owner::screenshot), Ok(0));
        let save = |spec| move |owner: &mut Owner<FakeHost>| owner.save(&spec);
        harness
            .ask(save(spec("example", "Icon visible", STATE)))
            .expect("saves");
        let folder = apps.folder("example");
        let picture = folder.join("icon_visible-1.png");
        let bytes = std::fs::read(&picture).expect("saved picture");
        std::fs::remove_file(&picture).expect("remove");
        let missing = harness
            .ask(watch("example", "icon_visible"))
            .expect_err("no picture");
        assert!(
            missing.starts_with("Icon visible cannot be read: ") && missing.ends_with('.'),
            "{missing}"
        );
        std::fs::write(&picture, bytes).expect("restore");

        let mut file = apps.load().expect("loads")[0]
            .file
            .clone()
            .expect("a good file");
        let saved_window = file.window.clone();
        file.window = WindowDef::exact(appytappy_config::window_match::WindowFacts {
            title: "Other App",
            class: "OtherWindow",
            executable: "Other.exe",
        });
        apps.save("example", &file, &[]).expect("saves");
        assert_eq!(
            harness.ask(watch("example", "icon_visible")),
            Err("Open Example App to see what Icon visible reads.".to_owned())
        );
        file.window = saved_window;
        apps.save("example", &file, &[]).expect("saves");
        harness.owner.host.unlisted = true;
        assert_eq!(
            harness.ask(watch("example", "icon_visible")),
            Err("no windows here".to_owned())
        );
        harness.owner.host.unlisted = false;
        harness.owner.host.refuse = true;
        assert_eq!(
            harness.ask(watch("example", "icon_visible")),
            Err("no capture here".to_owned())
        );
        assert_eq!(
            harness.ask(watched(harness.owner.generation, None)),
            Err("No value is watched.".to_owned())
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }
}
