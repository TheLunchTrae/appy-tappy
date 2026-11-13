//! The messages the owner's thread receives and the answers it sends back:
//! the work commands and overlay windows send it, the frames its captures hand
//! over, and the shapes both travel in.

use std::fmt;
use std::time::Instant;

use appytappy_config::app_file::{AppFile, BuildDef, CheckDef, PanelDef, PinDef, SizeDef};
use appytappy_config::signal::Value;
use appytappy_sources::screen::{Image, Size};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::check_spec::{CharactersSpec, CheckSpec, ColorSpec, RegionSpec};
use crate::overlay::{FrameRead, Purpose};
use crate::recording::Text;
use crate::window_list::{WindowFit, WindowList};

use super::{Desktop, Host, Owner};

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
#[cfg_attr(test, derive(ts_rs::TS))]
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

/// A value's check to try on the live window before it is saved, as the UI
/// describes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct TrialSpec {
    /// The saved application the value is for, whose references it reads
    /// with and whose window's size changes say where it reads.
    pub(crate) app: String,
    /// The check, with the session's screenshots it names.
    pub(crate) check: CheckSpec,
    /// The screenshot the region was drawn on, whose size the value would
    /// keep.
    pub(crate) screenshot: usize,
    /// The side of the window the region stays at as the window changes
    /// size, or `None` before one is picked, for the side it is nearest.
    pub(crate) pin: Option<PinDef>,
}

/// A reference being taught to try on the live window before it is saved,
/// as the UI describes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReferenceTrial {
    /// The saved application the reference is for, whose window's size
    /// changes say where its region reads.
    pub(crate) app: String,
    /// The characters it is taught.
    pub(crate) characters: CharactersSpec,
    /// The characters the number it is made for reads of it, or `None`
    /// for every character it is taught, as [`CheckDef::Number`] has them.
    pub(crate) reads: Option<String>,
    /// Whether that number ignores digits after the decimal point.
    pub(crate) whole: bool,
    /// The side of the window that number's region stays at, or `None`
    /// for the side the region is nearest.
    pub(crate) pin: Option<PinDef>,
}

/// What a recording of readings records, as the UI describes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecordSpec {
    /// The region of the window to record.
    pub(crate) region: RegionSpec,
    /// The screenshot the region was drawn on, whose window size the
    /// recording keeps.
    pub(crate) screenshot: usize,
    /// The characters' color.
    pub(crate) ink: ColorSpec,
    /// How far from `ink` a pixel of a character may be.
    pub(crate) tolerance: u8,
}

/// A value to save, as the UI describes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
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
    /// The side of the window the region stays at as the window changes
    /// size.
    pub(crate) pin: PinDef,
    /// Whether the region was drawn as a reading area, larger than what it
    /// reads. Only a check that reads anywhere in its region may have one.
    pub(crate) reading_area: bool,
}

/// A saved value drawn again at another window size, as the UI describes
/// it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct FixSpec {
    /// The saved application.
    pub(crate) app: String,
    /// The value's ID.
    pub(crate) value: String,
    /// The check at this size: of the value's kind, with each of its
    /// states in order, or for a number, the reference to read with.
    pub(crate) check: CheckSpec,
    /// The screenshot the region was drawn on, whose size the fix is for.
    pub(crate) screenshot: usize,
}

/// A saved value changed in place, as the UI describes it. The value keeps
/// its ID and the window size it was drawn at.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct EditSpec {
    /// The saved application.
    pub(crate) app: String,
    /// The value's ID.
    pub(crate) value: String,
    /// The value's name, the one it has or a new one.
    pub(crate) value_name: String,
    /// The side of the window the region stays at as the window changes
    /// size.
    pub(crate) pin: PinDef,
    /// Whether the region is a reading area, larger than what it reads.
    /// Only a check that reads anywhere in its region may have one.
    pub(crate) reading_area: bool,
    /// Where the value is read and how.
    pub(crate) drawing: EditDrawing,
}

