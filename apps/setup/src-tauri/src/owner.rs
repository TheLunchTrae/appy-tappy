//! The thread that owns the setup app's state.
//!
//! Tauri runs commands on its own threads, and the capture hands frames over
//! on another. None of them touches the state: each sends a [`Message`] here,
//! and a request carries a oneshot channel the owner answers on. The state is
//! the picked window's capture, its newest frame, the session's screenshots,
//! and the check the user is setting up.

use std::io;
use std::ops::ControlFlow;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

use appytappy_config::signal::Value;
use appytappy_sources::screen::{Capture, CaptureSession, Crop, FrameHandler, Image};
use appytappy_sources::system::{ProcessEntry, System, WindowEntry};
use serde::Serialize;
use tokio::sync::oneshot;

use crate::check_spec::{self, CheckSpec, RegionCheck};
use crate::pixels;
use crate::window_list::{self, WindowList};

/// The shortest time between two frames the preview takes: about 30 a
/// second, smooth to the eye at a fraction of a game's frame rate.
const PREVIEW_INTERVAL: Duration = Duration::from_millis(33);

/// What the setup app needs from the operating system: its windows and
/// processes, and a capture of one window.
pub(crate) trait Host {
    /// A running capture.
    type Session: CaptureSession;

    /// Every top-level window.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn windows(&self) -> io::Result<Vec<WindowEntry>>;

    /// Every running process.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn processes(&self) -> io::Result<Vec<ProcessEntry>>;

    /// Starts capturing the whole of `window`, calling `handler` with frames.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error when the capture cannot start.
    fn capture(&self, window: u64, handler: FrameHandler) -> io::Result<Self::Session>;
}

impl<T: System + Capture> Host for T {
    type Session = T::Session;

    fn windows(&self) -> io::Result<Vec<WindowEntry>> {
        System::windows(self)
    }

    fn processes(&self) -> io::Result<Vec<ProcessEntry>> {
        System::processes(self)
    }

    fn capture(&self, window: u64, handler: FrameHandler) -> io::Result<Self::Session> {
        self.start(window, Crop::Whole, PREVIEW_INTERVAL, handler)
    }
}

/// Where a request's answer goes.
pub(crate) type Reply<T> = oneshot::Sender<T>;

/// A value the check read, as the UI shows it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub(crate) enum Reading {
    /// A number, such as a percentage.
    Number(f64),
    /// Yes or no, such as whether a panel shows.
    Flag(bool),
}

impl From<Value> for Reading {
    fn from(value: Value) -> Self {
        match value {
            Value::Number(number) => Self::Number(number.raw()),
            Value::Flag(flag) => Self::Flag(flag),
        }
    }
}

/// A command from the UI.
#[derive(Debug)]
pub(crate) enum Request {
    /// Lists the windows the user can pick.
    ListWindows(Reply<Result<WindowList, String>>),
    /// Captures `window`, dropping the previous window's screenshots and
    /// check.
    Pick {
        /// The window's handle.
        window: u64,
        /// Answered once the capture started.
        reply: Reply<Result<(), String>>,
    },
    /// The newest frame, scaled to at most `max_width`, as
    /// [`pixels::rgba`] encodes it: `None` until a frame arrives.
    Preview {
        /// The widest the UI draws it.
        max_width: u32,
        /// The frame, or why the capture stopped.
        reply: Reply<Result<Option<Vec<u8>>, String>>,
    },
    /// Keeps the newest frame as a screenshot and answers its index.
    Screenshot(Reply<Result<usize, String>>),
    /// A screenshot, scaled and encoded like a preview.
    ScreenshotPixels {
        /// The screenshot's index.
        index: usize,
        /// The widest the UI draws it.
        max_width: u32,
        /// The pixels, or `None` for an index that does not exist.
        reply: Reply<Option<Vec<u8>>>,
    },
    /// Sets up the check the live reading uses.
    SetCheck {
        /// The check as the UI describes it.
        spec: CheckSpec,
        /// Answered with what to fix when the check cannot be built.
        reply: Reply<Result<(), String>>,
    },
    /// What the check reads on the newest frame: `None` until a check is
    /// set up and a frame arrived.
    Reading(Reply<Result<Option<Reading>, String>>),
}

