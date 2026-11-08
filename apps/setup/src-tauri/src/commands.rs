//! The commands the web UI calls.
//!
//! Each one sends a request to the owner thread and awaits its answer, so a
//! slow answer never freezes the window. The commands take the app's handle
//! and look the bridge up on it, rather than borrowing it as an argument,
//! because Tauri checks a borrowing async command with code the lint wall
//! refuses. Pixels go back as raw bytes rather
//! than JSON, which would be several times larger.

use std::sync::mpsc::Sender;

use appytappy_config::app_file::{AntiCheatDef, OverlayModeDef, WindowDef};
use appytappy_config::automation_file::{AutomationDef, WhenDef};
use tauri::ipc::Response;
use tauri::{AppHandle, Manager};
use tokio::sync::oneshot;

use crate::applications::AppSpec;
use crate::automations::AutomationSummary;
use crate::check_spec::{CheckSpec, ColorSpec, RegionSpec};
use crate::overlay::{self, Card, OverlayView};
use crate::owner::{AppSummary, Live, Message, Reading, Reply, Request, SaveSpec, SavedValue};
use crate::recording::RecordingView;
use crate::window_list::{WindowFit, WindowList};

/// The UI's way to the owner thread.
#[derive(Debug)]
pub(crate) struct Bridge(pub(crate) Sender<Message>);

/// The message for a command whose answer never came.
const GONE: &str = "AppyTappy setup is closing.";

/// Sends the request `build` makes to the owner thread and awaits its
/// answer.
async fn ask<T>(app: &AppHandle, build: impl FnOnce(Reply<T>) -> Request) -> Result<T, String> {
    let (reply, answer) = oneshot::channel();
    app.state::<Bridge>()
        .0
        .send(Message::Request(build(reply)))
        .map_err(|_closed| GONE.to_owned())?;
    answer.await.map_err(|_dropped| GONE.to_owned())
}

/// Pixels for the UI, or no bytes at all when there are none yet.
fn pixels(bytes: Option<Vec<u8>>) -> Response {
    Response::new(bytes.unwrap_or_default())
}

/// The windows the user can pick, with the anti-cheat products running now.
#[tauri::command]
pub(crate) async fn list_windows(app: AppHandle) -> Result<WindowList, String> {
    ask(&app, Request::ListWindows).await?
}

/// Starts capturing `window` for the preview.
#[tauri::command]
pub(crate) async fn pick_window(app: AppHandle, window: u64) -> Result<(), String> {
    ask(&app, |reply| Request::Pick { window, reply }).await?
}

/// The newest frame, at most `max_width` wide; no bytes until one arrives.
#[tauri::command]
pub(crate) async fn preview(app: AppHandle, max_width: u32) -> Result<Response, String> {
    ask(&app, |reply| Request::Preview { max_width, reply })
        .await?
        .map(pixels)
}

/// Stops capturing and forgets the session's screenshots and check.
#[tauri::command]
pub(crate) async fn stop_capture(app: AppHandle) -> Result<(), String> {
    ask(&app, Request::Stop).await
}

/// Keeps the newest frame as a screenshot and returns its index.
#[tauri::command]
pub(crate) async fn take_screenshot(app: AppHandle) -> Result<usize, String> {
    ask(&app, Request::Screenshot).await?
}

/// A screenshot, at most `max_width` wide; no bytes for an unknown index.
#[tauri::command]
pub(crate) async fn screenshot(
    app: AppHandle,
    index: usize,
    max_width: u32,
) -> Result<Response, String> {
    ask(&app, |reply| Request::ScreenshotPixels {
        index,
        max_width,
        reply,
    })
    .await
    .map(pixels)
}

/// Sets up the check the live reading uses.
#[tauri::command]
pub(crate) async fn set_check(app: AppHandle, spec: CheckSpec) -> Result<(), String> {
    ask(&app, |reply| Request::SetCheck { spec, reply }).await?
}

