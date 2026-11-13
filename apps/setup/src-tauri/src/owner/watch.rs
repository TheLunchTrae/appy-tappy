//! The automation editor's live reading: watching a saved value on its window
//! and telling the interface what it reads.
//!
//! The capture copies only the value's region as it reads at one window
//! size, and each frame means to the watch what it means to the runtime
//! ([`FrameMeaning`]). Once frames show another size, and no other, for a quarter
//! of a second, the value is placed there once, and the capture starts
//! again for its region there, so the watch reads the value at each size as
//! the runtime does, and a drag across many sizes starts it again only
//! once.

use std::collections::BTreeMap;
use std::io;
use std::mem;
use std::time::Instant;

use appytappy_config::automation_file::WhenDef;
use appytappy_config::signal::SignalName;
use appytappy_sources::screen::saved::{self, AtSize, SavedValue, Unread};
use appytappy_sources::screen::settle::Settle;
use appytappy_sources::screen::{
    CaptureSession, Crop, Frame, FrameMeaning, Image, Layout, OutsideWindow, Size,
};

use serde::Serialize;

use crate::debug::workings::{self, ReadingDetails};
use crate::reading::{Reading, at_size};
use crate::ticket::{Running, Ticket};

use super::capture::{self, Capture};
use super::host::Host;
use super::messages::Event;
use super::{Owner, WATCH_INTERVAL};

/// What a watched value reads on the newest frame, for the automation
/// editor.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum Live {
    /// No frame has arrived from the window yet.
    Waiting,
    /// The value's reading.
    Read {
        /// What it reads.
        reading: Reading,
        /// Whether the condition the editor asked about holds for it, or
        /// `None` when it asked about none.
        holds: Option<bool>,
    },
    /// The window shows but the value cannot be read on it, and why.
    Problem {
        /// What is wrong, for the user.
        message: String,
    },
    /// The capture stopped, such as when the window closed, and why. The
    /// editor watches the value again.
    Lost {
        /// What happened, for the user.
        message: String,
    },
}

/// What a frame of the watched value's window showed.
#[derive(Debug)]
pub(crate) struct Seen {
    /// The window's size.
    pub(crate) window_size: Size,
    /// The pixels of the region the capture copies, or `None` when the
    /// region lies outside a window of this size.
    pub(crate) image: Option<Image>,
}

/// The automation editor's watch of a saved value.
#[derive(Debug)]
pub(super) struct Watching<S> {
    /// The value and what its frames showed.
    watched: Watched,
    /// The capture of the value's region, or why it stopped.
    capture: WatchCapture<S>,
}

/// A watch's capture.
#[derive(Debug)]
enum WatchCapture<S> {
    /// It runs.
    Running(Running<S>),
    /// It stopped, and why. Nothing starts it again: the editor shows why
    /// and watches the value again.
    Lost(String),
}

impl<S: CaptureSession> Watching<S> {
    /// The ticket of the capture, while it runs.
    pub(super) const fn ticket(&self) -> Option<Ticket> {
        match &self.capture {
            WatchCapture::Running(running) => Some(running.ticket),
            WatchCapture::Lost(_) => None,
        }
    }

    /// Stops the capture, if it runs.
    ///
    /// # Errors
    ///
    /// Returns the error the capture's thread ended with.
    pub(super) fn close(self) -> io::Result<()> {
        match self.capture {
            WatchCapture::Running(running) => running.close(),
            WatchCapture::Lost(_) => Ok(()),
        }
    }

    /// Stops the capture, if it runs, and keeps `error` as why it stopped,
    /// for the editor to show.
    fn lose(&mut self, error: String) {
        if let WatchCapture::Running(running) =
            mem::replace(&mut self.capture, WatchCapture::Lost(error))
        {
            // Nothing needs this capture, even if it would go on: the editor
            // watches again, or another takes its place.
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the watch says why its capture stopped, whatever its thread ended with"
            )]
            let _ = running.close();
        }
    }
}