/// What reaches the owner thread.
#[derive(Debug)]
pub(crate) enum Message {
    /// A command from the UI.
    Request(Request),
    /// The capture with this generation sent a frame.
    Frame {
        /// Which capture sent it; frames from an earlier one are dropped.
        generation: u64,
        /// The window's whole frame.
        image: Image,
    },
    /// The capture with this generation stopped, such as when the window
    /// closed.
    CaptureFailed {
        /// Which capture stopped.
        generation: u64,
        /// Why, for the user.
        error: String,
    },
    /// The app is closing.
    Shutdown,
}

/// The setup app's state.
#[derive(Debug)]
pub(crate) struct Owner<H: Host> {
    host: H,
    /// The setup app's own process, left out of the window list.
    own_process: u32,
    /// A sender for this thread's messages, handed to each capture.
    messages: Sender<Message>,
    /// Counts captures, so a frame from a window the user left is dropped.
    generation: u64,
    session: Option<H::Session>,
    latest: Option<Image>,
    failure: Option<String>,
    screenshots: Vec<Image>,
    check: Option<RegionCheck>,
}

impl<H: Host> Owner<H> {
    /// An owner with no window picked.
    pub(crate) const fn new(host: H, own_process: u32, messages: Sender<Message>) -> Self {
        Self {
            host,
            own_process,
            messages,
            generation: 0,
            session: None,
            latest: None,
            failure: None,
            screenshots: Vec::new(),
            check: None,
        }
    }

    /// Applies one message; breaks on [`Message::Shutdown`].
    pub(crate) fn handle(&mut self, message: Message) -> ControlFlow<()> {
        match message {
            Message::Request(request) => self.answer(request),
            Message::Frame { generation, image } => {
                if generation == self.generation {
                    self.latest = Some(image);
                }
            },
            Message::CaptureFailed { generation, error } => {
                if generation == self.generation {
                    self.failure = Some(error);
                }
            },
            Message::Shutdown => return ControlFlow::Break(()),
        }
        ControlFlow::Continue(())
    }

    /// Stops the capture, if one runs.
    ///
    /// # Errors
    ///
    /// Returns the error the capture's thread ended with.
    pub(crate) fn close(&mut self) -> io::Result<()> {
        self.session.take().map_or(Ok(()), CaptureSession::close)
    }

    fn answer(&mut self, request: Request) {
        match request {
            Request::ListWindows(reply) => send(reply, self.window_list()),
            Request::Pick { window, reply } => send(reply, self.pick(window)),
            Request::Preview { max_width, reply } => send(
                reply,
                match (&self.failure, &self.latest) {
                    (Some(error), _) => Err(error.clone()),
                    (None, latest) => {
                        Ok(latest.as_ref().map(|image| pixels::rgba(image, max_width)))
                    },
                },
            ),
            Request::Screenshot(reply) => send(reply, self.screenshot()),
            Request::ScreenshotPixels {
                index,
                max_width,
                reply,
            } => send(
                reply,
                self.screenshots
                    .get(index)
                    .map(|image| pixels::rgba(image, max_width)),
            ),
            Request::SetCheck { spec, reply } => send(
                reply,
                check_spec::build(&spec, &self.screenshots)
                    .map(|check| self.check = Some(check))
                    .map_err(|error| error.to_string()),
            ),
            Request::Reading(reply) => send(reply, self.reading()),
        }
    }

    fn window_list(&self) -> Result<WindowList, String> {
        let windows = self.host.windows().map_err(|error| error.to_string())?;
        let processes = self.host.processes().map_err(|error| error.to_string())?;
        Ok(window_list::list(windows, &processes, self.own_process))
    }

