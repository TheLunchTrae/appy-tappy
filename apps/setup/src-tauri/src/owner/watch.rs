//! The automation editor's live reading: watching a saved value on its window
//! and telling the interface what it reads.
//!
//! The capture copies only the value's region as it reads at one window
//! size, and each frame means to the watch what it means to the runtime
//! ([`Shown`]). Once frames show another size, and no other, for a quarter
//! of a second, the value is placed there once, and the capture starts
//! again for its region there, so the watch reads the value at each size as
//! the runtime does, and a drag across many sizes starts it again only
//! once.

use std::collections::BTreeMap;
use std::io;
use std::time::Instant;

use appytappy_config::automation_file::WhenDef;
use appytappy_config::signal::SignalName;
use appytappy_sources::screen::saved::{self, AtSize, SavedValue, Unread};
use appytappy_sources::screen::settle::Settle;
use appytappy_sources::screen::{Crop, Frame, Layout, OutsideWindow, Shown, Size};

use crate::debug::workings::{self, ReadingDetails};

use super::host::Host;
use super::messages::{Live, Message, Seen};
use super::{Owner, WATCH_INTERVAL};

/// The saved value the automation editor watches.
#[derive(Debug)]
pub(super) struct Watched {
    /// The watch's number: the generation of its first capture, which it
    /// keeps when its capture starts again for another window size.
    id: u64,
    /// Its application's ID.
    app: String,
    /// Its ID.
    value: String,
    /// Its name, for messages.
    name: String,
    /// The captured window.
    window: u64,
    /// The value's saved check.
    check: SavedValue,
    /// The window size the capture reads the value at, and where and how
    /// it reads there. The capture copies that region.
    reads: (Size, AtSize),
    /// The size frames last settled at where the value does not read, and
    /// why. The capture goes on as it was, so it says so only while frames
    /// show that size.
    unread: Option<(Size, Unread)>,
    /// The window size frames show, settled.
    settle: Settle,
    /// What the newest frame showed, or `None` before one arrived.
    pub(super) seen: Option<Seen>,
    /// When the newest frame was seen.
    seen_at: Option<Instant>,
    /// Why the capture stopped, once it has.
    pub(super) failure: Option<String>,
}

