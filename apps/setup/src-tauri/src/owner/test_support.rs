//! The stand-ins the owner's tests share: a host whose captures the test drives
//! by hand, and a harness that asks the owner and delivers frames.

use std::io;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use appytappy_config::app_file::{BuildDef, SizeChangesDef, WindowDef};
use appytappy_config::click::{ClickButton, ClickDelivery, Spot};
use appytappy_config::process::ProcessId;
use appytappy_config::window_match::WindowMatch;
use appytappy_sources::screen::place::Target;
use appytappy_sources::screen::{
    CaptureSession, Crop, Frame, FrameHandler, Image, NextFrame, Size,
};
use appytappy_sources::system::{Bounds, ProcessEntry, WindowEntry, WindowPlace};

use crate::applications::AppSpec;
use crate::check_spec::CharactersSpec;
use crate::overlay::OverlayWindow;
use crate::recording::{Scope, Text, look, ocr};
use crate::stores::{self, Stores};
use crate::ticket::Ticket;
use appytappy_config::app_file::{ColorDef, RegionDef};

use super::host::{ClickNow, ClickedWindow, Host};
use super::messages::{Event, Message};
use super::{Owner, PREVIEW_INTERVAL};
use super::{RecordSpec, ReferenceTrial, SaveSpec, TrialSpec};

/// A host with one window whose captures the test drives by hand: each
/// started capture's handler goes to the test over a channel, and so
/// does each closed capture's window.
pub(super) struct FakeHost {
    handlers: Sender<Started>,
    closed: Sender<u64>,
    pub(super) refuse: bool,
    /// Makes listing windows fail.
    pub(super) unlisted: bool,
    /// Makes window 7 minimized.
    pub(super) minimized: bool,
    /// The process of another copy of the example application, whose
    /// minimized window 6 fits as well as window 7 and is listed first.
    pub(super) twin: Option<u32>,
    /// The process that owns window 7, the example application's.
    pub(super) process: u32,
    /// The build that process runs, or `None` when it cannot be read.
    pub(super) build: Option<BuildDef>,
    /// Told each time the owner asks the runtime to read the saved files
    /// again.
    reloads: Sender<()>,
    /// Told the overlay windows wanted each time the owner opens and closes
    /// them to match.
    synced: Sender<Vec<OverlayWindow>>,
    /// Why opening and closing the overlay windows fails, if it does.
    pub(super) unsynced: Option<String>,
    /// The time the owner reads, which [`Harness::wait`] moves on.
    pub(super) clock: Instant,
    /// Whether the runtime runs, or why that cannot be told.
    pub(super) runtime: Result<bool, String>,
    /// Told each click the owner sends.
    clicks: Sender<SentClick>,
    /// Why sending a click fails, if it does.
    pub(super) unclicked: Option<String>,
}

/// A click the owner had the fake host send.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct SentClick {
    /// The name of the application it clicks in.
    pub(super) app: String,
    /// How that application's window is found.
    pub(super) window: WindowMatch,
    /// Its spot, or `None` for a click on a picture.
    pub(super) spot: Option<Spot>,
    pub(super) button: ClickButton,
    pub(super) delivery: ClickDelivery,
}

/// The build the fake host's application runs unless a test changes it.
pub(super) fn example_build() -> BuildDef {
    BuildDef {
        version: Some("1.4.2.0".to_owned()),
        size: 1_024,
        modified: 1_762_500_000,
    }
}

/// A capture the fake host started.
pub(super) struct Started {
    pub(super) window: u64,
    pub(super) crop: Crop,
    pub(super) interval: Duration,
    pub(super) handler: FrameHandler,
}

/// The setup app's own process in the fake host.
const OWN_PROCESS: u32 = 99;

