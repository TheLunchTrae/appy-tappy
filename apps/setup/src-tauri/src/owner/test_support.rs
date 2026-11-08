//! The stand-ins the owner's tests share: a host whose captures the test drives
//! by hand, and a harness that asks the owner and delivers frames.

use std::io;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use appytappy_config::anti_cheat::ProcessId;
use appytappy_config::app_file::WindowDef;
use appytappy_config::store::{AppStore, AutomationStore};
use appytappy_sources::screen::{CaptureSession, Crop, Frame, FrameHandler, Image, Size};
use appytappy_sources::system::{Bounds, ProcessEntry, WindowEntry, WindowPlace};
use tokio::sync::oneshot;

use crate::applications::AppSpec;
use crate::recording::ocr;
use crate::stores::Stores;

use super::host::Host;
use super::messages::{Message, Reply, Request, SaveSpec};
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

pub(super) struct Harness {
    pub(super) owner: Owner<FakeHost>,
    pub(super) messages: Receiver<Message>,
    pub(super) handlers: Receiver<Started>,
    pub(super) closed: Receiver<u64>,
    /// What the owner sends the text reader.
    pub(super) jobs: Receiver<ocr::Job>,
}

/// Fresh, empty stores for one test, all in one folder.
pub(super) fn scratch_store(name: &str) -> (Stores, std::path::PathBuf) {
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

pub(super) fn harness(refuse: bool) -> Harness {
    harness_with(refuse, Err("no store in this test".to_owned()))
}

pub(super) fn harness_with(refuse: bool, store: Result<Stores, String>) -> Harness {
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
    /// Sends `request` built around a fresh reply channel and returns the
    /// answer.
    pub(super) fn ask<T>(&mut self, request: impl FnOnce(Reply<T>) -> Request) -> T {
        let (reply, answer) = oneshot::channel();
        assert!(
            self.owner
                .handle(Message::Request(request(reply)))
                .is_continue()
        );
        answer.blocking_recv().expect("answered")
    }

    /// Picks window 7 and returns the capture's handler.
    pub(super) fn pick(&mut self) -> FrameHandler {
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
    }
}

/// Saves `Example App`, with no values, as the application `example`.
pub(super) fn add_example_app(harness: &mut Harness) {
    let saved = harness.ask(|reply| Request::SaveApp {
        existing: None,
        spec: AppSpec {
            name: " Example App ".to_owned(),
            window: WindowDef::exact(appytappy_config::window_match::WindowFacts {
                title: "Example App",
                class: "ExampleWindow",
                executable: "Example.exe",
            }),
        },
        reply,
    });
    assert_eq!(saved, Ok("example".to_owned()));
}

pub(super) const REFERENCE: &str = r#"{"kind":"reference","region":{"x":0,"y":0,"width":2,"height":2},
    "screenshot":0,"tolerance":0}"#;

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
    assert_eq!(harness.ask(Request::Screenshot), Ok(0));
    harness
        .ask(|reply| Request::SaveValue {
            spec: Box::new(spec("example", "Progress bar", BAR)),
            reply,
        })
        .expect("saves");
    (harness, root)
}

pub(super) const WINDOW: Size = Size {
    width: 4,
    height: 2,
};
