//! The thread that owns the setup app's state.
//!
//! Tauri runs commands on its own threads, and the capture hands frames over
//! on another. None of them touches the state: each sends a [`Message`] here,
//! and a request carries a oneshot channel the owner answers on. The state is
//! the picked window's capture, its newest frame, the session's screenshots,
//! and the check the user is setting up.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::ops::ControlFlow;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use appytappy_config::app_file::{
    AntiCheatDef, AppFile, FORMAT, OverlayDef, OverlayModeDef, PanelDef, ScreenDef, SizeDef,
    ValueDef, WindowDef, value_id,
};
use appytappy_config::automation_file::{AutomationDef, WhenDef};
use appytappy_config::signal::{SignalName, Value};
use appytappy_config::store::new_app_id;
use appytappy_sources::screen::check::Check;
use appytappy_sources::screen::digits::DigitReader;
use appytappy_sources::screen::saved;
use appytappy_sources::screen::{
    Capture, CaptureSession, Crop, Frame, FrameHandler, Image, OutsideWindow, Region, Size,
};
use appytappy_sources::system::{ProcessEntry, System, WindowEntry, WindowPlace, best_window};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::automations::{self, AutomationSummary, Stores};
use crate::check_spec::{self, CheckSpec, ColorSpec, RecordedLabel, RegionCheck, RegionSpec};
use crate::ocr;
use crate::overlay::{self, Card, FrameRead, OverlayView, OverlayWindow, Overlays, Place};
use crate::pixels;
use crate::recording::{Recording, RecordingView, Text};
use crate::window_list::{self, WindowList};

/// The shortest time between two frames the preview takes: about 30 a
/// second, smooth to the eye at a fraction of an application's frame rate.
const PREVIEW_INTERVAL: Duration = Duration::from_millis(33);

/// The shortest time between two frames the automation editor's live reading
/// takes: often enough for a reading it shows about twice a second.
const WATCH_INTERVAL: Duration = Duration::from_millis(250);

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

    /// Where `window` is and whether it shows, or `None` once it closed.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn window_place(&self, window: u64) -> io::Result<Option<WindowPlace>>;

    /// Starts capturing `crop` of `window`, calling `handler` with frames at
    /// most every `interval`.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error when the capture cannot start.
    fn capture(
        &self,
        window: u64,
        crop: Crop,
        interval: Duration,
        handler: FrameHandler,
    ) -> io::Result<Self::Session>;
}

impl<T: System + Capture> Host for T {
    type Session = T::Session;

    fn windows(&self) -> io::Result<Vec<WindowEntry>> {
        System::windows(self)
    }

    fn processes(&self) -> io::Result<Vec<ProcessEntry>> {
        System::processes(self)
    }

    fn window_place(&self, window: u64) -> io::Result<Option<WindowPlace>> {
        System::window_place(self, window)
    }

    fn capture(
        &self,
        window: u64,
        crop: Crop,
        interval: Duration,
        handler: FrameHandler,
    ) -> io::Result<Self::Session> {
        self.start(window, crop, interval, handler)
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

/// What a watched value reads on the newest frame, for the automation
/// editor.
#[derive(Debug, Clone, PartialEq, Serialize)]
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

/// The saved value the automation editor watches.
#[derive(Debug)]
struct Watched {
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
    seen: Option<Seen>,
    /// Why the capture stopped, once it has.
    failure: Option<String>,
}

/// What a frame of the watched value's window showed.
#[derive(Debug)]
pub(crate) enum Seen {
    /// The value's region, on a window of `window_size`.
    Region {
        /// The window's size.
        window_size: Size,
        /// The region's pixels.
        image: Image,
    },
    /// A window of this size, which the region lies outside.
    Outside(Size),
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
    /// The screenshot the region was drawn on, whose size the value keeps.
    pub(crate) screenshot: usize,
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
    /// Stops capturing and drops the session's screenshots and check, once
    /// the user leaves the capture screen.
    Stop(Reply<()>),
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
    /// Records `region` of the picked window, drawn on `screenshot`, keeping
    /// each reading it shows. Goes on with a stopped recording of the same
    /// region and ink; otherwise starts a new one in its place.
    StartRecording {
        /// The region to record.
        region: RegionSpec,
        /// The characters' color.
        ink: ColorSpec,
        /// How far a pixel may be from `ink` and still be ink.
        tolerance: u8,
        /// The screenshot the region was drawn on.
        screenshot: usize,
        /// Answered once it records, or with why it cannot.
        reply: Reply<Result<(), String>>,
    },
    /// Stops the recording, keeping its readings for review.
    StopRecording(Reply<()>),
    /// Forgets the recording and its readings.
    DiscardRecording(Reply<()>),
    /// The recording as the UI shows it: `None` without one.
    RecordingState(Reply<Option<RecordingView>>),
    /// A recorded reading's pixels, scaled and encoded like a preview.
    RecordedPixels {
        /// The reading's index.
        index: usize,
        /// The widest the UI draws it.
        max_width: u32,
        /// The pixels, or `None` for a reading that does not exist.
        reply: Reply<Option<Vec<u8>>>,
    },
    /// What a number check would make of each labeled reading, learned in
    /// order: `None` for one it learns, or why it cannot, such as a
    /// character that an earlier reading showed with another shape.
    ReviewProblems {
        /// The readings with what they show.
        readings: Vec<RecordedLabel>,
        /// One answer per reading, in order.
        reply: Reply<Vec<Option<String>>>,
    },
    /// Captures the region of a saved value on its application's window,
    /// for the automation editor. Like [`Request::Pick`], it drops the
    /// previous window's capture, screenshots and check.
    WatchValue {
        /// The application's ID.
        app: String,
        /// The value's ID.
        value: String,
        /// The watch's number once the capture started, or why it cannot.
        reply: Reply<Result<u64, String>>,
    },
    /// What the watched value reads on the newest frame, and whether `when`
    /// holds for it; an error once the watch was stopped or replaced.
    WatchedReading {
        /// The watch's number.
        watch: u64,
        /// The condition to test, which must watch the watched value.
        when: Option<Box<WhenDef>>,
        /// The reading.
        reply: Reply<Result<Live, String>>,
    },
    /// Stops the watch, unless another capture replaced it already.
    StopWatching {
        /// The watch's number.
        watch: u64,
        /// Answered once it stopped.
        reply: Reply<()>,
    },
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
    /// Removes a saved value and the pictures only it used, unless an
    /// automation watches it.
    DeleteValue {
        /// The application's ID.
        app: String,
        /// The value's ID.
        value: String,
        /// Answered once it is gone.
        reply: Reply<Result<(), String>>,
    },
    /// Sets how an application protects itself from other programs.
    SetAntiCheat {
        /// The application's ID.
        app: String,
        /// Its anti-cheat class.
        class: AntiCheatDef,
        /// Answered once it is saved.
        reply: Reply<Result<(), String>>,
    },
    /// The saved automations.
    ListAutomations(Reply<Result<Vec<AutomationSummary>, String>>),
    /// Saves an automation.
    SaveAutomation {
        /// The automation to replace, or `None` for a new one.
        existing: Option<String>,
        /// The automation; its ID is set on saving.
        automation: Box<AutomationDef>,
        /// Its ID, or what to fix.
        reply: Reply<Result<String, String>>,
    },
    /// Turns an automation on or off.
    SetAutomationEnabled {
        /// The automation's ID.
        id: String,
        /// Whether it runs.
        enabled: bool,
        /// Answered once it is saved.
        reply: Reply<Result<(), String>>,
    },
    /// Removes an automation.
    DeleteAutomation {
        /// The automation's ID.
        id: String,
        /// Answered once it is gone.
        reply: Reply<Result<(), String>>,
    },
    /// The overlays that are on, for their windows to open.
    Overlays(Reply<Result<Vec<OverlayWindow>, String>>),
    /// Turns an application's overlay on or off and sets where it shows,
    /// keeping where the user put its panel.
    SetOverlay {
        /// The application's ID.
        app: String,
        /// Whether it shows.
        enabled: bool,
        /// Where it shows.
        mode: OverlayModeDef,
        /// Answered once it is saved.
        reply: Reply<Result<(), String>>,
    },
    /// What an application's overlay shows now.
    OverlayState {
        /// The application's ID.
        app: String,
        /// What it shows, or why it cannot.
        reply: Reply<Result<OverlayView, String>>,
    },
    /// Where an application's overlay goes over its window now, or `None`
    /// while it should not show.
    OverlayPlace {
        /// The application's ID.
        app: String,
        /// Where it goes.
        reply: Reply<Option<Place>>,
    },
    /// The size an application's overlay card takes, as its page measured
    /// it.
    OverlayCard {
        /// The application's ID.
        app: String,
        /// The card's size.
        card: Card,
        /// Answered once it is kept.
        reply: Reply<()>,
    },
    /// Stops an application's overlay once its window closed.
    CloseOverlay {
        /// The application's ID.
        app: String,
        /// The mode of the window that closed. An overlay open in the other
        /// mode belongs to the window that replaced it, and stays.
        mode: OverlayModeDef,
        /// Answered once it stopped.
        reply: Reply<()>,
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
        /// When it arrived.
        received: Instant,
    },
    /// The text reader read reading `index` of recording `recording`.
    Recognized {
        /// Which recording it belongs to; text for an earlier one is
        /// dropped.
        recording: u64,
        /// The reading's index.
        index: usize,
        /// What it read.
        text: Text,
    },
    /// The watch with this generation saw a frame, or stopped and why.
    Watch {
        /// Which capture sent it; frames from an earlier one are dropped.
        generation: u64,
        /// What the frame showed, or why the capture stopped.
        seen: Result<Seen, String>,
    },
    /// The capture with this generation stopped, such as when the window
    /// closed.
    CaptureFailed {
        /// Which capture stopped.
        generation: u64,
        /// Why, for the user.
        error: String,
    },
    /// An application's overlay read its values, or stopped and why.
    Overlay {
        /// The application's ID.
        app: String,
        /// Which of its captures sent it; readings from an earlier one are
        /// dropped.
        generation: u64,
        /// What the frame read, or why the capture stopped.
        read: Result<FrameRead, String>,
    },
    /// The user moved or resized an application's picture-in-picture
    /// panel.
    PanelPlaced {
        /// The application's ID.
        app: String,
        /// Where it is now.
        panel: PanelDef,
    },
    /// The app is closing.
    Shutdown,
}

/// The setup app's state.
#[derive(Debug)]
pub(crate) struct Owner<H: Host> {
    host: H,
    /// Where the applications' and automations' files live, or why that is
    /// unknown.
    stores: Result<Stores, String>,
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
    /// The region the user records, with the readings it showed.
    recording: Option<Recording>,
    /// Counts recordings, so text read for one the user discarded or
    /// replaced is dropped.
    recording_id: u64,
    /// Sends new readings to the text reader's thread.
    ocr: Sender<ocr::Job>,
    /// The saved value the automation editor watches.
    watched: Option<Watched>,
    /// The overlays that show, each with a capture of its own.
    overlays: Overlays<H::Session>,
}

impl<H: Host> Owner<H> {
    /// An owner with no window picked.
    pub(crate) const fn new(
        host: H,
        stores: Result<Stores, String>,
        own_process: u32,
        messages: Sender<Message>,
        ocr: Sender<ocr::Job>,
    ) -> Self {
        Self {
            host,
            stores,
            own_process,
            messages,
            generation: 0,
            session: None,
            latest: None,
            failure: None,
            screenshots: Vec::new(),
            check: None,
            recording: None,
            recording_id: 0,
            ocr,
            watched: None,
            overlays: Overlays::new(),
        }
    }

    /// Applies one message; breaks on [`Message::Shutdown`].
    pub(crate) fn handle(&mut self, message: Message) -> ControlFlow<()> {
        match message {
            Message::Request(request) => self.answer(request),
            Message::Frame {
                generation,
                image,
                received,
            } => {
                if generation == self.generation {
                    self.record(&image, received);
                    self.latest = Some(image);
                }
            },
            Message::Recognized {
                recording,
                index,
                text,
            } => {
                if let Some(current) = self
                    .recording
                    .as_mut()
                    .filter(|current| current.id == recording)
                {
                    current.read(index, text);
                }
            },
            Message::CaptureFailed { generation, error } => {
                if generation == self.generation {
                    self.failure = Some(error);
                }
            },
            Message::Watch { generation, seen } => {
                if let Some(watched) = self
                    .watched
                    .as_mut()
                    .filter(|_| generation == self.generation)
                {
                    match seen {
                        Ok(seen) => watched.seen = Some(seen),
                        Err(error) => {
                            watched.failure = Some(error);
                            // The editor watches again, so nothing needs this
                            // capture, even if it would go on.
                            #[expect(
                                clippy::let_underscore_must_use,
                                reason = "the capture already failed, and the editor shows why"
                            )]
                            let _ = self.close();
                        },
                    }
                }
            },
            Message::Overlay {
                app,
                generation,
                read,
            } => {
                // The overlay shows why its capture stopped, and looks for
                // the window again.
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the capture already failed, and the overlay shows why"
                )]
                let _ = self.overlays.read(&app, generation, read);
            },
            Message::PanelPlaced { app, panel } => self.overlays.placed(&app, panel),
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