#[derive(Debug)]
pub(super) struct FakeSession {
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
            title: "AppyTappy Setup".to_owned(),
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
        let example = |handle, process| WindowEntry {
            handle,
            process: ProcessId::new(process),
            title: "Example App".to_owned(),
            class: "ExampleWindow".to_owned(),
            visible: true,
            bounds: Bounds {
                left: 0,
                top: 0,
                right: 4,
                bottom: 2,
            },
        };
        let twin = self.twin.map(|process| example(6, process));
        Ok([Some(own), twin, Some(example(7, self.process))]
            .into_iter()
            .flatten()
            .collect())
    }

    fn processes(&self) -> io::Result<Vec<ProcessEntry>> {
        let process = |id, executable: &str| ProcessEntry {
            id: ProcessId::new(id),
            parent: ProcessId::new(0),
            executable: executable.to_owned(),
            threads: 1,
        };
        let twin = self.twin.map(|id| process(id, "Example.exe"));
        Ok([
            Some(process(self.process, "Example.exe")),
            twin,
            Some(process(OWN_PROCESS, "appytappy-setup.exe")),
        ]
        .into_iter()
        .flatten()
        .collect())
    }

    fn window_place(&self, window: u64) -> io::Result<Option<WindowPlace>> {
        let shown = match window {
            7 => !self.minimized,
            6 if self.twin.is_some() => false,
            _ => return Ok(None),
        };
        Ok(Some(WindowPlace {
            bounds: Bounds {
                left: 0,
                top: 0,
                right: 4,
                bottom: 2,
            },
            shown,
            front: true,
        }))
    }

    fn executable_build(&self, process: ProcessId) -> io::Result<BuildDef> {
        if process != ProcessId::new(self.process) {
            return Err(io::Error::other("another process"));
        }
        self.build
            .clone()
            .ok_or_else(|| io::Error::other("unreadable here"))
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

    fn click(&self, click: ClickNow<'_>) -> Result<ClickedWindow, String> {
        let spot = match click.target {
            Target::Spot(spot) => Some(spot),
            Target::Picture(_) => None,
        };
        self.clicks
            .send(SentClick {
                app: click.app.to_owned(),
                window: click.window.clone(),
                spot,
                button: click.button,
                delivery: click.delivery,
            })
            .expect("test listens");
        match &self.unclicked {
            Some(why) => Err(why.clone()),
            None => Ok(ClickedWindow {
                handle: 7,
                title: "Example App".to_owned(),
            }),
        }
    }

    fn now(&self) -> Instant {
        self.clock
    }

    fn reload_runtime(&self) {
        self.reloads.send(()).expect("test listens");
    }

    fn runtime_runs(&self) -> io::Result<bool> {
        self.runtime.clone().map_err(io::Error::other)
    }

    fn sync_overlay_windows(&self, wanted: &[OverlayWindow]) -> Result<(), String> {
        self.synced.send(wanted.to_vec()).expect("test listens");
        self.unsynced.clone().map_or(Ok(()), Err)
    }
}

pub(super) struct Harness {
    pub(super) owner: Owner<FakeHost>,
    pub(super) messages: Receiver<Message<FakeHost>>,
    pub(super) handlers: Receiver<Started>,
    pub(super) closed: Receiver<u64>,
    /// What the owner sends the text reader.
    pub(super) jobs: Receiver<ocr::Job>,
    /// The frames the owner sends to be looked over.
    pub(super) looks: Receiver<look::Job>,
    /// Each time the owner asked the runtime to read the saved files again.
    reloads: Receiver<()>,
    /// The overlay windows wanted each time the owner opened and closed
    /// them to match.
    pub(super) synced: Receiver<Vec<OverlayWindow>>,
    /// Each click the owner sent.
    pub(super) clicks: Receiver<SentClick>,
}

/// Fresh, empty stores for one test, all in one folder.
pub(super) fn scratch_store(name: &str) -> (Stores, std::path::PathBuf) {
    stores::scratch(&format!("setup-{name}"))
}

/// What a [`harness`] answers to anything that needs its stores.
pub(super) const NO_STORE: &str =
    "AppyTappy cannot find where to keep your values: no store in this test.";

pub(super) fn harness(refuse: bool) -> Harness {
    harness_with(refuse, Err("no store in this test".to_owned()))
}

pub(super) fn harness_with(refuse: bool, store: Result<Stores, String>) -> Harness {
    let (sender, messages) = mpsc::channel();
    let (handler_sender, handlers) = mpsc::channel();
    let (closed_sender, closed) = mpsc::channel();
    let (job_sender, jobs) = mpsc::channel();
    let (look_sender, looks) = mpsc::channel();
    let (reload_sender, reloads) = mpsc::channel();
    let (synced_sender, synced) = mpsc::channel();
    let (click_sender, clicks) = mpsc::channel();
    let host = FakeHost {
        handlers: handler_sender,
        closed: closed_sender,
        refuse,
        unlisted: false,
        minimized: false,
        twin: None,
        process: 3,
        build: Some(example_build()),
        reloads: reload_sender,
        synced: synced_sender,
        unsynced: None,
        clock: Instant::now(),
        runtime: Ok(false),
        clicks: click_sender,
        unclicked: None,
    };
    Harness {
        owner: Owner::new(host, store, OWN_PROCESS, sender, job_sender, look_sender),
        messages,
        handlers,
        closed,
        jobs,
        looks,
        reloads,
        synced,
        clicks,
    }
}