/// What the check reads on the newest frame, if a check and a frame exist.
#[tauri::command]
pub(crate) async fn read_value(app: AppHandle) -> Result<Option<Reading>, String> {
    ask(&app, Request::Reading).await?
}

/// Captures the region of the saved value `value` of the application
/// `application` on its window, for the automation editor, and returns the
/// watch's number.
#[tauri::command]
pub(crate) async fn watch_value(
    app: AppHandle,
    application: String,
    value: String,
) -> Result<u64, String> {
    ask(&app, |reply| Request::WatchValue {
        app: application,
        value,
        reply,
    })
    .await?
}

/// What the watch's value reads now, and whether `when` holds for it.
#[tauri::command]
pub(crate) async fn watched_reading(
    app: AppHandle,
    watch: u64,
    when: Option<WhenDef>,
) -> Result<Live, String> {
    ask(&app, |reply| Request::WatchedReading {
        watch,
        when: when.map(Box::new),
        reply,
    })
    .await?
}

/// Stops the watch, unless another capture replaced it already.
#[tauri::command]
pub(crate) async fn stop_watching(app: AppHandle, watch: u64) -> Result<(), String> {
    ask(&app, |reply| Request::StopWatching { watch, reply }).await
}

/// The running windows `window` takes, the best fit first.
#[tauri::command]
pub(crate) async fn match_windows(
    app: AppHandle,
    window: WindowDef,
) -> Result<Vec<WindowFit>, String> {
    ask(&app, |reply| Request::MatchWindows { window, reply }).await?
}

/// The saved applications and their values.
#[tauri::command]
pub(crate) async fn list_apps(app: AppHandle) -> Result<Vec<AppSummary>, String> {
    ask(&app, Request::ListApps).await?
}

/// Saves an application's name and window match over the application
/// `existing`, or as a new application; answers its ID.
#[tauri::command]
pub(crate) async fn save_app(
    app: AppHandle,
    existing: Option<String>,
    spec: AppSpec,
) -> Result<String, String> {
    ask(&app, |reply| Request::SaveApp {
        existing,
        spec,
        reply,
    })
    .await?
}

/// Removes an application with its values, unless an automation watches one
/// of them.
#[tauri::command]
pub(crate) async fn remove_app(app: AppHandle, application: String) -> Result<(), String> {
    ask(&app, |reply| Request::RemoveApp {
        app: application,
        reply,
    })
    .await?
}

/// Saves a value to the application `spec` names.
#[tauri::command]
pub(crate) async fn save_value(app: AppHandle, spec: SaveSpec) -> Result<SavedValue, String> {
    let spec = Box::new(spec);
    ask(&app, |reply| Request::SaveValue { spec, reply }).await?
}

/// Removes a saved value.
#[tauri::command]
pub(crate) async fn delete_value(
    app: AppHandle,
    application: String,
    value: String,
) -> Result<(), String> {
    ask(&app, |reply| Request::DeleteValue {
        app: application,
        value,
        reply,
    })
    .await?
}

/// Sets how an application protects itself from other programs.
#[tauri::command]
pub(crate) async fn set_anti_cheat(
    app: AppHandle,
    application: String,
    class: AntiCheatDef,
) -> Result<(), String> {
    ask(&app, |reply| Request::SetAntiCheat {
        app: application,
        class,
        reply,
    })
    .await?
}

/// The saved automations, with what stops each one from running.
#[tauri::command]
pub(crate) async fn list_automations(app: AppHandle) -> Result<Vec<AutomationSummary>, String> {
    ask(&app, Request::ListAutomations).await?
}

/// Saves `automation` over `existing`, or as a new one; returns its ID.
#[tauri::command]
pub(crate) async fn save_automation(
    app: AppHandle,
    existing: Option<String>,
    automation: AutomationDef,
) -> Result<String, String> {
    let automation = Box::new(automation);
    ask(&app, |reply| Request::SaveAutomation {
        existing,
        automation,
        reply,
    })
    .await?
}