    /// Saves where each panel is, closes the overlays, and stops the
    /// capture, as the app ends.
    ///
    /// # Errors
    ///
    /// Returns the error the capture's thread ended with. An overlay's
    /// capture that ended with an error already showed it.
    pub(crate) fn shut_down(&mut self) -> io::Result<()> {
        self.save_panels(true);
        #[expect(
            clippy::let_underscore_must_use,
            reason = "an overlay's capture error already showed in its window, which is closing"
        )]
        let _ = self.overlays.close(None);
        self.close()
    }

    fn answer(&mut self, request: Request) {
        let saves = request.saves();
        if request.changes_values() {
            // Each overlay reads its values again, from their saved files,
            // after keeping where the user just put its panel.
            self.save_panels(true);
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a capture that ended with an error stopped already, and the next poll starts another"
            )]
            let _ = self.overlays.close(None);
        }
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
                check_spec::build(&spec, &self.screenshots, self.recording.as_ref())
                    .map(|check| self.check = Some(check))
                    .map_err(|error| error.to_string()),
            ),
            Request::Stop(reply) => {
                self.stop();
                send(reply, ());
            },
            Request::Reading(reply) => send(reply, self.reading()),
            Request::StartRecording {
                region,
                ink,
                tolerance,
                screenshot,
                reply,
            } => send(
                reply,
                self.start_recording(region, ink, tolerance, screenshot),
            ),
            Request::StopRecording(reply) => {
                if let Some(recording) = &mut self.recording {
                    recording.stop(Instant::now());
                }
                send(reply, ());
            },
            Request::DiscardRecording(reply) => {
                self.recording = None;
                send(reply, ());
            },
            Request::RecordingState(reply) => send(
                reply,
                self.recording
                    .as_ref()
                    .map(|recording| recording.view(Instant::now())),
            ),
            Request::RecordedPixels {
                index,
                max_width,
                reply,
            } => send(
                reply,
                self.recording
                    .as_ref()
                    .and_then(|recording| recording.readings().get(index))
                    .map(|kept| pixels::rgba(&kept.image, max_width)),
            ),
            Request::ReviewProblems { readings, reply } => {
                send(reply, self.review_problems(&readings));
            },
            Request::WatchValue { app, value, reply } => send(reply, self.watch(&app, &value)),
            Request::WatchedReading { watch, when, reply } => {
                send(reply, self.watched_reading(watch, when.as_deref()));
            },
            Request::StopWatching { watch, reply } => {
                if self.watched.is_some() && watch == self.generation {
                    self.stop();
                }
                send(reply, ());
            },
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
            Request::SetAntiCheat { app, class, reply } => {
                send(reply, self.set_anti_cheat(&app, class));
            },
            Request::ListAutomations(reply) => {
                send(reply, self.stores().and_then(automations::list));
            },
            Request::SaveAutomation {
                existing,
                automation,
                reply,
            } => send(
                reply,
                self.stores()
                    .and_then(|stores| automations::save(stores, existing.as_deref(), *automation)),
            ),
            Request::SetAutomationEnabled { id, enabled, reply } => send(
                reply,
                self.stores()
                    .and_then(|stores| automations::set_enabled(stores, &id, enabled)),
            ),
            Request::DeleteAutomation { id, reply } => send(
                reply,
                self.stores()
                    .and_then(|stores| automations::delete(stores, &id)),
            ),
            Request::Overlays(reply) => send(reply, self.overlay_windows()),
            Request::SetOverlay {
                app,
                enabled,
                mode,
                reply,
            } => send(reply, self.set_overlay(&app, enabled, mode)),
            Request::OverlayState { app, reply } => {
                let view = self.overlay_view(&app);
                self.save_panels(false);
                send(reply, view);
            },
            Request::OverlayPlace { app, reply } => {
                send(reply, self.overlays.place(&self.host, &app));
            },
            Request::OverlayCard { app, card, reply } => {
                self.overlays.card(&app, card);
                send(reply, ());
            },
            Request::CloseOverlay { app, mode, reply } => {
                if self.overlays.mode(&app) == Some(mode) {
                    self.save_panels(true);
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "the overlay's window closed, so nothing shows the capture's error"
                    )]
                    let _ = self.overlays.close(Some(&app));
                }
                send(reply, ());
            },
        }
        if saves {
            tell_runtime();
        }
    }

    /// The saved application `app`, from its file.
    fn app_file(&self, app: &str) -> Result<AppFile, String> {
        self.stores()?
            .apps
            .load()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|stored| stored.id == app)
            .ok_or_else(|| "That application is no longer saved.".to_owned())?
            .file
    }

    fn overlay_windows(&self) -> Result<Vec<OverlayWindow>, String> {
        let loaded = self
            .stores()?
            .apps
            .load()
            .map_err(|error| error.to_string())?;
        Ok(overlay::windows(loaded.iter().filter_map(|stored| {
            Some((stored.id.as_str(), stored.file.as_ref().ok()?))
        })))
    }

    fn set_overlay(&self, app: &str, enabled: bool, mode: OverlayModeDef) -> Result<(), String> {
        let mut file = self.app_file(app)?;
        let panel = file.overlay.and_then(|overlay| overlay.panel);
        file.overlay = Some(OverlayDef {
            enabled,
            mode,
            panel,
        });
        self.stores()?
            .apps
            .save(app, &file, &[])
            .map_err(|error| error.to_string())
    }

    fn overlay_view(&mut self, app: &str) -> Result<OverlayView, String> {
        let stores = self.stores.as_ref().map_err(Clone::clone);
        self.overlays.view(&self.host, &self.messages, app, || {
            let stores = stores?;
            let file = stores
                .apps
                .load()
                .map_err(|error| error.to_string())?
                .into_iter()
                .find(|stored| stored.id == app)
                .ok_or_else(|| "That application is no longer saved.".to_owned())?
                .file?;
            Ok((file, stores.apps.folder(app)))
        })
    }

    /// Saves where the user put each panel, once it settled or, with `all`,
    /// right away. A place that cannot be saved is lost, and the panel opens
    /// where it last was saved.
    fn save_panels(&mut self, all: bool) {
        for (app, panel) in self.overlays.settled(all) {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a panel's place is a convenience; the panel still shows"
            )]
            let _ = self.save_panel(&app, panel);
        }
    }

    fn save_panel(&self, app: &str, panel: PanelDef) -> Result<(), String> {
        let mut file = self.app_file(app)?;
        let Some(overlay) = &mut file.overlay else {
            return Ok(());
        };
        overlay.panel = Some(panel);
        self.stores()?
            .apps
            .save(app, &file, &[])
            .map_err(|error| error.to_string())
    }

    fn stores(&self) -> Result<&Stores, String> {
        self.stores.as_ref().map_err(Clone::clone)
    }

    fn apps(&self) -> Result<Vec<AppSummary>, String> {
        let stored = self
            .stores()?
            .apps
            .load()
            .map_err(|error| error.to_string())?;
        // The saved files read fine without the running windows, so a
        // failed window listing only leaves each one's windows empty.
        let windows = self
            .window_list()
            .map(|list| list.windows)
            .unwrap_or_default();
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
        let store = &self.stores()?.apps;
        let name = spec.value_name.trim();
        if name.is_empty() {
            return Err("Name the value first.".to_owned());
        }
        // A picture check's region was drawn on the screenshot it keeps.
        let drawn_on = match &spec.check {
            CheckSpec::Reference { screenshot, .. } => *screenshot,
            _ => spec.screenshot,
        };
        let size = self
            .screenshots
            .get(drawn_on)
            .map(Image::size)
            .ok_or_else(|| "Take a screenshot first.".to_owned())?;
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
                overlay: None,
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
        let saved =
            check_spec::to_saved(&spec.check, &self.screenshots, self.recording.as_ref(), &id)
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
        let stores = self.stores()?;
        let store = &stores.apps;
        let mut file = store
            .load()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|stored| stored.id == app)
            .ok_or_else(|| "That application is no longer saved.".to_owned())?
            .file?;
        if let Some(saved) = file.values.iter().find(|saved| saved.id == value) {
            automations::check_unused(stores, app, value, &saved.name)?;
        }
        file.values.retain(|saved| saved.id != value);
        store
            .save(app, &file, &[])
            .and_then(|()| store.prune(app, &file))
            .map_err(|error| error.to_string())
    }

    /// Saves `class` as the anti-cheat class of the application `app`. A
    /// class other than none stops its automations that press keys, which
    /// the Automations page then shows as unable to run.
    fn set_anti_cheat(&self, app: &str, class: AntiCheatDef) -> Result<(), String> {
        let store = &self.stores()?.apps;
        let mut file = store
            .load()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|stored| stored.id == app)
            .ok_or_else(|| "That application is no longer saved.".to_owned())?
            .file?;
        file.anti_cheat = class;
        store
            .save(app, &file, &[])
            .map_err(|error| error.to_string())
    }

    fn window_list(&self) -> Result<WindowList, String> {
        let windows = self.host.windows().map_err(|error| error.to_string())?;
        let processes = self.host.processes().map_err(|error| error.to_string())?;
        Ok(window_list::list(windows, &processes, self.own_process))
    }

    /// Stops the capture and forgets its frames, screenshots, recording, and
    /// check.
    fn stop(&mut self) {
        // A capture that failed already said why; what comes next matters now.
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the old capture's error is moot once the user moved on"
        )]
        let _ = self.close();
        self.generation = self.generation.wrapping_add(1);
        self.latest = None;
        self.failure = None;
        self.screenshots.clear();
        self.check = None;
        self.recording = None;
        self.watched = None;
    }

    fn pick(&mut self, window: u64) -> Result<(), String> {
        self.start(window, Crop::Whole, PREVIEW_INTERVAL, preview_message)
            .map(drop)
    }

    /// Stops the capture and starts capturing `crop` of `window`, sending
    /// each frame as `to_message` makes it. Answers the new capture's
    /// generation.
    fn start(
        &mut self,
        window: u64,
        crop: Crop,
        interval: Duration,
        to_message: fn(u64, io::Result<Frame<'_>>) -> Message,
    ) -> Result<u64, String> {
        self.stop();
        let generation = self.generation;
        let messages = self.messages.clone();
        let handler: FrameHandler = Box::new(move |frame| {
            // The owner stopped when the send fails, and the capture is
            // about to be stopped too, so the wait does not matter.
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a failed send means the owner stopped and will close this capture"
            )]
            let _ = messages.send(to_message(generation, frame));
            interval
        });
        let session = self
            .host
            .capture(window, crop, interval, handler)
            .map_err(|error| error.to_string())?;
        self.session = Some(session);
        Ok(generation)
    }

    fn screenshot(&mut self) -> Result<usize, String> {
        let image = self
            .latest
            .clone()
            .ok_or_else(|| "No frame has arrived from the window yet.".to_owned())?;
        self.screenshots.push(image);
        Ok(self.screenshots.len().saturating_sub(1))
    }

    /// Records `frame`, which arrived at `received`, and sends a new reading
    /// it shows to the text reader.
    fn record(&mut self, frame: &Image, received: Instant) {
        let Some(recording) = &mut self.recording else {
            return;
        };
        let Some(new) = recording.offer(frame, received) else {
            return;
        };
        let job = ocr::Job {
            recording: recording.id,
            index: new.index,
            line: new.line,
        };
        if self.ocr.send(job).is_err() {
            recording.read(
                new.index,
                Text::Failed("The text reader stopped. Type what this shows.".to_owned()),
            );
        }
    }

    fn start_recording(
        &mut self,
        region: RegionSpec,
        ink: ColorSpec,
        tolerance: u8,
        screenshot: usize,
    ) -> Result<(), String> {
        if self.session.is_none() {
            return Err("Pick a window to record.".to_owned());
        }
        let region = Region::new(region.x, region.y, region.width, region.height)
            .ok_or_else(|| "Draw the region to record first.".to_owned())?;
        let drawn_on = self
            .screenshots
            .get(screenshot)
            .ok_or_else(|| "Take a screenshot first.".to_owned())?;
        if drawn_on.crop(region).is_none() {
            return Err(
                "The region reaches past the edge of the window. Draw it inside the window."
                    .to_owned(),
            );
        }
        let window_size = drawn_on.size();
        let reader = DigitReader::new(ink.into(), tolerance);
        let now = Instant::now();
        match &mut self.recording {
            Some(recording) if recording.records(region, &reader, window_size) => {
                recording.resume(now);
            },
            _ => {
                self.recording_id = self.recording_id.wrapping_add(1);
                self.recording = Some(Recording::new(
                    self.recording_id,
                    region,
                    reader,
                    window_size,
                    now,
                ));
            },
        }
        Ok(())
    }

    /// Learns `readings` in order on a check of their own, and answers why
    /// each one it cannot learn fails. One that fails is left out, so the
    /// ones after it are judged on the ones that worked.
    fn review_problems(&self, readings: &[RecordedLabel]) -> Vec<Option<String>> {
        let Some(recording) = &self.recording else {
            return vec![
                Some("That recording is gone. Record the region again.".to_owned());
                readings.len()
            ];
        };
        let mut reader = recording.reader().clone();
        readings
            .iter()
            .map(|label| {
                let Some(kept) = recording.readings().get(label.reading) else {
                    return Some("That reading is gone. Record the region again.".to_owned());
                };
                let image = &kept.image;
                let mut trial = reader.clone();
                match trial.learn(image, &label.text) {
                    Ok(()) => {
                        reader = trial;
                        None
                    },
                    Err(error) => Some(sentence(&error.to_string())),
                }
            })
            .collect()
    }

    fn reading(&self) -> Result<Option<Reading>, String> {
        let (Some(check), Some(latest)) = (&self.check, &self.latest) else {
            return Ok(None);
        };
        read(check, latest).map(|value| Some(value.into()))
    }

    fn watch(&mut self, app: &str, value: &str) -> Result<u64, String> {
        let store = &self.stores()?.apps;
        let file = store
            .load()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|stored| stored.id == app)
            .ok_or_else(|| "That application is no longer saved.".to_owned())?
            .file?;
        let saved = file
            .values
            .iter()
            .find(|saved| saved.id == value)
            .ok_or_else(|| format!("{} no longer has that value.", file.name))?;
        let folder = store.folder(app);
        let check = saved::build(&saved.screen, |name| fs::read(folder.join(name)))
            .map_err(|error| format!("{} cannot be read: {error}.", saved.name))?;
        let processes = self.host.processes().map_err(|error| error.to_string())?;
        let windows = self.host.windows().map_err(|error| error.to_string())?;
        let window = best_window(&file.window.to_match(), processes, windows)
            .ok_or_else(|| format!("Open {} to see what {} reads.", file.name, saved.name))?;
        let watch = self.start(
            window,
            Crop::Region(check.region),
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

    fn watched_reading(&self, watch: u64, when: Option<&WhenDef>) -> Result<Live, String> {
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
                Some(condition.holds(&BTreeMap::from([(signal, value)])))
            });
        Ok(Live::Read {
            reading: value.into(),
            holds,
        })
    }
}

