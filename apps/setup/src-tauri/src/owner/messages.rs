//! The messages the owner's thread receives and the answers it sends back:
//! the work commands and overlay windows send it, the frames its captures hand
//! over, and the shapes both travel in.

use std::fmt;
use std::time::Instant;

use appytappy_config::app_file::{AppFile, BuildDef, PanelDef};
use appytappy_config::signal::Value;
use appytappy_sources::HostSystem;
use appytappy_sources::screen::{Image, Size};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::check_spec::CheckSpec;
use crate::overlay::FrameRead;
use crate::recording::Text;

use super::{Host, Owner};

/// Where a request's answer goes.
pub(crate) type Reply<T> = oneshot::Sender<T>;

/// Why the picked window's capture failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CaptureFailure {
    /// Why, for the user.
    pub(crate) error: String,
    /// Whether the window closed, which ends the capture for good. Any other
    /// failure lasts only until the next good frame.
    pub(crate) ended: bool,
}

/// A value the check read, as the UI shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub(crate) enum Reading {
    /// A number, such as a percentage.
    Number(f64),
    /// Yes or no, such as whether a color shows.
    Flag(bool),
    /// A state's name, such as `Paused`, or `none`.
    State(String),
}

impl From<Value> for Reading {
    fn from(value: Value) -> Self {
        match value {
            Value::Number(number) => Self::Number(number.raw()),
            Value::Flag(flag) => Self::Flag(flag),
            Value::State(state) => Self::State(state.as_str().to_owned()),
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
    /// The saved application to add the value to.
    pub(crate) app: String,
    /// The value's name, such as `Progress`.
    pub(crate) value_name: String,
    /// The check, with the session's screenshots it names.
    pub(crate) check: CheckSpec,
    /// The screenshot the region was drawn on, whose size the value keeps.
    pub(crate) screenshot: usize,
}

/// A reference to save, as the UI describes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReferenceSaveSpec {
    /// The saved application to add the reference to.
    pub(crate) app: String,
    /// The reference's name, such as `Percent font`.
    pub(crate) reference_name: String,
    /// The characters it is taught, as a [`CheckSpec::Characters`] that
    /// names the reference when it is saved already.
    pub(crate) check: CheckSpec,
}

/// Where a reference was saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SavedReferenceId {
    /// The application's ID.
    pub(crate) app: String,
    /// The reference's ID.
    pub(crate) reference: String,
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
    /// The build the best fit's executable runs, or `None` while no window
    /// matches or its build cannot be read.
    pub(crate) running: Option<BuildDef>,
}

/// Work for the owner thread, from a command or an overlay window, which it
/// runs with the state and which sends its own answer, as
/// [`ask`](crate::commands) builds it.
pub(crate) struct Run<H: Host>(Work<H>);

/// Work for the owner thread, which answers on its own if it answers.
type Work<H> = Box<dyn FnOnce(&mut Owner<H>) + Send>;

impl<H: Host> Run<H> {
    /// Work that runs `work` on the owner and sends what it returns to
    /// `reply`.
    pub(crate) fn new<T: Send + 'static>(
        reply: Reply<T>,
        work: impl FnOnce(&mut Owner<H>) -> T + Send + 'static,
    ) -> Self {
        Self(Box::new(move |owner| {
            // A dropped command, such as one whose window closed while it
            // waited, needs no answer.
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a dropped command needs no answer"
            )]
            let _ = reply.send(work(owner));
        }))
    }

    /// Work that runs `work` on the owner, for a sender that waits for no
    /// answer.
    pub(crate) fn unanswered(work: impl FnOnce(&mut Owner<H>) + Send + 'static) -> Self {
        Self(Box::new(work))
    }

    /// Runs the work on `owner`.
    pub(super) fn run(self, owner: &mut Owner<H>) {
        (self.0)(owner);
    }
}

impl<H: Host> fmt::Debug for Run<H> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Run")
    }
}

/// What reaches the owner thread. Its host is the operating system's,
/// except in the tests.
#[derive(Debug)]
pub(crate) enum Message<H: Host = HostSystem> {
    /// Work from a command or an overlay window.
    Run(Run<H>),
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
        /// Why, and whether the capture ended.
        failure: CaptureFailure,
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

#[cfg(test)]
mod tests {
    use appytappy_config::signal::StateName;

    use super::*;

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
            serde_json::to_value(Reading::from(Value::State(StateName::none())))
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
    fn a_command_runs_on_the_owner_and_answers() {
        let mut harness = crate::owner::test_support::harness(false);
        let (reply, mut answer) = oneshot::channel();
        let run = Run::new(reply, Owner::screenshot);
        assert!(harness.owner.handle(Message::Run(run)).is_continue());
        assert_eq!(
            answer.try_recv(),
            Ok(Err("No frame has arrived from the window yet.".to_owned()))
        );

        let (sender, told) = std::sync::mpsc::channel();
        let run = Run::unanswered(move |owner| {
            owner.stop();
            sender.send(()).expect("test listens");
        });
        // The work is a closure, so a logged message names only what it is.
        assert_eq!(format!("{run:?}"), "Run");
        assert!(harness.owner.handle(Message::Run(run)).is_continue());
        assert_eq!(told.try_recv(), Ok(()));
    }
}
