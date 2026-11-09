//! The stand-ins the owner's tests share: a host whose captures the test drives
//! by hand, and a harness that asks the owner and delivers frames.

use std::io;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use appytappy_config::anti_cheat::ProcessId;
use appytappy_config::app_file::{BuildDef, WindowDef};
use appytappy_sources::screen::{CaptureSession, Crop, Frame, FrameHandler, Image, Size};
use appytappy_sources::system::{Bounds, ProcessEntry, WindowEntry, WindowPlace};

use crate::applications::AppSpec;
use crate::overlay::OverlayWindow;
use crate::recording::ocr;
use crate::stores::{self, Stores};

use super::host::Host;
use super::messages::{Message, SaveSpec};
use super::{Owner, PREVIEW_INTERVAL};

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
        Ok(vec![
            own,
            WindowEntry {
                handle: 7,
                process: ProcessId::new(self.process),
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
                id: ProcessId::new(self.process),
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

    fn now(&self) -> Instant {
        self.clock
    }

    fn reload_runtime(&self) {
        self.reloads.send(()).expect("test listens");
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
    /// Each time the owner asked the runtime to read the saved files again.
    reloads: Receiver<()>,
    /// The overlay windows wanted each time the owner opened and closed
    /// them to match.
    pub(super) synced: Receiver<Vec<OverlayWindow>>,
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
    let (reload_sender, reloads) = mpsc::channel();
    let (synced_sender, synced) = mpsc::channel();
    let host = FakeHost {
        handlers: handler_sender,
        closed: closed_sender,
        refuse,
        unlisted: false,
        minimized: false,
        process: 3,
        build: Some(example_build()),
        reloads: reload_sender,
        synced: synced_sender,
        unsynced: None,
        clock: Instant::now(),
    };
    Harness {
        owner: Owner::new(host, store, OWN_PROCESS, sender, job_sender),
        messages,
        handlers,
        closed,
        jobs,
        reloads,
        synced,
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

    /// Picks window 7 and returns the capture's handler.
    pub(super) fn pick(&mut self) -> FrameHandler {
        self.ask(|owner| owner.pick(7)).expect("capture starts");
        let started = self.handlers.try_recv().expect("capture started");
        assert_eq!(
            (started.window, started.crop, started.interval),
            (7, Crop::Whole, PREVIEW_INTERVAL)
        );
        started.handler
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

pub(super) fn spec(app: &str, value_name: &str, check: &str) -> SaveSpec {
    SaveSpec {
        app: app.to_owned(),
        value_name: value_name.to_owned(),
        check: serde_json::from_str(check).expect("valid JSON"),
        screenshot: 0,
        pin: appytappy_config::app_file::PinDef::TopLeft,
    }
}

/// Saves `Example App`, with no values, as the application `example`.
pub(super) fn add_example_app(harness: &mut Harness) {
    let saved = harness.ask(|owner| {
        owner.save_app(
            None,
            AppSpec {
                name: " Example App ".to_owned(),
                window: WindowDef::exact(appytappy_config::window_match::WindowFacts {
                    title: "Example App",
                    class: "ExampleWindow",
                    executable: "Example.exe",
                }),
                size_changes: appytappy_config::app_file::SizeChangesDef::Often,
            },
        )
    });
    assert_eq!(saved, Ok("example".to_owned()));
}

pub(super) const STATE: &str = r#"{"kind":"state","region":{"x":0,"y":0,"width":2,"height":2},
    "drawnOn":0,"states":[{"name":"Shown","screenshot":0}],"tolerance":0}"#;

pub(super) const BAR: &str = r#"{"kind":"barFill","region":{"x":0,"y":0,"width":4,"height":2},
    "filled":{"red":255,"green":0,"blue":0},"empty":{"red":40,"green":40,"blue":40}}"#;

/// A harness with the bar `Progress bar` saved for `Example App`, drawn
/// on the whole of window 7's 4 by 2 frame.
pub(super) fn with_saved_bar(name: &str) -> (Harness, std::path::PathBuf) {
    let (stores, root) = scratch_store(name);
    let mut harness = harness_with(false, Ok(stores));
    add_example_app(&mut harness);
    let mut handler = harness.pick();
    harness.deliver(&mut handler, Ok(&frame_image()));
    assert_eq!(harness.ask(Owner::screenshot), Ok(0));
    harness
        .ask(|owner| owner.save(&spec("example", "Progress bar", BAR)))
        .expect("saves");
    (harness, root)
}

pub(super) const WINDOW: Size = Size {
    width: 4,
    height: 2,
};