/// The text reader's message that reading `index` of the recording numbered
/// `recording` reads `text`.
pub(super) fn recognized(recording: u64, index: usize, text: Text) -> Message<FakeHost> {
    Message::Sent {
        ticket: Ticket::numbered(recording),
        event: Event::Recognized { index, text },
    }
}

/// The text reader's message that find `index` of the search numbered
/// `search` reads `text`.
pub(super) fn found_text(search: u64, index: usize, text: Text) -> Message<FakeHost> {
    Message::Sent {
        ticket: Ticket::numbered(search),
        event: Event::FoundText { index, text },
    }
}

/// A 4 by 2 frame: its left half red and its right half gray.
pub(super) fn frame_image() -> Image {
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
    /// Moves the owner's clock on by `time`, as if that long passed.
    pub(super) fn wait(&mut self, time: Duration) {
        self.owner.host.clock = self.owner.host.clock.checked_add(time).expect("in range");
    }

    /// Calls `command` on the owner directly and returns its answer. How a
    /// `Run` reaches the owner is tested in `messages.rs`.
    pub(super) fn ask<T>(&mut self, command: impl FnOnce(&mut Owner<FakeHost>) -> T) -> T {
        command(&mut self.owner)
    }

    /// Forgets the reloads that arrived so far, so a later
    /// [`Harness::reloads`] counts only those that follow.
    pub(super) fn forget_reloads(&self) {
        while self.reloads.try_recv().is_ok() {}
    }

    /// How many times the owner asked the runtime to read the saved files
    /// again since the last time this was asked.
    pub(super) fn reloads(&self) -> usize {
        self.reloads.try_iter().count()
    }

    /// Stops the capture's recording by its number, as the screen that
    /// started it does.
    pub(super) fn stop_recording(&mut self) {
        let recording = self
            .owner
            .capture
            .recording()
            .expect("a recording to stop")
            .id()
            .number();
        self.owner.stop_recording(recording);
    }

    /// Picks window 7 and returns the capture's handler.
    pub(super) fn pick(&mut self) -> FrameHandler {
        self.pick_numbered().1
    }

    /// Picks window 7 and returns the capture's number, as the page gets
    /// it, with the capture's handler.
    pub(super) fn pick_numbered(&mut self) -> (u64, FrameHandler) {
        let capture = self.ask(|owner| owner.pick(7)).expect("capture starts");
        let started = self.handlers.try_recv().expect("capture started");
        assert_eq!(
            (started.window, started.crop, started.interval),
            (7, Crop::Whole, PREVIEW_INTERVAL)
        );
        (capture, started.handler)
    }

    /// Runs `handler` on `frame`, a whole window's frame, and applies
    /// what it sent the owner.
    pub(super) fn deliver(&mut self, handler: &mut FrameHandler, frame: io::Result<&Image>) {
        let frame = frame.map(|image| (image.size(), image));
        self.deliver_at(handler, frame, PREVIEW_INTERVAL);
    }

    /// Runs `handler` on `frame`, its window's size and pixels, checks
    /// the wait it answers, and applies what it sent the owner.
    pub(super) fn deliver_at(
        &mut self,
        handler: &mut FrameHandler,
        frame: io::Result<(Size, &Image)>,
        interval: Duration,
    ) {
        let message = self.sent(handler, frame, interval);
        assert!(self.owner.handle(message).is_continue());
    }

    /// Runs `handler` on `frame`, a whole window's frame, and applies what
    /// it sent the owner only once `waited` more has passed on the owner's
    /// clock, as if the owner were busy that long while the frame waited.
    pub(super) fn deliver_late(
        &mut self,
        handler: &mut FrameHandler,
        frame: &Image,
        waited: Duration,
    ) {
        let message = self.sent(handler, Ok((frame.size(), frame)), PREVIEW_INTERVAL);
        self.wait(waited);
        assert!(self.owner.handle(message).is_continue());
    }

    /// Runs `handler` on `frame`, which arrives at the time on the owner's
    /// clock, checks the wait it answers, and returns what it sent the
    /// owner.
    fn sent(
        &self,
        handler: &mut FrameHandler,
        frame: io::Result<(Size, &Image)>,
        interval: Duration,
    ) -> Message<FakeHost> {
        let frame = frame.map(|(window_size, image)| Frame {
            window_size,
            image,
            received: self.owner.host.clock,
            delay: Duration::ZERO,
        });
        assert_eq!(handler(frame), NextFrame::After(interval));
        self.messages.try_recv().expect("handler sent")
    }
}