/// Turns an automation on or off.
#[tauri::command]
pub(crate) async fn set_automation_enabled(
    app: AppHandle,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    ask(&app, |reply| Request::SetAutomationEnabled {
        id,
        enabled,
        reply,
    })
    .await?
}

/// Removes an automation.
#[tauri::command]
pub(crate) async fn delete_automation(app: AppHandle, id: String) -> Result<(), String> {
    ask(&app, |reply| Request::DeleteAutomation { id, reply }).await?
}

/// Opens a window for each overlay that is on, and closes the rest.
#[tauri::command]
pub(crate) async fn sync_overlays(app: AppHandle) -> Result<(), String> {
    let wanted = ask(&app, Request::Overlays).await??;
    overlay::window::sync(&app, &wanted).map_err(|error| error.to_string())
}

/// Turns `application`'s overlay on or off and sets where it shows, then
/// opens or closes its window to match.
#[tauri::command]
pub(crate) async fn set_overlay(
    app: AppHandle,
    application: String,
    enabled: bool,
    mode: OverlayModeDef,
) -> Result<(), String> {
    ask(&app, |reply| Request::SetOverlay {
        app: application,
        enabled,
        mode,
        reply,
    })
    .await??;
    sync_overlays(app).await
}

/// What `application`'s overlay shows now.
#[tauri::command]
pub(crate) async fn overlay_state(
    app: AppHandle,
    application: String,
) -> Result<OverlayView, String> {
    ask(&app, |reply| Request::OverlayState {
        app: application,
        reply,
    })
    .await?
}

/// Keeps the size of `application`'s overlay card in logical pixels, which
/// its window over the application takes.
#[tauri::command]
pub(crate) async fn overlay_card(
    app: AppHandle,
    application: String,
    width: f64,
    height: f64,
) -> Result<(), String> {
    ask(&app, |reply| Request::OverlayCard {
        app: application,
        card: Card { width, height },
        reply,
    })
    .await
}

/// Records `region` of the picked window, drawn on `screenshot`, keeping
/// each reading it shows. With `more`, goes on with the stopped recording;
/// otherwise starts a new one.
#[tauri::command]
pub(crate) async fn start_recording(
    app: AppHandle,
    region: RegionSpec,
    ink: ColorSpec,
    tolerance: u8,
    screenshot: usize,
    more: bool,
) -> Result<(), String> {
    ask(&app, |reply| Request::StartRecording {
        region,
        ink,
        tolerance,
        screenshot,
        more,
        reply,
    })
    .await?
}

/// Stops the recording, keeping its readings for review.
#[tauri::command]
pub(crate) async fn stop_recording(app: AppHandle) -> Result<(), String> {
    ask(&app, Request::StopRecording).await
}

/// Forgets the recording and its readings.
#[tauri::command]
pub(crate) async fn discard_recording(app: AppHandle) -> Result<(), String> {
    ask(&app, Request::DiscardRecording).await
}

/// The recording and its readings so far; `None` without one.
#[tauri::command]
pub(crate) async fn recording_state(app: AppHandle) -> Result<Option<RecordingView>, String> {
    ask(&app, Request::RecordingState).await
}

/// A recorded reading, at most `max_width` wide; no bytes for an unknown
/// index.
#[tauri::command]
pub(crate) async fn recorded_reading(
    app: AppHandle,
    index: usize,
    max_width: u32,
) -> Result<Response, String> {
    ask(&app, |reply| Request::RecordedPixels {
        index,
        max_width,
        reply,
    })
    .await
    .map(pixels)
}

/// Why the number check `spec` describes cannot learn each of its recorded
/// readings, in order: `None` for one it can.
#[tauri::command]
pub(crate) async fn review_problems(
    app: AppHandle,
    spec: CheckSpec,
) -> Result<Vec<Option<String>>, String> {
    ask(&app, |reply| Request::ReviewProblems { spec, reply }).await?
}