/// `text` as a sentence of its own: capitalized, with a full stop.
fn sentence(text: &str) -> String {
    let mut characters = text.chars();
    characters.next().map_or_else(String::new, |first| {
        format!("{}{}.", first.to_uppercase(), characters.as_str())
    })
}

/// A preview capture's message for one frame.
fn preview_message(generation: u64, frame: io::Result<Frame<'_>>) -> Message {
    match frame {
        Ok(frame) => Message::Frame {
            generation,
            image: frame.image.clone(),
            received: frame.received,
        },
        Err(error) => Message::CaptureFailed {
            generation,
            error: error.to_string(),
        },
    }
}

/// A watch's message for one frame. A window too small for the region is
/// another size, not a stopped capture: the capture goes on.
fn watch_message(generation: u64, frame: io::Result<Frame<'_>>) -> Message {
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

/// What `check` reads on `frame`.
fn read(check: &RegionCheck, frame: &Image) -> Result<Value, String> {
    let cropped = frame.crop(check.region).ok_or_else(|| {
        "The region reaches past the edge of the window. Draw it inside the window.".to_owned()
    })?;
    check
        .check
        .read(&cropped)
        .map_err(|error| error.to_string())
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
    owner.shut_down()
}

impl Request {
    /// Whether the request changes the saved applications or automations,
    /// which the runtime reads.
    const fn saves(&self) -> bool {
        matches!(
            self,
            Self::SaveValue { .. }
                | Self::DeleteValue { .. }
                | Self::SetAntiCheat { .. }
                | Self::SaveAutomation { .. }
                | Self::SetAutomationEnabled { .. }
                | Self::DeleteAutomation { .. }
        )
    }

    /// Whether the request changes an application's values or overlay, which
    /// each overlay then reads again.
    const fn changes_values(&self) -> bool {
        matches!(
            self,
            Self::SaveValue { .. } | Self::DeleteValue { .. } | Self::SetOverlay { .. }
        )
    }
}

/// Asks the runtime to read the saved automations again. A runtime that is
/// not running reads them when it starts, and one that cannot be reached
/// keeps what it read, so a failure here changes nothing the user did.
#[mutants::skip]
fn tell_runtime() {
    #[expect(
        clippy::let_underscore_must_use,
        reason = "the save already happened; the runtime picks it up when it next starts"
    )]
    let _ = appytappy_platform::request_reload(appytappy_platform::RELOAD);
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
    use appytappy_config::automation_file::{ActionDef, ComparisonDef, MediaCommandDef, TargetDef};
    use appytappy_sources::screen::Region;
    use appytappy_sources::system::Bounds;

    use super::*;
    use appytappy_config::store::{AppStore, AutomationStore};

    use crate::check_spec::CheckSpec;
    use crate::overlay::{OVERLAY_INTERVAL, RETRY_POLLS, SETTLE_POLLS, ValueRead, ValueView};

    /// A host with one window whose captures the test drives by hand: each
    /// started capture's handler goes to the test over a channel, and so
    /// does each closed capture's window.
    struct FakeHost {
        handlers: Sender<Started>,
        closed: Sender<u64>,
        refuse: bool,
        /// Makes listing windows fail.
        unlisted: bool,
        /// Makes window 7 minimized.
        minimized: bool,
    }

    /// A capture the fake host started.
    struct Started {
        window: u64,
        crop: Crop,
        interval: Duration,
        handler: FrameHandler,
    }

    /// The setup app's own process in the fake host.
    const OWN_PROCESS: u32 = 99;

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
            if self.unlisted {
                return Err(io::Error::other("no windows here"));
            }
            let own = WindowEntry {
                handle: 8,
                process: ProcessId::new(OWN_PROCESS),
                title: "AppyTappy setup".to_owned(),
                class: "SetupWindow".to_owned(),
                visible: true,
                // Elsewhere on the screen, so an overlay that followed it
                // instead of its application's window would show it.
                bounds: Bounds {
                    left: 10,
                    top: 20,
                    right: 14,
                    bottom: 22,
                },
            };
            Ok(vec![
                own,
                WindowEntry {
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
                },
            ])
        }

        fn processes(&self) -> io::Result<Vec<ProcessEntry>> {
            Ok(vec![
                ProcessEntry {
                    id: ProcessId::new(3),
                    parent: ProcessId::new(0),
                    executable: "Example.exe".to_owned(),
                    threads: 1,
                },
                ProcessEntry {
                    id: ProcessId::new(OWN_PROCESS),
                    parent: ProcessId::new(0),
                    executable: "appytappy-setup.exe".to_owned(),
                    threads: 1,
                },
            ])
        }

        fn window_place(&self, window: u64) -> io::Result<Option<WindowPlace>> {
            Ok((window == 7).then_some(WindowPlace {
                bounds: Bounds {
                    left: 0,
                    top: 0,
                    right: 4,
                    bottom: 2,
                },
                shown: !self.minimized,
                front: true,
            }))
        }

        fn capture(
            &self,
            window: u64,
            crop: Crop,
            interval: Duration,
            handler: FrameHandler,
        ) -> io::Result<FakeSession> {
            if self.refuse {
                return Err(io::Error::other("no capture here"));
            }
            self.handlers
                .send(Started {
                    window,
                    crop,
                    interval,
                    handler,
                })
                .expect("test listens");
            Ok(FakeSession {
                window,
                closed: self.closed.clone(),
            })
        }
    }

    struct Harness {
        owner: Owner<FakeHost>,
        messages: Receiver<Message>,
        handlers: Receiver<Started>,
        closed: Receiver<u64>,
        /// What the owner sends the text reader.
        jobs: Receiver<ocr::Job>,
    }

    /// Fresh, empty stores for one test, all in one folder.
    fn scratch_store(name: &str) -> (Stores, std::path::PathBuf) {
        let folder =
            std::env::temp_dir().join(format!("appytappy-setup-{name}-{}", std::process::id()));
        if folder.exists() {
            std::fs::remove_dir_all(&folder).expect("remove leftovers");
        }
        let stores = Stores {
            apps: AppStore::new(folder.join("apps")),
            automations: AutomationStore::new(folder.join("automations.json")),
        };
        (stores, folder)
    }

    fn harness(refuse: bool) -> Harness {
        harness_with(refuse, Err("no store in this test".to_owned()))
    }

    fn harness_with(refuse: bool, store: Result<Stores, String>) -> Harness {
        let (sender, messages) = mpsc::channel();
        let (handler_sender, handlers) = mpsc::channel();
        let (closed_sender, closed) = mpsc::channel();
        let (job_sender, jobs) = mpsc::channel();
        let host = FakeHost {
            handlers: handler_sender,
            closed: closed_sender,
            refuse,
            unlisted: false,
            minimized: false,
        };
        Harness {
            owner: Owner::new(host, store, OWN_PROCESS, sender, job_sender),
            messages,
            handlers,
            closed,
            jobs,
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
            let started = self.handlers.try_recv().expect("capture started");
            assert_eq!(
                (started.window, started.crop, started.interval),
                (7, Crop::Whole, PREVIEW_INTERVAL)
            );
            started.handler
        }

        /// Runs `handler` on `frame`, a whole window's frame, and applies
        /// what it sent the owner.
        fn deliver(&mut self, handler: &mut FrameHandler, frame: io::Result<&Image>) {
            let frame = frame.map(|image| (image.size(), image));
            self.deliver_at(handler, frame, PREVIEW_INTERVAL);
        }

        /// Runs `handler` on `frame`, its window's size and pixels, checks
        /// the wait it answers, and applies what it sent the owner.
        fn deliver_at(
            &mut self,
            handler: &mut FrameHandler,
            frame: io::Result<(Size, &Image)>,
            interval: Duration,
        ) {
            let frame = frame.map(|(window_size, image)| Frame {
                window_size,
                image,
                received: Instant::now(),
                delay: Duration::ZERO,
            });
            assert_eq!(handler(frame), interval);
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

    /// A 4 by 2 frame with its left `red` columns red and the rest gray.
    fn red_columns(red: usize) -> Image {
        let row: Vec<u8> = (0..4)
            .flat_map(|x| {
                if x < red {
                    [0, 0, 255, 0]
                } else {
                    [40, 40, 40, 0]
                }
            })
            .collect();
        Image::new(
            Size {
                width: 4,
                height: 2,
            },
            [row.clone(), row].concat(),
        )
        .expect("matching length")
    }

    /// Records the whole 4 by 2 frame, red as ink, drawn on screenshot 0.
    fn start_recording(red: u8) -> impl FnOnce(Reply<Result<(), String>>) -> Request {
        let region: RegionSpec =
            serde_json::from_str(r#"{"x":0,"y":0,"width":4,"height":2}"#).expect("valid JSON");
        let ink: ColorSpec =
            serde_json::from_str(&format!(r#"{{"red":{red},"green":0,"blue":0}}"#))
                .expect("valid JSON");
        move |reply| Request::StartRecording {
            region,
            ink,
            tolerance: 0,
            screenshot: 0,
            reply,
        }
    }

    impl Harness {
        /// The recording as the UI sees it.
        fn recording(&mut self) -> Option<RecordingView> {
            self.ask(Request::RecordingState)
        }

        /// Each reading's sightings and text.
        fn readings(&mut self) -> Vec<(u32, Text)> {
            self.recording()
                .expect("a recording")
                .readings
                .into_iter()
                .map(|reading| (reading.seen, reading.text))
                .collect()
        }
    }

    #[test]
    fn a_recording_keeps_each_reading_once_and_has_it_read_as_text() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Request::Screenshot), Ok(0));
        assert_eq!(harness.recording(), None);
        assert_eq!(harness.ask(start_recording(255)), Ok(()));
        for frame in [
            red_columns(2),
            red_columns(2),
            red_columns(0),
            red_columns(3),
        ] {
            harness.deliver(&mut handler, Ok(&frame));
        }
        let jobs: Vec<(u64, usize)> = harness
            .jobs
            .try_iter()
            .map(|job| (job.recording, job.index))
            .collect();
        assert_eq!(jobs, [(1, 0), (1, 1)], "one job per new reading");
        assert_eq!(
            harness.readings(),
            [(2, Text::Pending), (1, Text::Pending)],
            "a frame with no ink is skipped"
        );
        for (recording, text) in [(0, "stale"), (1, "1")] {
            assert!(
                harness
                    .owner
                    .handle(Message::Recognized {
                        recording,
                        index: 0,
                        text: Text::Read(text.to_owned()),
                    })
                    .is_continue()
            );
        }
        assert_eq!(
            harness.readings(),
            [(2, Text::Read("1".to_owned())), (1, Text::Pending)]
        );
        let pixels_of = |index| {
            move |reply| Request::RecordedPixels {
                index,
                max_width: 4,
                reply,
            }
        };
        assert_eq!(
            harness.ask(pixels_of(1)),
            Some(pixels::rgba(&red_columns(3), 4))
        );
        assert_eq!(harness.ask(pixels_of(2)), None);

        harness.ask(Request::StopRecording);
        harness.deliver(&mut handler, Ok(&red_columns(4)));
        let stopped = harness.recording().expect("kept for review");
        assert!(!stopped.recording);
        assert_eq!(
            stopped.readings.len(),
            2,
            "a stopped recording keeps no more"
        );
        assert_eq!(harness.jobs.try_iter().count(), 0);
    }

    #[test]
    fn review_and_the_check_learn_from_recorded_readings() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Request::Screenshot), Ok(0));
        harness.ask(start_recording(255)).expect("records");
        harness.deliver(&mut handler, Ok(&red_columns(2)));
        harness.deliver(&mut handler, Ok(&red_columns(3)));
        let label = |reading, text: &str| RecordedLabel {
            reading,
            text: text.to_owned(),
        };
        let problems = harness.ask(|reply| Request::ReviewProblems {
            readings: vec![label(0, "1"), label(1, "12"), label(1, "7"), label(9, "1")],
            reply,
        });
        assert_eq!(problems[0], None);
        assert_eq!(
            problems[1].as_deref(),
            Some(
                "The region shows 1 characters but the text has 2; characters that touch count \
                 as one."
            ),
            "one blob cannot be two characters"
        );
        assert_eq!(problems[2], None, "judged without the reading that failed");
        assert_eq!(
            problems[3].as_deref(),
            Some("That reading is gone. Record the region again.")
        );
        let number = |reading| -> CheckSpec {
            serde_json::from_str(&format!(
                r#"{{"kind":"number","region":{{"x":0,"y":0,"width":4,"height":2}},
                    "ink":{{"red":255,"green":0,"blue":0}},"tolerance":0,"labels":[],
                    "recorded":[{{"reading":{reading},"text":"1"}}]}}"#
            ))
            .expect("valid JSON")
        };
        assert_eq!(
            harness.ask(|reply| Request::SetCheck {
                spec: number(0),
                reply
            }),
            Ok(())
        );
        harness.ask(Request::DiscardRecording);
        assert_eq!(harness.recording(), None);
        assert_eq!(
            harness.ask(|reply| Request::SetCheck {
                spec: number(0),
                reply
            }),
            Err("That recording is gone. Record the region again.".to_owned())
        );
        assert_eq!(
            harness.ask(|reply| Request::ReviewProblems {
                readings: vec![label(0, "1")],
                reply,
            }),
            [Some(
                "That recording is gone. Record the region again.".to_owned()
            )]
        );
    }

    #[test]
    fn recording_goes_on_after_a_stop_and_starts_over_for_other_ink() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Request::Screenshot), Ok(0));
        harness.ask(start_recording(255)).expect("records");
        harness.deliver(&mut handler, Ok(&red_columns(2)));
        harness.ask(Request::StopRecording);
        harness.ask(start_recording(255)).expect("goes on");
        assert!(harness.recording().expect("kept").recording);
        harness.deliver(&mut handler, Ok(&red_columns(3)));
        assert_eq!(harness.readings().len(), 2, "the same recording went on");
        harness
            .ask(start_recording(200))
            .expect("records other ink");
        assert_eq!(harness.readings().len(), 0, "a new recording");
        let jobs: Vec<u64> = harness.jobs.try_iter().map(|job| job.recording).collect();
        assert_eq!(jobs, [1, 1]);
        assert!(
            harness
                .owner
                .handle(Message::Recognized {
                    recording: 1,
                    index: 0,
                    text: Text::Read("1".to_owned()),
                })
                .is_continue()
        );
        assert_eq!(
            harness.readings().len(),
            0,
            "text for the old one is dropped"
        );
        let _next = harness.pick();
        assert_eq!(harness.recording(), None, "another window drops it");
    }

    #[test]
    fn a_recording_needs_a_window_a_screenshot_and_a_region_inside_it() {
        let mut harness = harness(false);
        assert_eq!(
            harness.ask(start_recording(255)),
            Err("Pick a window to record.".to_owned())
        );
        let mut handler = harness.pick();
        assert_eq!(
            harness.ask(start_recording(255)),
            Err("Take a screenshot first.".to_owned())
        );
        let small = Image::new(
            Size {
                width: 2,
                height: 2,
            },
            [0, 0, 255, 0].repeat(4),
        )
        .expect("matching length");
        harness.deliver(&mut handler, Ok(&small));
        assert_eq!(harness.ask(Request::Screenshot), Ok(0));
        assert_eq!(
            harness.ask(start_recording(255)),
            Err(
                "The region reaches past the edge of the window. Draw it inside the window."
                    .to_owned()
            )
        );
        let empty: RegionSpec =
            serde_json::from_str(r#"{"x":0,"y":0,"width":0,"height":2}"#).expect("valid JSON");
        let ink: ColorSpec =
            serde_json::from_str(r#"{"red":255,"green":0,"blue":0}"#).expect("valid JSON");
        assert_eq!(
            harness.ask(|reply| Request::StartRecording {
                region: empty,
                ink,
                tolerance: 0,
                screenshot: 0,
                reply,
            }),
            Err("Draw the region to record first.".to_owned())
        );
    }

    #[test]
    fn a_reading_the_text_reader_cannot_take_says_to_type_it() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Request::Screenshot), Ok(0));
        harness.ask(start_recording(255)).expect("records");
        let (closed, _) = mpsc::channel();
        harness.owner.ocr = closed;
        harness.deliver(&mut handler, Ok(&red_columns(2)));
        assert_eq!(
            harness.readings(),
            [(
                1,
                Text::Failed("The text reader stopped. Type what this shows.".to_owned())
            )]
        );
    }

    #[test]
    fn a_problem_reads_as_a_sentence_of_its_own() {
        assert_eq!(sentence("a character was given"), "A character was given.");
        assert_eq!(sentence(""), "");
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
            screenshot: 0,
        })
    }

    const REFERENCE: &str = r#"{"kind":"reference","region":{"x":0,"y":0,"width":2,"height":2},
        "screenshot":0,"tolerance":0}"#;
    const BAR: &str = r#"{"kind":"barFill","region":{"x":0,"y":0,"width":4,"height":2},
        "filled":{"red":255,"green":0,"blue":0},"empty":{"red":40,"green":40,"blue":40}}"#;

    #[test]
    fn saved_values_are_listed_with_their_running_windows_and_can_be_removed() {
        let (stores, root) = scratch_store("save");
        let apps_root = stores.apps.folder("");
        let mut harness = harness_with(false, Ok(stores));
        let mut handler = harness.pick();
        let save = |spec| move |reply| Request::SaveValue { spec, reply };
        assert_eq!(
            harness.ask(save(spec(None, "Icon visible", REFERENCE))),
            Err("Take a screenshot first.".to_owned())
        );
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Request::Screenshot), Ok(0));
        // The window shrinks after the screenshot, and the value keeps the
        // size of the screenshot its region was drawn on.
        let shrunk = Image::new(
            Size {
                width: 2,
                height: 1,
            },
            vec![0; 8],
        )
        .expect("matching length");
        harness.deliver(&mut handler, Ok(&shrunk));
        assert_eq!(
            harness.ask(save(spec(None, "  ", REFERENCE))),
            Err("Name the value first.".to_owned())
        );
        let mut stale = spec(None, "Icon visible", REFERENCE);
        // A picture check's own screenshot wins over the one shown.
        stale.screenshot = 9;
        let saved = harness.ask(save(stale)).expect("saves a new app");
        assert_eq!(
            saved,
            SavedValue {
                app: "example".to_owned(),
                value: "icon_visible".to_owned()
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
        assert_eq!(ids, ["icon_visible", "progress_bar"]);
        assert_eq!(
            file.values[0].screen.window_size,
            SizeDef {
                width: 4,
                height: 2
            }
        );
        let folder = apps_root.join("example");
        assert!(folder.join("icon_visible.png").exists());

        let automation: AutomationDef = serde_json::from_value(serde_json::json!({
            "id": "",
            "name": "Pause music",
            "enabled": true,
            "when": {"app": "example", "value": "icon_visible", "is": "equals", "target": true},
            "actions": [
                {"kind": "media", "command": "pause", "app": " Player.exe "},
                {"kind": "volume", "percent": 20, "app": null}
            ],
            "undo": true,
            "cooldownMs": 1500
        }))
        .expect("the UI's spelling");
        let id = harness
            .ask(|reply| Request::SaveAutomation {
                existing: None,
                automation: Box::new(automation),
                reply,
            })
            .expect("saves");
        assert_eq!(id, "pause_music");
        assert_eq!(
            harness.ask(|reply| Request::DeleteValue {
                app: "example".to_owned(),
                value: "icon_visible".to_owned(),
                reply,
            }),
            Err("Pause music uses Icon visible. Change or delete it first.".to_owned())
        );
        harness
            .ask(|reply| Request::SetAutomationEnabled {
                id: id.clone(),
                enabled: false,
                reply,
            })
            .expect("turns off");
        let listed = harness.ask(Request::ListAutomations).expect("lists");
        assert_eq!(listed.len(), 1);
        assert!(!listed[0].automation.enabled);
        assert_eq!(listed[0].automation.cooldown_ms, 1500);
        assert_eq!(
            listed[0].automation.actions[0],
            ActionDef::Media {
                command: MediaCommandDef::Pause,
                app: Some("Player.exe".to_owned()),
            }
        );
        harness
            .ask(|reply| Request::DeleteAutomation { id, reply })
            .expect("removes");
        assert_eq!(harness.ask(Request::ListAutomations), Ok(Vec::new()));

        // Keys are pressed only for an application whose anti-cheat is none.
        let press: AutomationDef = serde_json::from_value(serde_json::json!({
            "id": "",
            "name": "Press keys",
            "enabled": true,
            "when": {"app": "example", "value": "icon_visible", "is": "equals", "target": true},
            "actions": [{"kind": "keys", "keys": " shift + ctrl + m "}],
            "undo": false,
            "cooldownMs": 0
        }))
        .expect("the UI's spelling");
        let save_press = |harness: &mut Harness| {
            harness.ask(|reply| Request::SaveAutomation {
                existing: None,
                automation: Box::new(press.clone()),
                reply,
            })
        };
        assert!(
            save_press(&mut harness)
                .is_err_and(|error| error.contains("anti-cheat is set to none")),
            "an application whose anti-cheat is not set cannot press keys"
        );
        let set = |harness: &mut Harness, class| {
            harness.ask(|reply| Request::SetAntiCheat {
                app: "example".to_owned(),
                class,
                reply,
            })
        };
        set(&mut harness, AntiCheatDef::None).expect("sets");
        let apps = harness.ask(Request::ListApps).expect("lists");
        assert_eq!(
            apps[0].file.as_ref().map(|file| file.anti_cheat),
            Some(AntiCheatDef::None)
        );
        let id = save_press(&mut harness).expect("saves");
        let listed = harness.ask(Request::ListAutomations).expect("lists");
        assert_eq!(
            listed[0].automation.actions,
            [ActionDef::Keys {
                keys: "Ctrl+Shift+M".to_owned()
            }],
            "the file keeps one spelling"
        );
        assert_eq!(listed[0].problem, None);
        set(&mut harness, AntiCheatDef::KernelLevel).expect("sets");
        let listed = harness.ask(Request::ListAutomations).expect("lists");
        assert!(
            listed[0]
                .problem
                .as_ref()
                .is_some_and(|problem| problem.contains("anti-cheat is set to none")),
            "{listed:?}"
        );
        harness
            .ask(|reply| Request::DeleteAutomation { id, reply })
            .expect("removes");
        assert_eq!(
            set(&mut harness, AntiCheatDef::None).map(drop),
            Ok(()),
            "lowering it again is allowed too"
        );

        harness
            .ask(|reply| Request::DeleteValue {
                app: "example".to_owned(),
                value: "icon_visible".to_owned(),
                reply,
            })
            .expect("removes");
        assert!(!folder.join("icon_visible.png").exists());
        let apps = harness.ask(Request::ListApps).expect("lists");
        let file = apps[0].file.as_ref().expect("a good file");
        assert_eq!(file.values.len(), 1);
        // The saved values still list when the windows cannot be.
        harness.owner.host.unlisted = true;
        let apps = harness.ask(Request::ListApps).expect("lists");
        assert_eq!((apps.len(), apps[0].windows.len()), (1, 0));
        harness.owner.host.unlisted = false;
        assert_eq!(
            harness.ask(|reply| Request::DeleteValue {
                app: "gone".to_owned(),
                value: "x".to_owned(),
                reply,
            }),
            Err("That application is no longer saved.".to_owned())
        );
        assert_eq!(
            harness.ask(|reply| Request::SetAntiCheat {
                app: "gone".to_owned(),
                class: AntiCheatDef::None,
                reply,
            }),
            Err("That application is no longer saved.".to_owned())
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    fn watch(app: &str, value: &str) -> impl FnOnce(Reply<Result<u64, String>>) -> Request {
        let (app, value) = (app.to_owned(), value.to_owned());
        move |reply| Request::WatchValue { app, value, reply }
    }

    fn watched(
        watch: u64,
        when: Option<WhenDef>,
    ) -> impl FnOnce(Reply<Result<Live, String>>) -> Request {
        move |reply| Request::WatchedReading {
            watch,
            when: when.map(Box::new),
            reply,
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

    /// A harness with the bar `Progress bar` saved for `Example App`, drawn
    /// on the whole of window 7's 4 by 2 frame.
    fn with_saved_bar(name: &str) -> (Harness, std::path::PathBuf) {
        let (stores, root) = scratch_store(name);
        let mut harness = harness_with(false, Ok(stores));
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Request::Screenshot), Ok(0));
        harness
            .ask(|reply| Request::SaveValue {
                spec: spec(None, "Progress bar", BAR),
                reply,
            })
            .expect("saves");
        (harness, root)
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
                Crop::Region(Region::new(0, 0, 4, 2).expect("not empty")),
                WATCH_INTERVAL
            )
        );
        (watch, started.handler)
    }

    const WINDOW: Size = Size {
        width: 4,
        height: 2,
    };

    fn overlay_state(reply: Reply<Result<OverlayView, String>>) -> Request {
        Request::OverlayState {
            app: "example".to_owned(),
            reply,
        }
    }

    fn overlay_place(reply: Reply<Option<Place>>) -> Request {
        Request::OverlayPlace {
            app: "example".to_owned(),
            reply,
        }
    }

    fn set_overlay(
        enabled: bool,
        mode: OverlayModeDef,
    ) -> impl FnOnce(Reply<Result<(), String>>) -> Request {
        move |reply| Request::SetOverlay {
            app: "example".to_owned(),
            enabled,
            mode,
            reply,
        }
    }

    fn close_overlay(mode: OverlayModeDef) -> impl FnOnce(Reply<()>) -> Request {
        move |reply| Request::CloseOverlay {
            app: "example".to_owned(),
            mode,
            reply,
        }
    }

    /// The overlay's view of `Progress bar` reading `reading` or failing
    /// with `problem`.
    fn bar_view(
        mode: OverlayModeDef,
        reading: Option<Reading>,
        problem: Option<&str>,
    ) -> OverlayView {
        OverlayView {
            name: "Example App".to_owned(),
            mode,
            message: None,
            values: vec![ValueView {
                name: "Progress bar".to_owned(),
                reading,
                problem: problem.map(str::to_owned),
            }],
        }
    }

    /// Polls the overlay once and returns its capture's handler, checking it
    /// captures only `crop` of window 7.
    fn overlay_capture_of(harness: &mut Harness, crop: Region) -> FrameHandler {
        harness.ask(overlay_state).expect("shows");
        let started = harness.handlers.try_recv().expect("capture started");
        assert_eq!(
            (started.window, started.crop, started.interval),
            (7, Crop::Region(crop), OVERLAY_INTERVAL)
        );
        started.handler
    }

    /// Polls the overlay once and returns its capture's handler, checking it
    /// captures only the part of window 7 that holds the bar.
    fn overlay_capture(harness: &mut Harness) -> FrameHandler {
        overlay_capture_of(harness, Region::new(0, 0, 4, 2).expect("not empty"))
    }

    /// Changes the saved `Example App` file with `change`, as if another
    /// save did, without telling the overlays.
    fn change_app_file(harness: &Harness, change: impl FnOnce(&mut AppFile)) {
        let stores = harness.owner.stores().expect("stores");
        let mut file = stores
            .apps
            .load()
            .expect("loads")
            .into_iter()
            .find(|stored| stored.id == "example")
            .expect("saved")
            .file
            .expect("readable");
        change(&mut file);
        stores.apps.save("example", &file, &[]).expect("saves");
    }

    #[test]
    fn an_overlay_reads_its_applications_values_and_follows_its_window() {
        let (mut harness, root) = with_saved_bar("overlay");
        assert_eq!(
            harness.ask(overlay_state),
            Err("The overlay for Example App is off.".to_owned())
        );
        assert_eq!(harness.ask(Request::Overlays), Ok(Vec::new()));
        harness
            .ask(set_overlay(true, OverlayModeDef::Overlay))
            .expect("saves");
        assert_eq!(
            harness.ask(Request::Overlays),
            Ok(vec![OverlayWindow {
                app: "example".to_owned(),
                name: "Example App".to_owned(),
                mode: OverlayModeDef::Overlay,
                panel: None,
            }])
        );
        // The picked window's capture is the capture screen's, not the
        // overlay's.
        assert!(harness.closed.try_recv().is_err());
        assert_eq!(harness.ask(overlay_place), None, "not open yet");

        let mut handler = overlay_capture(&mut harness);
        let over = OverlayModeDef::Overlay;
        assert_eq!(harness.ask(overlay_state), Ok(bar_view(over, None, None)));
        harness.deliver_at(&mut handler, Ok((WINDOW, &frame_image())), OVERLAY_INTERVAL);
        assert_eq!(
            harness.ask(overlay_state),
            Ok(bar_view(over, Some(Reading::Number(50.0)), None))
        );

        // It goes over the window once its card is measured, and not while
        // the window is minimized.
        assert_eq!(harness.ask(overlay_place), None, "no card yet");
        let card = Card {
            width: 180.0,
            height: 40.0,
        };
        harness.ask(|reply| Request::OverlayCard {
            app: "example".to_owned(),
            card,
            reply,
        });
        assert_eq!(
            harness.ask(overlay_place),
            Some(Place {
                x: 0,
                y: 0,
                width: 4,
                height: 2,
                card,
            })
        );
        harness.owner.host.minimized = true;
        assert_eq!(harness.ask(overlay_place), None);
        let minimized = harness.ask(overlay_state).expect("shows");
        assert_eq!(
            minimized.message.as_deref(),
            Some("Restore Example App to see its values.")
        );
        harness.owner.host.minimized = false;

        // A window of a size no value was drawn at reads nothing, and the
        // capture goes on.
        let wider = Size {
            width: 6,
            height: 2,
        };
        harness.deliver_at(&mut handler, Ok((wider, &frame_image())), OVERLAY_INTERVAL);
        let resized = |width| {
            Ok(bar_view(
                over,
                None,
                Some(&format!("Drawn on a 4x2 window; it is {width}x2 now.")),
            ))
        };
        assert_eq!(harness.ask(overlay_state), resized(6));
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
            OVERLAY_INTERVAL,
        );
        assert_eq!(harness.ask(overlay_state), resized(3));
        assert!(harness.closed.try_recv().is_err(), "the capture goes on");
        let white = Image::new(WINDOW, vec![255; 32]).expect("matching length");
        harness.deliver_at(&mut handler, Ok((WINDOW, &white)), OVERLAY_INTERVAL);
        let view = harness.ask(overlay_state).expect("shows");
        let problem = view.values[0].problem.as_deref().expect("a problem");
        assert!(problem.starts_with("Cannot read: "), "{problem}");

        // A capture that stops closes, says why, and starts again about a
        // second later.
        harness.deliver_at(
            &mut handler,
            Err(io::Error::other("the window closed")),
            OVERLAY_INTERVAL,
        );
        assert_eq!(harness.closed.try_recv(), Ok(7));
        for _ in 0..RETRY_POLLS {
            let view = harness.ask(overlay_state).expect("shows");
            assert_eq!(
                view.message.as_deref(),
                Some("Example App cannot be read: the window closed.")
            );
            assert!(harness.handlers.try_recv().is_err(), "it waits");
        }
        let mut handler = overlay_capture(&mut harness);
        // Readings from the stopped capture are dropped.
        let message = Message::Overlay {
            app: "example".to_owned(),
            generation: 0,
            read: Ok(FrameRead::Values(vec![ValueRead::Read(Reading::Flag(
                true,
            ))])),
        };
        assert!(harness.owner.handle(message).is_continue());
        assert_eq!(harness.ask(overlay_state), Ok(bar_view(over, None, None)));
        harness.deliver_at(&mut handler, Ok((WINDOW, &frame_image())), OVERLAY_INTERVAL);

        // Reading the saved file again keeps the card's size, which the page
        // sends only when it changes.
        harness.ask(set_overlay(true, over)).expect("saves");
        assert_eq!(harness.closed.try_recv(), Ok(7));
        drop(overlay_capture(&mut harness));
        assert!(harness.ask(overlay_place).is_some());

        // Changing the overlay reads the saved file again.
        harness
            .ask(set_overlay(false, OverlayModeDef::Overlay))
            .expect("saves");
        assert_eq!(harness.closed.try_recv(), Ok(7));
        assert_eq!(
            harness.ask(overlay_state),
            Err("The overlay for Example App is off.".to_owned())
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn an_overlay_reads_the_values_drawn_at_the_windows_size_and_says_why_others_cannot() {
        let (mut harness, root) = with_saved_bar("overlay-sizes");
        change_app_file(&harness, |file| {
            let mut small = file.values[0].clone();
            small.id = "small".to_owned();
            small.name = "Small".to_owned();
            small.screen.window_size = SizeDef {
                width: 8,
                height: 4,
            };
            small.screen.region.x = 4;
            small.screen.region.y = 2;
            let mut icon = file.values[0].clone();
            icon.id = "icon".to_owned();
            icon.name = "Icon".to_owned();
            icon.screen.check = serde_json::from_value(serde_json::json!({
                "kind": "reference",
                "image": "missing.png",
                "tolerance": 0
            }))
            .expect("the file's spelling");
            file.values.extend([small, icon]);
            file.overlay = Some(OverlayDef {
                enabled: true,
                mode: OverlayModeDef::PictureInPicture,
                panel: None,
            });
        });
        // Only the bar was drawn at 4x2, so only its region is captured.
        let mut handler = overlay_capture(&mut harness);
        harness.deliver_at(&mut handler, Ok((WINDOW, &frame_image())), OVERLAY_INTERVAL);
        let view = harness.ask(overlay_state).expect("shows");
        let lines: Vec<(&str, Option<Reading>, Option<&str>)> = view
            .values
            .iter()
            .map(|value| (value.name.as_str(), value.reading, value.problem.as_deref()))
            .collect();
        assert_eq!(
            lines[0],
            ("Progress bar", Some(Reading::Number(50.0)), None)
        );
        assert_eq!(
            lines[1],
            ("Small", None, Some("Drawn on a 8x4 window; it is 4x2 now."))
        );
        let (icon, reading, problem) = lines[2];
        assert_eq!((icon, reading), ("Icon", None));
        assert!(
            problem.is_some_and(|problem| problem.starts_with("Cannot read: ")),
            "{problem:?}"
        );

        // At 8x4 the capture starts again with the small value's region.
        let large = Size {
            width: 8,
            height: 4,
        };
        harness.deliver_at(&mut handler, Ok((large, &frame_image())), OVERLAY_INTERVAL);
        assert_eq!(harness.closed.try_recv(), Ok(7));
        let mut handler =
            overlay_capture_of(&mut harness, Region::new(4, 2, 4, 2).expect("not empty"));
        harness.deliver_at(&mut handler, Ok((large, &frame_image())), OVERLAY_INTERVAL);
        let view = harness.ask(overlay_state).expect("shows");
        assert_eq!(
            view.values[0].problem.as_deref(),
            Some("Drawn on a 4x2 window; it is 8x4 now.")
        );
        assert_eq!(view.values[1].reading, Some(Reading::Number(50.0)));
        // A panel does not go over the window.
        assert_eq!(harness.ask(overlay_place), None);
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn an_overlay_without_its_window_or_file_says_why_and_tries_again_about_once_a_second() {
        let (mut harness, root) = with_saved_bar("overlay-retry");
        harness
            .ask(set_overlay(true, OverlayModeDef::Overlay))
            .expect("saves");
        harness.owner.host.unlisted = true;
        let gone = OverlayView {
            name: "Example App".to_owned(),
            mode: OverlayModeDef::Overlay,
            message: Some("no windows here".to_owned()),
            values: vec![ValueView {
                name: "Progress bar".to_owned(),
                reading: None,
                problem: None,
            }],
        };
        assert_eq!(harness.ask(overlay_state), Ok(gone.clone()));
        harness.owner.host.unlisted = false;
        for _ in 0..RETRY_POLLS {
            assert_eq!(harness.ask(overlay_state), Ok(gone.clone()));
            assert!(harness.handlers.try_recv().is_err(), "it waits");
        }
        drop(overlay_capture(&mut harness));

        // An overlay that cannot open reads its file again only after a
        // while, rather than on every poll.
        harness
            .ask(set_overlay(false, OverlayModeDef::Overlay))
            .expect("saves");
        assert_eq!(harness.closed.try_recv(), Ok(7));
        let off = Err("The overlay for Example App is off.".to_owned());
        assert_eq!(harness.ask(overlay_state), off);
        change_app_file(&harness, |file| {
            file.overlay = Some(OverlayDef {
                enabled: true,
                mode: OverlayModeDef::Overlay,
                panel: None,
            });
        });
        for _ in 0..RETRY_POLLS {
            assert_eq!(harness.ask(overlay_state), off);
        }
        drop(overlay_capture(&mut harness));
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_panel_keeps_where_the_user_put_it_once_it_settles() {
        let (mut harness, root) = with_saved_bar("overlay-panel");
        let panel_mode = OverlayModeDef::PictureInPicture;
        harness.ask(set_overlay(true, panel_mode)).expect("saves");
        drop(overlay_capture(&mut harness));
        assert_eq!(
            harness.ask(overlay_state),
            Ok(bar_view(panel_mode, None, None))
        );
        let panel = |x| PanelDef {
            x,
            y: 20,
            width: 260,
            height: 200,
        };
        let placed = |x| Message::PanelPlaced {
            app: "example".to_owned(),
            panel: panel(x),
        };
        let saved = |harness: &mut Harness| {
            harness
                .ask(Request::Overlays)
                .expect("listed")
                .first()
                .and_then(|overlay| overlay.panel)
        };
        assert!(harness.owner.handle(placed(-900)).is_continue());
        assert!(harness.owner.handle(placed(-800)).is_continue());
        for _ in 1..SETTLE_POLLS {
            harness.ask(overlay_state).expect("shows");
            assert_eq!(
                saved(&mut harness),
                None,
                "it waits for the panel to settle"
            );
        }
        harness.ask(overlay_state).expect("shows");
        assert_eq!(saved(&mut harness), Some(panel(-800)));

        // Saving the overlay right after a move keeps both.
        assert!(harness.owner.handle(placed(-700)).is_continue());
        harness.ask(set_overlay(true, panel_mode)).expect("saves");
        assert_eq!(saved(&mut harness), Some(panel(-700)));
        assert_eq!(harness.closed.try_recv(), Ok(7));
        drop(overlay_capture(&mut harness));

        // The overlay window of the mode it left closing later leaves it.
        harness.ask(close_overlay(OverlayModeDef::Overlay));
        assert!(harness.closed.try_recv().is_err());
        // Its own window closing saves its place at once.
        assert!(harness.owner.handle(placed(10)).is_continue());
        harness.ask(close_overlay(panel_mode));
        assert_eq!(harness.closed.try_recv(), Ok(7));
        assert_eq!(saved(&mut harness), Some(panel(10)));
        // So does the app ending.
        drop(overlay_capture(&mut harness));
        assert!(harness.owner.handle(placed(30)).is_continue());
        harness.owner.shut_down().expect("closes");
        assert_eq!(saved(&mut harness), Some(panel(30)));
        // A place for an overlay that is not open is dropped.
        assert!(harness.owner.handle(placed(50)).is_continue());
        harness.owner.shut_down().expect("closes");
        assert_eq!(saved(&mut harness), Some(panel(30)));
        std::fs::remove_dir_all(root).expect("clean up");
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
            .ask(|reply| Request::SetCheck { spec, reply })
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
        harness.ask(|reply| Request::StopWatching {
            watch: first,
            reply,
        });
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
        harness.ask(|reply| Request::StopWatching {
            watch: second,
            reply,
        });
        assert!(harness.closed.try_recv().is_err());
        let (third, _handler) = watch_bar(&mut harness);
        assert_eq!(harness.closed.try_recv(), Ok(7));
        harness.ask(|reply| Request::StopWatching {
            watch: third,
            reply,
        });
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
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Request::Screenshot), Ok(0));
        let save = |spec| move |reply| Request::SaveValue { spec, reply };
        harness
            .ask(save(spec(None, "Icon visible", REFERENCE)))
            .expect("saves");
        let folder = apps.folder("example");
        let picture = folder.join("icon_visible.png");
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
        assert_eq!(
            harness.ask(Request::ListAutomations),
            Err("no store in this test".to_owned())
        );
        let automation: AutomationDef = serde_json::from_value(serde_json::json!({
            "id": "", "name": "x", "enabled": true,
            "when": {"app": "a", "value": "b", "is": "equals", "target": true},
            "actions": [{"kind": "media", "command": "pause"}], "undo": false
        }))
        .expect("the UI's spelling");
        assert_eq!(
            harness.ask(|reply| Request::SaveAutomation {
                existing: None,
                automation: Box::new(automation),
                reply
            }),
            Err("no store in this test".to_owned())
        );
        assert_eq!(
            harness.ask(|reply| Request::SetAutomationEnabled {
                id: "x".to_owned(),
                enabled: true,
                reply
            }),
            Err("no store in this test".to_owned())
        );
        assert_eq!(
            harness.ask(|reply| Request::DeleteAutomation {
                id: "x".to_owned(),
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
    fn stopping_closes_the_capture_and_forgets_the_screenshots() {
        let mut harness = harness(false);
        let mut handler = harness.pick();
        harness.deliver(&mut handler, Ok(&frame_image()));
        assert_eq!(harness.ask(Request::Screenshot), Ok(0));
        harness.ask(Request::Stop);
        assert_eq!(harness.closed.try_recv(), Ok(7));
        assert_eq!(
            harness.ask(|reply| Request::ScreenshotPixels {
                index: 0,
                max_width: 100,
                reply,
            }),
            None
        );
        assert_eq!(
            harness.ask(Request::Screenshot),
            Err("No frame has arrived from the window yet.".to_owned())
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
        assert!(Host::window_place(&unsupported, 7).is_err());
        let handler: FrameHandler = Box::new(|_frame| Duration::ZERO);
        assert!(Host::capture(&unsupported, 7, Crop::Whole, PREVIEW_INTERVAL, handler).is_err());
    }

    #[test]
    fn only_requests_that_save_tell_the_runtime() {
        let automation: AutomationDef = serde_json::from_value(serde_json::json!({
            "id": "",
            "name": "x",
            "enabled": true,
            "when": {"app": "example", "value": "x", "is": "equals", "target": true},
            "actions": [],
            "undo": false,
            "cooldownMs": 0
        }))
        .expect("the UI's spelling");
        let saves = [
            Request::SaveValue {
                spec: spec(None, "x", BAR),
                reply: oneshot::channel().0,
            },
            Request::SaveAutomation {
                existing: None,
                automation: Box::new(automation),
                reply: oneshot::channel().0,
            },
            Request::DeleteValue {
                app: "example".to_owned(),
                value: "progress".to_owned(),
                reply: oneshot::channel().0,
            },
            Request::SetAutomationEnabled {
                id: "x".to_owned(),
                enabled: true,
                reply: oneshot::channel().0,
            },
            Request::DeleteAutomation {
                id: "x".to_owned(),
                reply: oneshot::channel().0,
            },
            Request::SetAntiCheat {
                app: "example".to_owned(),
                class: AntiCheatDef::None,
                reply: oneshot::channel().0,
            },
        ];
        assert!(saves.iter().all(Request::saves));
        assert!(!Request::ListApps(oneshot::channel().0).saves());
        assert!(!Request::ListAutomations(oneshot::channel().0).saves());
    }
}