/// A watch's message for one frame. A window too small for the region is
/// a frame without pixels, not a stopped capture: the capture goes on.
fn watch_message<H: Host>(generation: u64, frame: io::Result<Frame<'_>>) -> Message<H> {
    let seen = match frame {
        Ok(frame) => Ok(Seen {
            window_size: frame.window_size,
            image: Some(frame.image.clone()),
        }),
        Err(error) => OutsideWindow::in_error(&error)
            .map(|window_size| Seen {
                window_size,
                image: None,
            })
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
        let found = self.stores()?.app(app)?;
        let file = &found.file;
        let saved = file
            .values
            .iter()
            .find(|saved| saved.id == value)
            .ok_or_else(|| format!("{} no longer has that value.", file.name))?;
        let check = saved::build(&saved.screen, &file.references, file.size_changes, |name| {
            found.picture(name)
        })
        .map_err(|error| format!("{} cannot be read: {error}.", saved.name))?;
        let (processes, windows) = self.desktop()?;
        let window = self
            .host
            .read_window(&file.window.to_match(), processes, windows)
            .map(|read| read.entry.handle)
            .ok_or_else(|| format!("Open {} to see what {} reads.", file.name, saved.name))?;
        // A value reads where it was first drawn, at the size it was drawn
        // at, until frames settle at another size.
        let size = check.drawn.window_size;
        let at = check.first();
        let watch = self.start(
            window,
            Crop::Layout(Layout::one(at.region)),
            WATCH_INTERVAL,
            watch_message,
        )?;
        self.watched = Some(Watched {
            id: watch,
            app: app.to_owned(),
            value: value.to_owned(),
            name: saved.name.clone(),
            window,
            check,
            reads: (size, at),
            unread: None,
            settle: Settle::new(size),
            seen: None,
            seen_at: None,
            failure: None,
        });
        Ok(watch)
    }

    /// Keeps what a frame of the watched value's window showed, and acts on
    /// the size its frames settled at, as [`Owner::settle_watch`] does.
    pub(super) fn watch_seen(&mut self, seen: Seen) {
        let now = self.host.now();
        let Some(watched) = self.watched.as_mut() else {
            return;
        };
        watched.settle.saw(seen.window_size, now);
        watched.seen = Some(seen);
        watched.seen_at = Some(now);
        self.settle_watch(now);
    }

    /// Places the watched value at the size its frames settled at by `now`,
    /// once they have, unless the capture reads at that size already. Where
    /// the value reads there, the capture reads at that size from then on,
    /// and starts again when the value's region there is another than the
    /// one copied, keeping the watch's number. Where it does not, the
    /// capture goes on, and the watch says why while frames show that size.
    fn settle_watch(&mut self, now: Instant) {
        let Some(mut watched) = self.watched.take() else {
            return;
        };
        let placed = watched
            .settle
            .settled(now)
            .filter(|size| *size != watched.reads.0)
            .map(|size| (size, watched.check.at(size)));
        match placed {
            Some((size, Ok(at))) => {
                let moved = at.region != watched.reads.1.region;
                watched.reads = (size, at);
                if moved {
                    // Starting again drops the old capture's frames, which
                    // are of the old region.
                    watched.seen = None;
                    let started = self.start(
                        watched.window,
                        Crop::Layout(Layout::one(watched.reads.1.region)),
                        WATCH_INTERVAL,
                        watch_message,
                    );
                    watched.failure = started.err().or(watched.failure);
                }
            },
            Some((size, Err(unread))) => watched.unread = Some((size, unread)),
            None => {},
        }
        self.watched = Some(watched);
    }

    /// What the watched value reads on the newest frame, and whether `when`
    /// holds for it; an error once the watch was stopped or replaced. A
    /// window size that settled since the last frame places the value there
    /// first, so a window that stopped drawing is placed too.
    pub(crate) fn watched_reading(
        &mut self,
        watch: u64,
        when: Option<&WhenDef>,
    ) -> Result<Live, String> {
        self.settle_watch(self.host.now());
        let watched = self
            .watched
            .as_ref()
            .filter(|watched| watched.id == watch)
            .ok_or_else(|| "No value is watched.".to_owned())?;
        let name = &watched.name;
        if let Some(error) = &watched.failure {
            return Ok(Live::Lost {
                message: format!("{name} cannot be read: {error}."),
            });
        }
        let Some(seen) = &watched.seen else {
            return Ok(Live::Waiting);
        };
        let (reads_at, at) = &watched.reads;
        let image = match Shown::of(*reads_at, seen.window_size, seen.image.as_ref()) {
            Shown::Readable(image) => image,
            // Nothing is read at another size: once it settled, the value
            // either reads there, and the capture's size is that one, or
            // does not, and this says why.
            Shown::Resized(size) => {
                return Ok(watched
                    .unread
                    .filter(|(unread_at, _)| *unread_at == size)
                    .map_or(Live::Waiting, |(_, unread)| Live::Problem {
                        message: format!(
                            "{name} is not read at {size}: {unread}. {}",
                            advice(unread)
                        ),
                    }));
            },
            Shown::Unreadable(outside) => {
                return Ok(Live::Problem {
                    message: format!("{name} cannot be read now: {outside}."),
                });
            },
        };
        let value = match watched.check.check(at).read(image) {
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

    /// How the watched value's check reads the newest frame, for the debug
    /// details: `None` while the watched value does not read, which
    /// [`Owner::watched_reading`] explains; an error once the watch was
    /// stopped or replaced.
    pub(crate) fn explain_watched(&self, watch: u64) -> Result<Option<ReadingDetails>, String> {
        let watched = self
            .watched
            .as_ref()
            .filter(|watched| watched.id == watch)
            .ok_or_else(|| "No value is watched.".to_owned())?;
        let (reads_at, at) = &watched.reads;
        let (Some(seen), Some(seen_at), None) = (&watched.seen, watched.seen_at, &watched.failure)
        else {
            return Ok(None);
        };
        let Shown::Readable(image) = Shown::of(*reads_at, seen.window_size, seen.image.as_ref())
        else {
            return Ok(None);
        };
        let age = self.host.now().saturating_duration_since(seen_at);
        Ok(Some(workings::details(watched.check.check(at), image, age)))
    }

    /// Stops watch `watch`, unless another capture took its place.
    pub(crate) fn stop_watching(&mut self, watch: u64) {
        if self
            .watched
            .as_ref()
            .is_some_and(|watched| watched.id == watch)
        {
            self.stop();
        }
    }
}

/// What the user can do about a value that is not read at a window size,
/// as a sentence.
const fn advice(unread: Unread) -> &'static str {
    match unread {
        Unread::NotDrawn => "Resize the window, or fix the value at this size on the Values page.",
        Unread::Outside | Unread::TooSmall => "Fix the value at this size on the Values page.",
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use appytappy_config::app_file::WindowDef;
    use appytappy_config::automation_file::{ComparisonDef, TargetDef};
    use appytappy_sources::screen::{FrameHandler, Image, Region};

    use appytappy_sources::screen::settle::SETTLE;

    use crate::debug::workings::WorkingsView;
    use crate::owner::messages::Reading;
    use crate::owner::test_support::{
        FakeHost, Harness, STATE, WINDOW, add_example_app, example_build, frame_image,
        harness_with, scratch_store, spec, with_saved_bar,
    };

    use super::*;

    fn watch(app: &str, value: &str) -> impl FnOnce(&mut Owner<FakeHost>) -> Result<u64, String> {
        let (app, value) = (app.to_owned(), value.to_owned());
        move |owner: &mut Owner<FakeHost>| owner.watch(&app, &value)
    }

    fn explain(
        watch: u64,
    ) -> impl FnOnce(&mut Owner<FakeHost>) -> Result<Option<ReadingDetails>, String> {
        move |owner: &mut Owner<FakeHost>| owner.explain_watched(watch)
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
        assert_eq!(harness.ask(explain(watch)), Ok(None), "no frame yet");
        harness.deliver_at(&mut handler, Ok((WINDOW, &frame_image())), WATCH_INTERVAL);
        harness.wait(Duration::from_millis(20));
        let details = harness
            .ask(explain(watch))
            .expect("explains")
            .expect("read at its size");
        assert_eq!(details.frame_age_ms, 20, "by the owner's clock");
        assert!(
            matches!(
                details.workings,
                Some(WorkingsView::BarFill { filled, counted }) if filled * 2 == counted
            ),
            "{details:?}"
        );
        assert_eq!(
            harness.ask(explain(watch.wrapping_add(1))),
            Err("No value is watched.".to_owned())
        );
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
        assert_eq!(
            harness.ask(watched(watch.wrapping_add(1), None)),
            Err("No value is watched.".to_owned()),
            "another watch's number reads nothing"
        );

        // As wide again but as tall, the bar pinned left reads where it was
        // drawn, so the capture goes on.
        let wider = Size {
            width: 6,
            height: 2,
        };
        harness.deliver_at(&mut handler, Ok((wider, &frame_image())), WATCH_INTERVAL);
        assert_eq!(
            harness.ask(watched(watch, None)),
            Ok(Live::Waiting),
            "until the size settles"
        );
        assert_eq!(
            harness.ask(explain(watch)),
            Ok(None),
            "a frame of another size than the value was placed at"
        );
        harness.wait(SETTLE);
        assert_eq!(harness.ask(watched(watch, None)), read(None));
        assert!(harness.closed.try_recv().is_err(), "the capture goes on");
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
        harness.wait(SETTLE);
        assert_eq!(
            harness.ask(watched(watch, None)),
            Ok(Live::Problem {
                message: "Progress bar is not read at 3x2: scaled to this window size, its \
                          region reaches outside the window. Fix the value at this size on the \
                          Values page."
                    .to_owned()
            })
        );
        // The capture still reads at the wider size, so a frame that shows
        // it again is read at once, as the runtime reads it, and a frame of
        // the size the bar does not read at says why at once.
        harness.deliver_at(&mut handler, Ok((wider, &frame_image())), WATCH_INTERVAL);
        assert_eq!(harness.ask(watched(watch, None)), read(None));
        assert!(harness.ask(explain(watch)).expect("explains").is_some());
        harness.deliver_at(&mut handler, Ok((narrower, &frame_image())), WATCH_INTERVAL);
        assert!(
            matches!(
                harness.ask(watched(watch, None)),
                Ok(Live::Problem { message }) if message.starts_with("Progress bar is not read at 3x2: ")
            ),
            "its region still fits, and it is not read there all the same"
        );
        assert_eq!(harness.ask(explain(watch)), Ok(None));
        let white = Image::new(WINDOW, vec![255; 32]).expect("matching length");
        harness.deliver_at(&mut handler, Ok((WINDOW, &white)), WATCH_INTERVAL);
        harness.wait(SETTLE);
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
            harness.ask(explain(watch)),
            Ok(None),
            "a failed capture's last frame is not explained"
        );
        assert_eq!(
            harness.closed.try_recv(),
            Ok(7),
            "a failed capture is closed at once"
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_watch_follows_its_window_to_each_size_and_keeps_its_number() {
        let (mut harness, root) = with_saved_bar("watch-sizes");
        let (watch, mut handler) = watch_bar(&mut harness);
        assert_eq!(harness.closed.try_recv(), Ok(7), "the pick's capture");
        // Twice the height, the bar doubles, and the capture starts again
        // for its region there.
        let large = Size {
            width: 8,
            height: 4,
        };
        let red = Image::new(large, [0, 0, 255, 0].repeat(32)).expect("matching length");
        harness.deliver_at(&mut handler, Ok((large, &red)), WATCH_INTERVAL);
        assert!(harness.closed.try_recv().is_err(), "not settled yet");
        // A drag passes another size, which starts the wait again; the
        // editor's poll places the value once it settled, with no frame.
        harness.wait(SETTLE / 2);
        let between = Size {
            width: 6,
            height: 3,
        };
        harness.deliver_at(&mut handler, Ok((between, &red)), WATCH_INTERVAL);
        harness.deliver_at(&mut handler, Ok((large, &red)), WATCH_INTERVAL);
        harness.wait(SETTLE / 2);
        assert_eq!(harness.ask(watched(watch, None)), Ok(Live::Waiting));
        assert!(harness.closed.try_recv().is_err(), "the wait started again");
        harness.wait(SETTLE);
        assert_eq!(harness.ask(watched(watch, None)), Ok(Live::Waiting));
        assert_eq!(harness.closed.try_recv(), Ok(7));
        let started = harness.handlers.try_recv().expect("capture started again");
        assert!(harness.handlers.try_recv().is_err(), "once");
        assert_eq!(
            started.crop,
            Crop::Layout(Layout::one(Region::new(0, 0, 8, 4).expect("not empty")))
        );
        assert_eq!(harness.ask(watched(watch, None)), Ok(Live::Waiting));
        // A late frame of the old capture is dropped.
        harness.deliver_at(&mut handler, Ok((WINDOW, &frame_image())), WATCH_INTERVAL);
        assert!(harness.handlers.try_recv().is_err(), "no capture starts");
        let mut handler = started.handler;
        let region = Image::new(large, [0, 0, 255, 0].repeat(32)).expect("matching length");
        harness.deliver_at(&mut handler, Ok((large, &region)), WATCH_INTERVAL);
        assert_eq!(
            harness.ask(watched(watch, None)),
            Ok(Live::Read {
                reading: Reading::Number(100.0),
                holds: None,
            })
        );
        // Read only at the sizes it was drawn at, it says so.
        let stores = harness.owner.stores().expect("stores");
        let mut file = stores.app("example").expect("saved").file;
        file.size_changes = appytappy_config::app_file::SizeChangesDef::Rarely;
        stores.apps.save("example", &file, &[]).expect("saves");
        let (watch, mut handler) = watch_bar(&mut harness);
        harness.deliver_at(&mut handler, Ok((large, &red)), WATCH_INTERVAL);
        harness.wait(SETTLE);
        harness.deliver_at(&mut handler, Ok((large, &red)), WATCH_INTERVAL);
        assert!(
            harness.handlers.try_recv().is_err(),
            "nothing to copy there"
        );
        assert_eq!(
            harness.ask(watched(watch, None)),
            Ok(Live::Problem {
                message: "Progress bar is not read at 8x4: it is read only at the window sizes \
                          it was drawn at. Resize the window, or fix the value at this size on \
                          the Values page."
                    .to_owned()
            })
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn the_editor_takes_the_window_and_build_the_runtime_reads() {
        let (mut harness, root) = with_saved_bar("watch-twin");
        // Another copy's window fits as well and is listed first, but it is
        // minimized, so the runtime reads window 7.
        harness.owner.host.twin = Some(40);
        let apps = harness.ask(Owner::apps).expect("lists");
        assert_eq!(apps[0].windows, [7, 6]);
        assert_eq!(apps[0].running.as_ref(), Some(&example_build()));
        let (_watch, _handler) = watch_bar(&mut harness);
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