/// The saved value the automation editor watches.
#[derive(Debug)]
struct Watched {
    /// The watch's number: the ticket of its first capture, which it keeps
    /// when its capture starts again for another window size.
    id: Ticket,
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
    /// What the newest frame showed and when it was seen, or `None` before
    /// one arrived.
    seen: Option<(Seen, Instant)>,
}

/// A watch's event for one frame. A window too small for the region is
/// a frame without pixels, not a stopped capture: the capture goes on.
fn watch_event(frame: io::Result<Frame<'_>>) -> Event {
    Event::Watch(match frame {
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
    })
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
        let check = saved::build(
            &saved.screen,
            &mut saved::Readers::new(&file.references),
            file.size_changes,
            |name| found.picture(name),
        )
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
        self.stop();
        let capture = capture::start(
            &self.host,
            &mut self.tickets,
            &self.messages,
            (window, Crop::Layout(Layout::one(at.region))),
            WATCH_INTERVAL,
            watch_event,
        )?;
        let id = capture.ticket;
        self.capture = Capture::Watching(Box::new(Watching {
            watched: Watched {
                id,
                app: app.to_owned(),
                value: value.to_owned(),
                name: saved.name.clone(),
                window,
                check,
                reads: (size, at),
                unread: None,
                settle: Settle::new(size),
                seen: None,
            },
            capture: WatchCapture::Running(capture),
        }));
        Ok(id.number())
    }

    /// Takes what the watch's capture sent: keeps what a frame of the
    /// watched value's window showed, and acts on the size its frames
    /// settled at, as [`Owner::settle_watch`] does, or stops the capture
    /// that failed and keeps why.
    pub(super) fn watch_sent(&mut self, seen: Result<Seen, String>) {
        let now = self.host.now();
        let Some(watching) = self.capture.watching_mut() else {
            return;
        };
        match seen {
            Ok(seen) => {
                watching.watched.settle.saw(seen.window_size, now);
                watching.watched.seen = Some((seen, now));
                self.settle_watch(now);
            },
            Err(error) => watching.lose(error),
        }
    }

    /// Places the watched value at the size its frames settled at by `now`,
    /// once they have, unless the capture reads at that size already. Where
    /// the value reads there, the capture reads at that size from then on,
    /// and starts again when the value's region there is another than the
    /// one copied, keeping the watch's number. Where it does not, the
    /// capture goes on, and the watch says why while frames show that size.
    /// A watch whose capture stopped is placed nowhere.
    fn settle_watch(&mut self, now: Instant) {
        let Some(watching) = self.capture.watching_mut() else {
            return;
        };
        if watching.ticket().is_none() {
            return;
        }
        let watched = &mut watching.watched;
        let placed = watched
            .settle
            .settled(now)
            .filter(|size| *size != watched.reads.0)
            .map(|size| (size, watched.check.at(size)));
        match placed {
            Some((size, Ok(at))) => {
                let moved = at.region != watched.reads.1.region;
                watched.reads = (size, at);
                watched.unread = None;
                if moved {
                    // Starting again drops the old capture's frames, which
                    // are of the old region. The old capture stops first,
                    // so the window has one at a time.
                    watched.seen = None;
                    let region = watched.reads.1.region;
                    let copied = (watched.window, Crop::Layout(Layout::one(region)));
                    watching.lose("its capture is starting again".to_owned());
                    let started = capture::start(
                        &self.host,
                        &mut self.tickets,
                        &self.messages,
                        copied,
                        WATCH_INTERVAL,
                        watch_event,
                    );
                    watching.capture = match started {
                        Ok(running) => WatchCapture::Running(running),
                        Err(error) => WatchCapture::Lost(error),
                    };
                }
            },
            Some((size, Err(unread))) => watched.unread = Some((size, unread)),
            None => {},
        }
    }

    /// The watch numbered `watch`, or an error once it was stopped or
    /// replaced.
    fn watching(&self, watch: u64) -> Result<&Watching<H::Session>, String> {
        self.capture
            .watching()
            .filter(|watching| watching.watched.id.number() == watch)
            .ok_or_else(|| "No value is watched.".to_owned())
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
        let watching = self.watching(watch)?;
        let watched = &watching.watched;
        let name = &watched.name;
        if let WatchCapture::Lost(error) = &watching.capture {
            return Ok(Live::Lost {
                message: format!("{name} cannot be read: {error}."),
            });
        }
        let Some((seen, _seen_at)) = &watched.seen else {
            return Ok(Live::Waiting);
        };
        let (reads_at, at) = &watched.reads;
        let image = match FrameMeaning::of(*reads_at, seen.window_size, seen.image.as_ref()) {
            FrameMeaning::Readable(image) => image,
            // Nothing is read at another size: once it settled, the value
            // either reads there, and the capture's size is that one, or
            // does not, and this says why.
            FrameMeaning::Resized(size) => {
                return Ok(watched
                    .unread
                    .filter(|(unread_at, _)| *unread_at == size)
                    .map_or(Live::Waiting, |(_, unread)| Live::Problem {
                        message: format!(
                            "{name} is not read {}. {}",
                            at_size(size, unread),
                            advice(unread)
                        ),
                    }));
            },
            FrameMeaning::Unreadable(outside) => {
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
        let Watching {
            watched,
            capture: WatchCapture::Running(_),
        } = self.watching(watch)?
        else {
            return Ok(None);
        };
        let (reads_at, at) = &watched.reads;
        let Some((seen, seen_at)) = &watched.seen else {
            return Ok(None);
        };
        let FrameMeaning::Readable(image) =
            FrameMeaning::of(*reads_at, seen.window_size, seen.image.as_ref())
        else {
            return Ok(None);
        };
        let age = self.host.now().saturating_duration_since(*seen_at);
        Ok(Some(workings::details(watched.check.check(at), image, age)))
    }

    /// Stops watch `watch`, unless another capture took its place.
    pub(crate) fn stop_watching(&mut self, watch: u64) {
        if self.watching(watch).is_ok() {
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
    use std::io;
    use std::time::Duration;

    use appytappy_config::app_file::WindowDef;
    use appytappy_config::automation_file::{ComparisonDef, TargetDef};
    use appytappy_sources::screen::{FrameHandler, Image, Region};

    use appytappy_sources::screen::settle::SETTLE;

    use crate::debug::workings::WorkingsView;
    use crate::owner::test_support::{
        BAR, FakeHost, Harness, STATE, WINDOW, add_example_app, capturing, example_build,
        frame_image, harness_with, scratch_store, spec, with_saved_bar,
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
    fn live_readings_reach_the_ui_tagged_by_kind() {
        let read = Live::Read {
            reading: Reading::Flag(true),
            holds: None,
        };
        assert_eq!(
            serde_json::to_value(&read).expect("serializes"),
            serde_json::json!({"kind": "read", "reading": {"kind": "flag", "value": true}, "holds": null})
        );
        assert_eq!(
            serde_json::to_value(Reading::Number(87.0)).expect("serializes"),
            serde_json::json!({"kind": "number", "value": 87.0})
        );
        assert_eq!(
            serde_json::to_value(Reading::from(appytappy_config::signal::Value::State(
                appytappy_config::signal::StateName::none(),
            )))
            .expect("serializes"),
            serde_json::json!({"kind": "state", "value": "none"})
        );
        assert_eq!(
            serde_json::to_value(Live::Waiting).expect("serializes"),
            serde_json::json!({"kind": "waiting"})
        );
        assert_eq!(
            serde_json::to_value(Live::Problem {
                message: "x".to_owned()
            })
            .expect("serializes"),
            serde_json::json!({"kind": "problem", "message": "x"})
        );
        assert_eq!(
            serde_json::to_value(Live::Lost {
                message: "y".to_owned()
            })
            .expect("serializes"),
            serde_json::json!({"kind": "lost", "message": "y"})
        );
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
                message: "Progress bar is not read at 3 × 2, the window's size now: scaled to this window size, its \
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
                Ok(Live::Problem { message }) if message.starts_with("Progress bar is not read at 3 × 2, the window's size now: ")
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
        // The capture reads at another size since, so a frame that only
        // passes the size the bar was not read at says nothing of it.
        harness.deliver_at(&mut handler, Ok((narrower, &frame_image())), WATCH_INTERVAL);
        assert_eq!(harness.ask(watched(watch, None)), Ok(Live::Waiting));
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
                message: "Progress bar is not read at 8 × 4, the window's size now: it is read only at the window sizes \
                          it was drawn at. Resize the window, or fix the value at this size on \
                          the Values page."
                    .to_owned()
            })
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_watch_whose_capture_cannot_start_again_says_why() {
        let (mut harness, root) = with_saved_bar("watch-restart-refused");
        let (watch, mut handler) = watch_bar(&mut harness);
        assert_eq!(harness.closed.try_recv(), Ok(7), "the pick's capture");
        // The window settles at a size where the bar's region is another,
        // and the capture for that region cannot start.
        let large = Size {
            width: 8,
            height: 4,
        };
        let red = Image::new(large, [0, 0, 255, 0].repeat(32)).expect("matching length");
        harness.deliver_at(&mut handler, Ok((large, &red)), WATCH_INTERVAL);
        harness.wait(SETTLE);
        harness.owner.host.refuse = true;
        assert_eq!(
            harness.ask(watched(watch, None)),
            Ok(Live::Lost {
                message: "Progress bar cannot be read: no capture here.".to_owned()
            })
        );
        assert_eq!(harness.closed.try_recv(), Ok(7), "the old capture closes");
        assert!(harness.handlers.try_recv().is_err(), "no capture started");
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_watch_whose_capture_stopped_is_not_started_again() {
        let (mut harness, root) = with_saved_bar("watch-lost");
        let (watch, mut handler) = watch_bar(&mut harness);
        assert_eq!(harness.closed.try_recv(), Ok(7), "the pick's capture");
        // The window takes a size where the bar's region is another, and
        // its capture stops before that size settles.
        let large = Size {
            width: 8,
            height: 4,
        };
        let red = Image::new(large, [0, 0, 255, 0].repeat(32)).expect("matching length");
        harness.deliver_at(&mut handler, Ok((large, &red)), WATCH_INTERVAL);
        harness.deliver_at(
            &mut handler,
            Err(io::Error::other("the window closed")),
            WATCH_INTERVAL,
        );
        assert_eq!(harness.closed.try_recv(), Ok(7), "the capture closes");
        let lost = Ok(Live::Lost {
            message: "Progress bar cannot be read: the window closed.".to_owned(),
        });
        harness.wait(SETTLE);
        assert_eq!(harness.ask(watched(watch, None)), lost);
        assert!(harness.handlers.try_recv().is_err(), "no capture starts");
        // What the stopped capture still sends is dropped.
        harness.deliver_at(&mut handler, Ok((large, &red)), WATCH_INTERVAL);
        harness.wait(SETTLE);
        assert_eq!(harness.ask(watched(watch, None)), lost);
        assert!(harness.handlers.try_recv().is_err(), "nor then");
        assert!(harness.closed.try_recv().is_err(), "and none is left open");
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_watch_takes_the_place_of_the_picked_window_and_what_it_kept() {
        let (mut harness, mut handler, root) = capturing("watch-replaces-pick");
        harness
            .ask(|owner| owner.save(&spec("example", "Progress bar", BAR)))
            .expect("saves");
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert!(harness.ask(Owner::screenshot).is_ok());
        let (watch, _watching) = watch_bar(&mut harness);
        let kept = |owner: &mut Owner<FakeHost>| {
            (owner.preview(4), owner.screenshot_pixels(0, 4).is_some())
        };
        assert_eq!(harness.ask(kept), (Ok(None), false));
        // The pick's capture sends a frame late.
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(kept), (Ok(None), false));
        assert_eq!(harness.ask(watched(watch, None)), Ok(Live::Waiting));
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
            harness.ask(watched(1, None)),
            Err("No value is watched.".to_owned())
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }
}