    fn pick(&mut self, window: u64) -> Result<(), String> {
        // A capture that failed already said why; the new window matters now.
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the old capture's error is moot once the user picked another window"
        )]
        let _ = self.close();
        self.generation = self.generation.wrapping_add(1);
        self.latest = None;
        self.failure = None;
        self.screenshots.clear();
        self.check = None;
        let generation = self.generation;
        let messages = self.messages.clone();
        let handler: FrameHandler = Box::new(move |frame| {
            let message = match frame {
                Ok(frame) => Message::Frame {
                    generation,
                    image: frame.image.clone(),
                },
                Err(error) => Message::CaptureFailed {
                    generation,
                    error: error.to_string(),
                },
            };
            // The owner stopped when the send fails, and the capture is
            // about to be stopped too, so the wait does not matter.
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a failed send means the owner stopped and will close this capture"
            )]
            let _ = messages.send(message);
            PREVIEW_INTERVAL
        });
        match self.host.capture(window, handler) {
            Ok(session) => {
                self.session = Some(session);
                Ok(())
            },
            Err(error) => Err(error.to_string()),
        }
    }

    fn screenshot(&mut self) -> Result<usize, String> {
        let image = self
            .latest
            .clone()
            .ok_or_else(|| "No frame has arrived from the window yet.".to_owned())?;
        self.screenshots.push(image);
        Ok(self.screenshots.len().saturating_sub(1))
    }

    fn reading(&self) -> Result<Option<Reading>, String> {
        let (Some(check), Some(latest)) = (&self.check, &self.latest) else {
            return Ok(None);
        };
        let cropped = latest.crop(check.region).ok_or_else(|| {
            "The region reaches past the edge of the window. Draw it inside the window.".to_owned()
        })?;
        check
            .check
            .read(&cropped)
            .map(|value| Some(value.into()))
            .map_err(|error| error.to_string())
    }
}

/// Runs `owner` until [`Message::Shutdown`] arrives, then stops its capture.
///
/// # Errors
///
/// Returns the error the capture's thread ended with.
pub(crate) fn run<H: Host>(mut owner: Owner<H>, messages: &Receiver<Message>) -> io::Result<()> {
    for message in messages {
        if owner.handle(message).is_break() {
            break;
        }
    }
    owner.close()
}