pub(super) fn spec(app: &str, value_name: &str, check: &str) -> SaveSpec {
    SaveSpec {
        app: app.to_owned(),
        value_name: value_name.to_owned(),
        check: serde_json::from_str(check).expect("valid JSON"),
        screenshot: 0,
        pin: appytappy_config::app_file::PinDef::TopLeft,
        reading_area: false,
    }
}

/// `check`, drawn on screenshot 0, to try on the live window for the
/// application `example`, before a side for it to stay at is picked.
pub(super) fn trial(check: &str) -> TrialSpec {
    TrialSpec {
        app: "example".to_owned(),
        check: serde_json::from_str(check).expect("valid JSON"),
        screenshot: 0,
        pin: None,
    }
}

/// The characters `json` describes.
pub(super) fn characters(json: &str) -> CharactersSpec {
    serde_json::from_str(json).expect("valid JSON")
}

/// A recording of `region` of screenshot 0 of the application `example`'s
/// window, of characters of the color `ink` at no tolerance.
pub(super) fn characters_in(region: RegionDef, ink: ColorDef) -> RecordSpec {
    RecordSpec {
        app: "example".to_owned(),
        region,
        screenshot: 0,
        pin: None,
        ink,
        tolerance: 0,
        scope: Scope::Place,
        examples: Vec::new(),
    }
}

/// `characters` tried for the application `example` as a reference made
/// for no number.
pub(super) fn teaching(characters: CharactersSpec) -> ReferenceTrial {
    ReferenceTrial {
        app: "example".to_owned(),
        characters,
        reads: None,
        whole: false,
        pin: None,
    }
}

/// A harness with `Example App` saved, whose window changes size often,
/// and the folder its stores are in, for the test to remove.
pub(super) fn with_example_app(name: &str) -> (Harness, std::path::PathBuf) {
    let (stores, root) = scratch_store(name);
    let mut harness = harness_with(false, Ok(stores));
    add_example_app(&mut harness);
    (harness, root)
}

/// A harness with `Example App` saved and window 7 picked, [`frame_image`]
/// its newest frame and its screenshot 0, with the capture's handler.
pub(super) fn capturing(name: &str) -> (Harness, FrameHandler, std::path::PathBuf) {
    let (mut harness, root) = with_example_app(name);
    let mut handler = harness.pick();
    harness.deliver(&mut handler, Ok(&frame_image()));
    assert_eq!(harness.ask(Owner::screenshot), Ok(0));
    (harness, handler, root)
}

/// `Example App` as the user describes it, with a window that changes
/// size as `size_changes` says.
pub(super) fn example_app(size_changes: SizeChangesDef) -> AppSpec {
    AppSpec {
        name: " Example App ".to_owned(),
        window: WindowDef::exact(appytappy_config::window_match::WindowFacts {
            title: "Example App",
            class: "ExampleWindow",
            executable: "Example.exe",
        }),
        size_changes,
        anti_cheat: None,
    }
}

/// Saves `Example App`, with no values and a window that changes size
/// often, as the application `example`.
pub(super) fn add_example_app(harness: &mut Harness) {
    let saved = harness.ask(|owner| owner.save_app(None, example_app(SizeChangesDef::Often)));
    assert_eq!(saved, Ok("example".to_owned()));
}

pub(super) const STATE: &str = r#"{"kind":"state","region":{"x":0,"y":0,"width":2,"height":2},
    "drawnOn":0,"states":[{"name":"Shown","screenshot":0}],"tolerance":0}"#;

pub(super) const BAR: &str = r#"{"kind":"barFill","region":{"x":0,"y":0,"width":4,"height":2},
    "filled":{"red":255,"green":0,"blue":0},"empty":{"red":40,"green":40,"blue":40}}"#;

/// A harness with the bar `Progress bar` saved for `Example App`, drawn
/// on the whole of window 7's 4 by 2 frame.
pub(super) fn with_saved_bar(name: &str) -> (Harness, std::path::PathBuf) {
    let (mut harness, _handler, root) = capturing(name);
    harness
        .ask(|owner| owner.save(&spec("example", "Progress bar", BAR)))
        .expect("saves");
    (harness, root)
}

pub(super) const WINDOW: Size = Size {
    width: 4,
    height: 2,
};

/// Saves `Other App`, on the same window as `Example App`, as the
/// application `example-2`, with the bar `Progress bar` of its own.
pub(super) fn add_other_bar(harness: &mut Harness) {
    let mut other = example_app(SizeChangesDef::Often);
    "Other App".clone_into(&mut other.name);
    assert_eq!(
        harness.ask(|owner| owner.save_app(None, other)),
        Ok("example-2".to_owned())
    );
    harness
        .ask(|owner| owner.save(&spec("example-2", "Progress bar", BAR)))
        .expect("saves");
}
