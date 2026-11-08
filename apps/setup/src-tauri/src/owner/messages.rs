//! The messages the owner's thread receives and the answers it sends back: the
//! interface's requests, the frames its captures hand over, and the shapes both
//! travel in.

use std::time::Instant;

use appytappy_config::app_file::{
    AntiCheatDef, AppFile, BuildDef, OverlayModeDef, PanelDef, WindowDef,
};
use appytappy_config::automation_file::{AutomationDef, WhenDef};
use appytappy_config::signal::Value;
use appytappy_sources::screen::{Image, Size};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::applications::AppSpec;
use crate::automations::AutomationSummary;
use crate::check_spec::{CheckSpec, ColorSpec, RegionSpec};
use crate::overlay::{Card, FrameRead, OverlayView, OverlayWindow, Place};
use crate::recording::{RecordingView, Text};
use crate::selection::{SelectionSpec, SelectionView};
use crate::window_list::{WindowFit, WindowList};

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
    /// [`pixels::rgba`](crate::pixels::rgba) encodes it: `None` until a frame arrives.
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
    /// What `selection`'s steps select on a screenshot.
    Select {
        /// The screenshot's index.
        screenshot: usize,
        /// The steps the user took on it.
        selection: SelectionSpec,
        /// The selection, or `None` when the steps leave nothing selected;
        /// an error for a screenshot that does not exist.
        reply: Reply<Result<Option<SelectionView>, String>>,
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
    /// each reading it shows. With `more`, goes on with the stopped
    /// recording, which must be of the same region, ink, and window size;
    /// otherwise starts a new one in place of any recording.
    StartRecording {
        /// The region to record.
        region: RegionSpec,
        /// The characters' color.
        ink: ColorSpec,
        /// How far a pixel may be from `ink` and still be ink.
        tolerance: u8,
        /// The screenshot the region was drawn on.
        screenshot: usize,
        /// Whether to go on with the recording rather than start over.
        more: bool,
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
    /// Why the number check `spec` describes could not learn each of its
    /// recorded readings, such as one whose shape an earlier reading or a
    /// labeled screenshot gave to another character.
    ReviewProblems {
        /// The check with the readings review would keep.
        spec: CheckSpec,
        /// One answer per recorded reading, in order: `None` for one it
        /// learns. An error when the labeled screenshots cannot be learned.
        reply: Reply<Result<Vec<Option<String>>, String>>,
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
        /// The windows it takes, the best fit first.
        reply: Reply<Result<Vec<WindowFit>, String>>,
    },
    /// The saved applications.
    ListApps(Reply<Result<Vec<AppSummary>, String>>),
    /// Saves an application's name and window match, as a new application
    /// when `existing` is `None`.
    SaveApp {
        /// The application's ID, or `None` for a new one.
        existing: Option<String>,
        /// Its name and window match.
        spec: AppSpec,
        /// Its ID, or what to fix.
        reply: Reply<Result<String, String>>,
    },
    /// Removes an application with its values, unless an automation watches
    /// one of them.
    RemoveApp {
        /// The application's ID.
        app: String,
        /// Answered once it is removed, or with why it cannot be.
        reply: Reply<Result<(), String>>,
    },
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
    /// Saves a reference, replacing the one of the same ID.
    SaveReference {
        /// The reference.
        spec: Box<ReferenceSaveSpec>,
        /// Where it was saved, or what to fix.
        reply: Reply<Result<SavedReferenceId, String>>,
    },
    /// Removes a saved reference and its pictures, unless a value reads
    /// with it.
    DeleteReference {
        /// The application's ID.
        app: String,
        /// The reference's ID.
        reference: String,
        /// Answered once it is gone.
        reply: Reply<Result<(), String>>,
    },
    /// Records the build the application runs now as the one its values
    /// were checked against.
    MarkChecked {
        /// The application's ID.
        app: String,
        /// Answered once it is saved, or with why it cannot be.
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

impl Request {
    /// Whether the request changes the saved applications or automations,
    /// which the runtime reads.
    pub(super) const fn saves(&self) -> bool {
        matches!(
            self,
            Self::SaveValue { .. }
                | Self::DeleteValue { .. }
                | Self::SaveApp { .. }
                | Self::RemoveApp { .. }
                | Self::SaveReference { .. }
                | Self::DeleteReference { .. }
                | Self::SetAntiCheat { .. }
                | Self::MarkChecked { .. }
                | Self::SaveAutomation { .. }
                | Self::SetAutomationEnabled { .. }
                | Self::DeleteAutomation { .. }
        )
    }

    /// Whether the request changes an application's values, the references
    /// they read with, or its overlay, which each overlay then reads again.
    pub(super) const fn changes_values(&self) -> bool {
        matches!(
            self,
            Self::SaveValue { .. }
                | Self::DeleteValue { .. }
                | Self::SaveApp { .. }
                | Self::RemoveApp { .. }
                | Self::SaveReference { .. }
                | Self::DeleteReference { .. }
                | Self::SetOverlay { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use appytappy_config::signal::StateName;

    use crate::owner::test_support::{BAR, spec};

    use super::*;

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
                spec: Box::new(spec("example", "x", BAR)),
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
            Request::MarkChecked {
                app: "example".to_owned(),
                reply: oneshot::channel().0,
            },
            Request::SaveApp {
                existing: Some("example".to_owned()),
                spec: crate::applications::AppSpec {
                    name: "Example App".to_owned(),
                    window: WindowDef::exact(appytappy_config::window_match::WindowFacts {
                        title: "Example App",
                        class: "ExampleWindow",
                        executable: "Example.exe",
                    }),
                },
                reply: oneshot::channel().0,
            },
            Request::RemoveApp {
                app: "example".to_owned(),
                reply: oneshot::channel().0,
            },
        ];
        assert!(saves.iter().all(Request::saves));
        assert!(!Request::ListApps(oneshot::channel().0).saves());
        assert!(!Request::ListAutomations(oneshot::channel().0).saves());
    }

    #[test]
    fn a_saved_or_deleted_reference_is_read_again_by_the_runtime_and_overlays() {
        let spec: ReferenceSaveSpec = serde_json::from_value(serde_json::json!({
            "app": "example",
            "referenceName": "Digits",
            "check": {
                "kind": "characters",
                "region": {"x": 0, "y": 0, "width": 1, "height": 1},
                "screenshot": 0,
                "ink": {"red": 0, "green": 0, "blue": 0},
                "tolerance": 0,
                "labels": []
            }
        }))
        .expect("the UI's spelling");
        let changes = [
            Request::SaveReference {
                spec: Box::new(spec),
                reply: oneshot::channel().0,
            },
            Request::DeleteReference {
                app: "example".to_owned(),
                reference: "digits".to_owned(),
                reply: oneshot::channel().0,
            },
        ];
        assert!(changes.iter().all(Request::saves));
        assert!(changes.iter().all(Request::changes_values));
        assert!(!Request::ListApps(oneshot::channel().0).changes_values());
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
}