/// Answers a request. The command that asked may have been dropped, such as
/// when the window closed while it waited, and then nobody needs the answer.
fn send<T>(reply: Reply<T>, answer: T) {
    #[expect(
        clippy::let_underscore_must_use,
        reason = "a dropped command needs no answer"
    )]
    let _ = reply.send(answer);
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Instant;

    use appytappy_config::anti_cheat::ProcessId;
    use appytappy_sources::screen::{Frame, Size};
    use appytappy_sources::system::Bounds;

    use super::*;
    use crate::check_spec::CheckSpec;

    /// A host with one window whose captures the test drives by hand: each
    /// started capture's handler goes to the test over a channel, and so
    /// does each closed capture's window.
    struct FakeHost {
        handlers: Sender<(u64, FrameHandler)>,
        closed: Sender<u64>,
        refuse: bool,
    }

    #[derive(Debug)]
    struct FakeSession {
        window: u64,
        closed: Sender<u64>,
    }

    impl CaptureSession for FakeSession {
        fn close(self) -> io::Result<()> {
            self.closed.send(self.window).map_err(io::Error::other)
        }
    }

    impl Host for FakeHost {
        type Session = FakeSession;

        fn windows(&self) -> io::Result<Vec<WindowEntry>> {
            Ok(vec![WindowEntry {
                handle: 7,
                process: ProcessId::new(3),
                title: "Geometry Dash".to_owned(),
                class: "GLFW30".to_owned(),
                visible: true,
                bounds: Bounds {
                    left: 0,
                    top: 0,
                    right: 4,
                    bottom: 2,
                },
            }])
        }

        fn processes(&self) -> io::Result<Vec<ProcessEntry>> {
            Ok(vec![ProcessEntry {
                id: ProcessId::new(3),
                parent: ProcessId::new(0),
                executable: "GeometryDash.exe".to_owned(),
                threads: 1,
            }])
        }

        fn capture(&self, window: u64, handler: FrameHandler) -> io::Result<FakeSession> {
            if self.refuse {
                return Err(io::Error::other("no capture here"));
            }
            self.handlers.send((window, handler)).expect("test listens");
            Ok(FakeSession {
                window,
                closed: self.closed.clone(),
            })
        }
    }

    struct Harness {
        owner: Owner<FakeHost>,
        messages: Receiver<Message>,
        handlers: Receiver<(u64, FrameHandler)>,
        closed: Receiver<u64>,
    }

    fn harness(refuse: bool) -> Harness {
        let (sender, messages) = mpsc::channel();
        let (handler_sender, handlers) = mpsc::channel();
        let (closed_sender, closed) = mpsc::channel();
        let host = FakeHost {
            handlers: handler_sender,
            closed: closed_sender,
            refuse,
        };
        Harness {
            owner: Owner::new(host, 99, sender),
            messages,
            handlers,
            closed,
        }
    }

    /// A 4 by 2 frame: its left half red and its right half gray.
    fn frame_image() -> Image {
        let red = [0, 0, 255, 0];
        let gray = [40, 40, 40, 0];
        let row = [red, red, gray, gray].concat();
        Image::new(
            Size {
                width: 4,
                height: 2,
            },
            [row.clone(), row].concat(),
        )
        .expect("matching length")
    }

    impl Harness {
        /// Sends `request` built around a fresh reply channel and returns the
        /// answer.
        fn ask<T>(&mut self, request: impl FnOnce(Reply<T>) -> Request) -> T {
            let (reply, answer) = oneshot::channel();
            assert!(
                self.owner
                    .handle(Message::Request(request(reply)))
                    .is_continue()
            );
            answer.blocking_recv().expect("answered")
        }

        /// Picks window 7 and returns the capture's handler.
        fn pick(&mut self) -> FrameHandler {
            self.ask(|reply| Request::Pick { window: 7, reply })
                .expect("capture starts");
            let (window, handler) = self.handlers.try_recv().expect("capture started");
            assert_eq!(window, 7);
            handler
        }

        /// Runs `handler` on `frame` and applies what it sent the owner.
        fn deliver(&mut self, handler: &mut FrameHandler, frame: io::Result<&Image>) {
            let frame = frame.map(|image| Frame {
                window_size: image.size(),
                image,
                received: Instant::now(),
                delay: Duration::ZERO,
            });
            assert_eq!(handler(frame), PREVIEW_INTERVAL);
            let message = self.messages.try_recv().expect("handler sent");
            assert!(self.owner.handle(message).is_continue());
        }
    }

    #[test]
    fn lists_windows_without_the_setup_apps_own() {
        let mut harness = harness(false);
        let list = harness.ask(Request::ListWindows).expect("the host answers");
        assert_eq!(list.windows.len(), 1);
        assert_eq!(list.windows[0].executable, "GeometryDash.exe");
    }

    #[test]
    fn previews_the_newest_frame_once_one_arrives() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        let preview = |reply| Request::Preview {
            max_width: 100,
            reply,
        };
        assert_eq!(harness.ask(preview), Ok(None));
        harness.deliver(&mut handler, Ok(&frame_image()));
        let bytes = harness.ask(preview).expect("running").expect("a frame");
        assert_eq!(bytes, pixels::rgba(&frame_image(), 100));
    }

    #[test]
    fn a_stopped_capture_explains_itself_until_another_window_is_picked() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        harness.deliver(
            &mut handler,
            Err(io::Error::other("the captured window closed")),
        );
        let preview = |reply| Request::Preview {
            max_width: 100,
            reply,
        };
        assert_eq!(
            harness.ask(preview),
            Err("the captured window closed".to_owned())
        );
        let _next = harness.pick();
        assert_eq!(harness.ask(preview), Ok(None));
    }

    #[test]
    fn frames_from_a_window_the_user_left_are_dropped() {
        let mut harness = harness(false);
        let mut first = harness.pick();
        let _second = harness.pick();
        harness.deliver(&mut first, Ok(&frame_image()));
        harness.deliver(&mut first, Err(io::Error::other("closed")));
        let preview = |reply| Request::Preview {
            max_width: 100,
            reply,
        };
        assert_eq!(harness.ask(preview), Ok(None));
    }

    #[test]
    fn a_capture_that_cannot_start_says_why() {
        let mut harness = harness(true);
        assert_eq!(
            harness.ask(|reply| Request::Pick { window: 7, reply }),
            Err("no capture here".to_owned())
        );
    }

    #[test]
    fn screenshots_keep_frames_until_another_window_is_picked() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        assert!(harness.ask(Request::Screenshot).is_err());
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Request::Screenshot), Ok(0));
        assert_eq!(harness.ask(Request::Screenshot), Ok(1));
        let pixels_of = |index| {
            move |reply| Request::ScreenshotPixels {
                index,
                max_width: 2,
                reply,
            }
        };
        assert_eq!(
            harness.ask(pixels_of(1)),
            Some(pixels::rgba(&frame_image(), 2))
        );
        assert_eq!(harness.ask(pixels_of(2)), None);
        let _next = harness.pick();
        assert_eq!(harness.ask(pixels_of(0)), None);
    }

    #[test]
    fn the_check_reads_the_live_frame() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        assert_eq!(harness.ask(Request::Reading), Ok(None));
        let spec: CheckSpec = serde_json::from_str(
            r#"{"kind":"barFill","region":{"x":0,"y":0,"width":4,"height":2},
                "filled":{"red":255,"green":0,"blue":0},"empty":{"red":40,"green":40,"blue":40}}"#,
        )
        .expect("valid JSON");
        harness
            .ask(|reply| Request::SetCheck { spec, reply })
            .expect("valid check");
        assert_eq!(harness.ask(Request::Reading), Ok(None));
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(
            harness.ask(Request::Reading),
            Ok(Some(Reading::Number(50.0)))
        );
    }

    #[test]
    fn a_region_outside_the_live_frame_is_reported() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        let spec: CheckSpec = serde_json::from_str(
            r#"{"kind":"color","region":{"x":0,"y":0,"width":4,"height":2},
                "color":{"red":255,"green":0,"blue":0},"tolerance":0}"#,
        )
        .expect("valid JSON");
        harness
            .ask(|reply| Request::SetCheck { spec, reply })
            .expect("valid check");
        let small = Image::new(
            Size {
                width: 1,
                height: 1,
            },
            vec![0, 0, 255, 0],
        )
        .expect("matching length");
        harness.deliver(&mut handler, Ok(&small));
        assert!(
            harness
                .ask(Request::Reading)
                .expect_err("outside")
                .contains("past the edge")
        );
    }

    #[test]
    fn a_check_that_cannot_be_built_says_what_to_fix() {
        let mut harness = harness(false);
        let spec: CheckSpec = serde_json::from_str(
            r#"{"kind":"color","region":{"x":0,"y":0,"width":0,"height":2},
                "color":{"red":255,"green":0,"blue":0},"tolerance":0}"#,
        )
        .expect("valid JSON");
        assert_eq!(
            harness.ask(|reply| Request::SetCheck { spec, reply }),
            Err("Draw a region on the screenshot first.".to_owned())
        );
    }

    #[test]
    fn readings_reach_the_ui_tagged_by_kind() {
        let number = serde_json::to_string(&Reading::Number(87.0)).expect("serializes");
        let flag = serde_json::to_string(&Reading::Flag(true)).expect("serializes");
        assert_eq!(number, r#"{"kind":"number","value":87.0}"#);
        assert_eq!(flag, r#"{"kind":"flag","value":true}"#);
    }

    #[test]
    fn picking_another_window_and_shutting_down_close_the_capture() {
        let mut harness = harness(false);
        let _first = harness.pick();
        assert!(harness.closed.try_recv().is_err());
        let _second = harness.pick();
        assert_eq!(harness.closed.try_recv(), Ok(7));
        let Harness {
            owner,
            messages,
            closed,
            ..
        } = harness;
        owner
            .messages
            .send(Message::Shutdown)
            .expect("owner listens");
        assert!(run(owner, &messages).is_ok());
        assert_eq!(closed.try_recv(), Ok(7));
    }

    #[test]
    fn the_host_system_lists_and_captures_through_the_platform() {
        let unsupported = appytappy_sources::system::Unsupported;
        assert!(Host::windows(&unsupported).is_err());
        assert!(Host::processes(&unsupported).is_err());
        let handler: FrameHandler = Box::new(|_frame| Duration::ZERO);
        assert!(unsupported.capture(7, handler).is_err());
    }
}