/// Where an edited value is read and how.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum EditDrawing {
    /// Its region stays where it is saved, so no screenshot is needed and
    /// the window may have any size.
    Kept {
        /// The check as the file spells it. A state check names the
        /// pictures the value has, in their order, each under the name the
        /// state has now.
        check: CheckDef,
    },
    /// Its region is drawn again, on a screenshot of the window size the
    /// value was drawn at.
    Drawn {
        /// The check, with the session's screenshots it names. A state
        /// check takes every state's picture again.
        check: CheckSpec,
        /// The screenshot the region was drawn on.
        screenshot: usize,
    },
}

/// What saving an edit would do, which the editor shows before the save.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct EditPreview {
    /// Why the edited value is not read at the window's size now, or `None`
    /// when the live reading reads it there.
    pub(crate) unread: Option<String>,
    /// The window sizes of the fixes the edit removes, since they no longer
    /// suit how the value reads. The save needs the user's word for it.
    pub(crate) dropped_fixes: Vec<SizeDef>,
    /// Why the save is refused: the automations the edit would break.
    pub(crate) refused: Option<String>,
}

/// A reference to save, as the UI describes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReferenceSaveSpec {
    /// The saved application to add the reference to.
    pub(crate) app: String,
    /// The reference's name, such as `Percent font`.
    pub(crate) reference_name: String,
    /// The characters it is taught, which name the reference when it is
    /// saved already.
    pub(crate) characters: CharactersSpec,
    /// The characters the user says it needs to know, or empty when the
    /// user chose none. The save lists each once, in order, refuses more
    /// than a reference saves, and replaces what the saved reference
    /// expected.
    pub(crate) expects: String,
}

/// Where a reference was saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "SavedReference"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct SavedReferenceId {
    /// The application's ID.
    pub(crate) app: String,
    /// The reference's ID.
    pub(crate) reference: String,
}

/// Where a value was saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct SavedValue {
    /// The application's ID.
    pub(crate) app: String,
    /// The value's ID.
    pub(crate) value: String,
}

/// A saved application, as the Values page lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppSummary {
    /// The application's ID.
    pub(crate) id: String,
    /// The file, or `None` when it is broken.
    pub(crate) file: Option<AppFile>,
    /// Why the file is broken.
    pub(crate) error: Option<String>,
    /// The handles of the running windows that match, the best fit first.
    #[cfg_attr(test, ts(type = "Array<number>"))]
    pub(crate) windows: Vec<u64>,
    /// The build the best fit's executable runs, or `None` while no window
    /// matches or its build cannot be read.
    pub(crate) running: Option<BuildDef>,
}

/// A saved application with the open windows its match takes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppFits {
    /// The application.
    pub(crate) app: AppSummary,
    /// The open windows its match takes, the one a new session would read
    /// first; none for a broken file.
    pub(crate) fits: Vec<WindowFit>,
}

/// The saved applications and the open windows, from one listing of the
/// desktop, as the picker of a value or reference shows them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppWindows {
    /// The open windows, which every application's fits are among.
    pub(crate) open: WindowList,
    /// Each saved application.
    pub(crate) apps: Vec<AppFits>,
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

// The work is a closure, which cannot be printed, so a `Run` prints only its
// name.
impl<H: Host> fmt::Debug for Run<H> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Run")
    }
}

/// What reaches the owner thread. Its host is the operating system's,
/// except in the tests.
#[derive(Debug)]
pub(crate) enum Message<H: Host = Desktop> {
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
    /// The text reader read find `index` of search `search`.
    FoundText {
        /// Which search it belongs to; text for an earlier one is dropped.
        search: u64,
        /// The find's index.
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
    /// An application's overlay, or the Values page's reading of its
    /// values, read them, or stopped and why.
    Overlay {
        /// Who the capture reads for.
        purpose: Purpose,
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
        assert_eq!(format!("{run:?}"), "Run");
        assert!(harness.owner.handle(Message::Run(run)).is_continue());
        assert_eq!(told.try_recv(), Ok(()));
    }
}
