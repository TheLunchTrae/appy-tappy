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

use appytappy_config::app_file::{
    AntiCheatDef, AppFile, FORMAT, ScreenDef, SizeDef, ValueDef, WindowDef, value_id,
};
use appytappy_config::signal::Value;
use appytappy_config::store::{AppStore, new_app_id};
use appytappy_sources::screen::{Capture, CaptureSession, Crop, FrameHandler, Image};
use appytappy_sources::system::{ProcessEntry, System, WindowEntry};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::check_spec::{self, CheckSpec, RegionCheck};
use crate::pixels;
use crate::window_list::{self, WindowList};

/// The shortest time between two frames the preview takes: about 30 a
/// second, smooth to the eye at a fraction of an application's frame rate.
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

/// A value to save, as the UI describes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SaveSpec {
    /// The saved application to add the value to, or `None` for a new one
    /// named `name` and recognized by `window`.
    pub(crate) app: Option<String>,
    /// A new application's name.
    pub(crate) name: String,
    /// How to recognize a new application's window.
    pub(crate) window: WindowDef,
    /// The value's name, such as `Progress`.
    pub(crate) value_name: String,
    /// The check, with the session's screenshots it names.
    pub(crate) check: CheckSpec,
}

/// Where a value was saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SavedValue {
    /// The application's ID.
    pub(crate) app: String,
    /// The value's ID.
    pub(crate) value: String,
}

/// A saved application, as the Values page lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppSummary {
    /// The application's ID.
    pub(crate) id: String,
    /// The file, or `None` when it is broken.
    pub(crate) file: Option<AppFile>,
    /// Why the file is broken.
    pub(crate) error: Option<String>,
    /// The handles of the running windows that match, the best fit first.
    pub(crate) windows: Vec<u64>,
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
    /// The handles of the running windows a window match finds, the best
    /// fit first.
    MatchWindows {
        /// How to recognize the window.
        window: WindowDef,
        /// The handles.
        reply: Reply<Result<Vec<u64>, String>>,
    },
    /// The saved applications.
    ListApps(Reply<Result<Vec<AppSummary>, String>>),
    /// Saves a value.
    SaveValue {
        /// The value.
        spec: Box<SaveSpec>,
        /// Where it was saved, or what to fix.
        reply: Reply<Result<SavedValue, String>>,
    },
    /// Removes a saved value and the pictures only it used.
    DeleteValue {
        /// The application's ID.
        app: String,
        /// The value's ID.
        value: String,
        /// Answered once it is gone.
        reply: Reply<Result<(), String>>,
    },
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
    /// Where the applications' files live, or why that is unknown.
    store: Result<AppStore, String>,
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
    pub(crate) const fn new(
        host: H,
        store: Result<AppStore, String>,
        own_process: u32,
        messages: Sender<Message>,
    ) -> Self {
        Self {
            host,
            store,
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
            Request::MatchWindows { window, reply } => send(
                reply,
                self.window_list()
                    .map(|list| window_list::matching(&list.windows, &window)),
            ),
            Request::ListApps(reply) => send(reply, self.apps()),
            Request::SaveValue { spec, reply } => send(reply, self.save(&spec)),
            Request::DeleteValue { app, value, reply } => {
                send(reply, self.delete(&app, &value));
            },
        }
    }

    fn store(&self) -> Result<&AppStore, String> {
        self.store.as_ref().map_err(Clone::clone)
    }

    fn apps(&self) -> Result<Vec<AppSummary>, String> {
        let stored = self.store()?.load().map_err(|error| error.to_string())?;
        let windows = self.window_list()?.windows;
        Ok(stored
            .into_iter()
            .map(|app| {
                let (file, error) = match app.file {
                    Ok(file) => (Some(file), None),
                    Err(error) => (None, Some(error)),
                };
                let matched = file
                    .as_ref()
                    .map(|file| window_list::matching(&windows, &file.window))
                    .unwrap_or_default();
                AppSummary {
                    id: app.id,
                    file,
                    error,
                    windows: matched,
                }
            })
            .collect())
    }

    fn save(&self, spec: &SaveSpec) -> Result<SavedValue, String> {
        let store = self.store()?;
        let name = spec.value_name.trim();
        if name.is_empty() {
            return Err("Name the value first.".to_owned());
        }
        let size = self
            .latest
            .as_ref()
            .or_else(|| self.screenshots.last())
            .map(Image::size)
            .ok_or_else(|| "No frame has arrived from the window yet.".to_owned())?;
        let loaded = store.load().map_err(|error| error.to_string())?;
        let (app, mut file) = if let Some(id) = &spec.app {
            let file = loaded
                .into_iter()
                .find(|app| &app.id == id)
                .ok_or_else(|| "That application is no longer saved.".to_owned())?
                .file?;
            (id.clone(), file)
        } else {
            let taken: Vec<&str> = loaded.iter().map(|app| app.id.as_str()).collect();
            let file = AppFile {
                format: FORMAT,
                name: spec.name.trim().to_owned(),
                window: spec.window.clone(),
                anti_cheat: AntiCheatDef::Unknown,
                values: Vec::new(),
            };
            (new_app_id(&spec.window.executable, &taken), file)
        };
        let id = value_id(name);
        if file.values.iter().any(|value| value.id == id) {
            return Err(format!(
                "{} already has a value named like \"{name}\". Pick another name.",
                file.name
            ));
        }
        let saved = check_spec::to_saved(&spec.check, &self.screenshots, &id)
            .map_err(|error| error.to_string())?;
        file.values.push(ValueDef {
            id: id.clone(),
            name: name.to_owned(),
            screen: ScreenDef {
                window_size: SizeDef {
                    width: size.width,
                    height: size.height,
                },
                region: saved.region,
                check: saved.check,
            },
        });
        store
            .save(&app, &file, &saved.pictures)
            .map_err(|error| error.to_string())?;
        Ok(SavedValue { app, value: id })
    }

    fn delete(&self, app: &str, value: &str) -> Result<(), String> {
        let store = self.store()?;
        let mut file = store
            .load()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|stored| stored.id == app)
            .ok_or_else(|| "That application is no longer saved.".to_owned())?
            .file?;
        file.values.retain(|saved| saved.id != value);
        store
            .save(app, &file, &[])
            .and_then(|()| store.prune(app, &file))
            .map_err(|error| error.to_string())
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
                title: "Example App".to_owned(),
                class: "ExampleWindow".to_owned(),
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
                executable: "Example.exe".to_owned(),
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

    /// A fresh, empty store folder for one test.
    fn scratch_store(name: &str) -> AppStore {
        let folder =
            std::env::temp_dir().join(format!("appytappy-setup-{name}-{}", std::process::id()));
        if folder.exists() {
            std::fs::remove_dir_all(&folder).expect("remove leftovers");
        }
        AppStore::new(folder)
    }

    fn harness(refuse: bool) -> Harness {
        harness_with(refuse, Err("no store in this test".to_owned()))
    }

    fn harness_with(refuse: bool, store: Result<AppStore, String>) -> Harness {
        let (sender, messages) = mpsc::channel();
        let (handler_sender, handlers) = mpsc::channel();
        let (closed_sender, closed) = mpsc::channel();
        let host = FakeHost {
            handlers: handler_sender,
            closed: closed_sender,
            refuse,
        };
        Harness {
            owner: Owner::new(host, store, 99, sender),
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
        assert_eq!(list.windows[0].executable, "Example.exe");
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

    fn spec(app: Option<&str>, value_name: &str, check: &str) -> Box<SaveSpec> {
        Box::new(SaveSpec {
            app: app.map(str::to_owned),
            name: " Example App ".to_owned(),
            window: WindowDef::exact(appytappy_config::window_match::WindowFacts {
                title: "Example App",
                class: "ExampleWindow",
                executable: "Example.exe",
            }),
            value_name: value_name.to_owned(),
            check: serde_json::from_str(check).expect("valid JSON"),
        })
    }

    const REFERENCE: &str = r#"{"kind":"reference","region":{"x":0,"y":0,"width":2,"height":2},
        "screenshot":0,"tolerance":0}"#;
    const BAR: &str = r#"{"kind":"barFill","region":{"x":0,"y":0,"width":4,"height":2},
        "filled":{"red":255,"green":0,"blue":0},"empty":{"red":40,"green":40,"blue":40}}"#;

    #[test]
    fn saved_values_are_listed_with_their_running_windows_and_can_be_removed() {
        let store = scratch_store("save");
        let root = store.folder("");
        let mut harness = harness_with(false, Ok(store));
        let mut handler = harness.pick();
        let save = |spec| move |reply| Request::SaveValue { spec, reply };
        assert_eq!(
            harness.ask(save(spec(None, "In menu", REFERENCE))),
            Err("No frame has arrived from the window yet.".to_owned())
        );
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Request::Screenshot), Ok(0));
        assert_eq!(
            harness.ask(save(spec(None, "  ", REFERENCE))),
            Err("Name the value first.".to_owned())
        );
        let saved = harness
            .ask(save(spec(None, "In menu", REFERENCE)))
            .expect("saves a new app");
        assert_eq!(
            saved,
            SavedValue {
                app: "example".to_owned(),
                value: "in_menu".to_owned()
            }
        );
        harness
            .ask(save(spec(Some("example"), "Progress bar", BAR)))
            .expect("adds to the app");
        assert_eq!(
            harness.ask(save(spec(Some("example"), "Progress BAR", BAR))),
            Err(
                "Example App already has a value named like \"Progress BAR\". Pick another name."
                    .to_owned()
            )
        );
        assert_eq!(
            harness.ask(save(spec(Some("gone"), "x", BAR))),
            Err("That application is no longer saved.".to_owned())
        );

        let apps = harness.ask(Request::ListApps).expect("lists");
        assert_eq!(apps.len(), 1);
        let app = &apps[0];
        assert_eq!((app.id.as_str(), &app.error), ("example", &None));
        assert_eq!(app.windows, [7]);
        let file = app.file.as_ref().expect("a good file");
        assert_eq!(file.name, "Example App");
        let ids: Vec<&str> = file.values.iter().map(|value| value.id.as_str()).collect();
        assert_eq!(ids, ["in_menu", "progress_bar"]);
        assert_eq!(
            file.values[0].screen.window_size,
            SizeDef {
                width: 4,
                height: 2
            }
        );
        let folder = root.join("example");
        assert!(folder.join("in_menu.png").exists());

        harness
            .ask(|reply| Request::DeleteValue {
                app: "example".to_owned(),
                value: "in_menu".to_owned(),
                reply,
            })
            .expect("removes");
        assert!(!folder.join("in_menu.png").exists());
        let apps = harness.ask(Request::ListApps).expect("lists");
        let file = apps[0].file.as_ref().expect("a good file");
        assert_eq!(file.values.len(), 1);
        assert_eq!(
            harness.ask(|reply| Request::DeleteValue {
                app: "gone".to_owned(),
                value: "x".to_owned(),
                reply,
            }),
            Err("That application is no longer saved.".to_owned())
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn without_a_store_saving_and_listing_say_why() {
        let mut harness = harness(false);
        assert_eq!(
            harness.ask(Request::ListApps),
            Err("no store in this test".to_owned())
        );
        assert_eq!(
            harness.ask(|reply| Request::SaveValue {
                spec: spec(None, "x", BAR),
                reply
            }),
            Err("no store in this test".to_owned())
        );
        assert_eq!(
            harness.ask(|reply| Request::DeleteValue {
                app: "a".to_owned(),
                value: "b".to_owned(),
                reply
            }),
            Err("no store in this test".to_owned())
        );
    }

    #[test]
    fn a_window_match_finds_the_running_window() {
        let mut harness = harness(false);
        let window = WindowDef::exact(appytappy_config::window_match::WindowFacts {
            title: "Example App",
            class: "ExampleWindow",
            executable: "Example.exe",
        });
        assert_eq!(
            harness.ask(|reply| Request::MatchWindows { window, reply }),
            Ok(vec![7])
        );
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
