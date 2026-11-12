//! The commands the web UI calls.
//!
//! Each one sends the owner thread work to run with the setup app's state
//! and awaits its answer, so a slow answer never freezes the window. The
//! work is a call of one of the owner's methods, so a new command needs a
//! method there and a command here. The commands take the app's handle
//! and look the bridge up on it, rather than borrowing it as an argument,
//! because Tauri checks a borrowing async command with code the lint wall
//! refuses. Pixels go back as raw bytes rather
//! than JSON, which would be several times larger.

pub(crate) mod table;

use std::sync::mpsc::Sender;
use std::time::Instant;

use appytappy_config::app_file::{AntiCheatDef, OverlayModeDef, SizeDef, WindowDef};
use appytappy_config::automation_file::{AutomationDef, ClickAtDef, WhenDef};
use appytappy_config::settings_file::SettingsFile;
use tauri::ipc::Response;
use tauri::{AppHandle, Manager};
use tokio::sync::oneshot;

use crate::applications::AppSpec;
use crate::automations::AutomationSummary;
use crate::check_spec::{CheckSpec, ColorSpec, RegionSpec};
use crate::debug::bundle::BundleSpec;
use crate::debug::workings::ReadingDetails;
use crate::debug::{DebugOverview, LogName};
use crate::overlay::{Card, OverlayView};
use crate::owner::{
    AppSummary, ClickPictureView, Desktop, FixSpec, FoundClick, Live, Message, Owner, Reading,
    ReferenceSaveSpec, Run, SaveSpec, SavedReferenceId, SavedValue,
};
use crate::recording::RecordingView;
use crate::selection::{SelectionSpec, SelectionView};
use crate::settings::SettingsView;
use crate::window_list::{WindowFit, WindowList};

/// The UI's way to the owner thread.
#[derive(Debug)]
pub(crate) struct Bridge(pub(crate) Sender<Message>);

/// The message for a command whose answer never came.
const GONE: &str = "AppyTappy Setup is closing.";

/// Runs `work` on the owner thread, with the setup app's state, and awaits
/// what it returns. `command` is the command's name, which `setup.log`
/// records there with how long it took from being sent.
pub(crate) async fn ask<T: Send + 'static>(
    app: &AppHandle,
    command: &'static str,
    work: impl FnOnce(&mut Owner<Desktop>) -> T + Send + 'static,
) -> Result<T, String> {
    let sent = Instant::now();
    run(app, move |owner| {
        let answer = work(owner);
        owner.record_command(command, sent.elapsed(), None);
        answer
    })
    .await
}

/// [`ask`] for work that can fail, whose failure `setup.log` records.
pub(crate) async fn try_ask<T: Send + 'static>(
    app: &AppHandle,
    command: &'static str,
    work: impl FnOnce(&mut Owner<Desktop>) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let sent = Instant::now();
    run(app, move |owner| {
        let answer = work(owner);
        owner.record_command(command, sent.elapsed(), answer.as_ref().err());
        answer
    })
    .await?
}

/// Sends `work` to the owner thread and awaits its answer.
async fn run<T: Send + 'static>(
    app: &AppHandle,
    work: impl FnOnce(&mut Owner<Desktop>) -> T + Send + 'static,
) -> Result<T, String> {
    let (reply, answer) = oneshot::channel();
    app.state::<Bridge>()
        .0
        .send(Message::Run(Run::new(reply, work)))
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
    try_ask(&app, "list_windows", |owner| owner.window_list()).await
}

/// Starts capturing `window` for the preview.
#[tauri::command]
pub(crate) async fn pick_window(app: AppHandle, window: u64) -> Result<(), String> {
    try_ask(&app, "pick_window", move |owner| owner.pick(window)).await
}

/// The newest frame, at most `max_width` wide; no bytes until one arrives.
#[tauri::command]
pub(crate) async fn preview(app: AppHandle, max_width: u32) -> Result<Response, String> {
    try_ask(&app, "preview", move |owner| owner.preview(max_width))
        .await
        .map(pixels)
}

/// Stops capturing and forgets the session's screenshots and check.
#[tauri::command]
pub(crate) async fn stop_capture(app: AppHandle) -> Result<(), String> {
    ask(&app, "stop_capture", Owner::stop).await
}

/// Keeps the newest frame as a screenshot and returns its index.
#[tauri::command]
pub(crate) async fn take_screenshot(app: AppHandle) -> Result<usize, String> {
    try_ask(&app, "take_screenshot", Owner::screenshot).await
}

/// A screenshot, at most `max_width` wide; no bytes for an unknown index.
#[tauri::command]
pub(crate) async fn screenshot(
    app: AppHandle,
    index: usize,
    max_width: u32,
) -> Result<Response, String> {
    ask(&app, "screenshot", move |owner| {
        owner.screenshot_pixels(index, max_width)
    })
    .await
    .map(pixels)
}

/// What the steps in `selection` select on the screenshot `screenshot`;
/// `None` when they leave nothing selected.
#[tauri::command]
pub(crate) async fn select(
    app: AppHandle,
    screenshot: usize,
    selection: SelectionSpec,
) -> Result<Option<SelectionView>, String> {
    try_ask(&app, "select", move |owner| {
        owner.select(screenshot, &selection)
    })
    .await
}

/// Sets up the check the live reading uses.
#[tauri::command]
pub(crate) async fn set_check(app: AppHandle, spec: CheckSpec) -> Result<(), String> {
    try_ask(&app, "set_check", move |owner| owner.set_check(&spec)).await
}

/// What the check reads on the newest frame, if a check and a frame exist.
#[tauri::command]
pub(crate) async fn read_value(app: AppHandle) -> Result<Option<Reading>, String> {
    try_ask(&app, "read_value", |owner| owner.reading()).await
}

/// How the check reads the newest frame, for the debug details.
#[tauri::command]
pub(crate) async fn explain_reading(app: AppHandle) -> Result<Option<ReadingDetails>, String> {
    try_ask(&app, "explain_reading", |owner| owner.explain_reading()).await
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
    try_ask(&app, "watch_value", move |owner| {
        owner.watch(&application, &value)
    })
    .await
}

/// What the watch's value reads now, and whether `when` holds for it.
#[tauri::command]
pub(crate) async fn watched_reading(
    app: AppHandle,
    watch: u64,
    when: Option<WhenDef>,
) -> Result<Live, String> {
    try_ask(&app, "watched_reading", move |owner| {
        owner.watched_reading(watch, when.as_ref())
    })
    .await
}

/// How the watch's value reads now, for the debug details.
#[tauri::command]
pub(crate) async fn explain_watched(
    app: AppHandle,
    watch: u64,
) -> Result<Option<ReadingDetails>, String> {
    try_ask(&app, "explain_watched", move |owner| {
        owner.explain_watched(watch)
    })
    .await
}

/// Stops the watch, unless another capture replaced it already.
#[tauri::command]
pub(crate) async fn stop_watching(app: AppHandle, watch: u64) -> Result<(), String> {
    ask(&app, "stop_watching", move |owner| {
        owner.stop_watching(watch);
    })
    .await
}

/// The running windows `window` takes, the one a new session would read
/// first.
#[tauri::command]
pub(crate) async fn match_windows(
    app: AppHandle,
    window: WindowDef,
) -> Result<Vec<WindowFit>, String> {
    try_ask(&app, "match_windows", move |owner| {
        owner.match_windows(&window)
    })
    .await
}

/// The window a new session would read for `window`, or `None` while none
/// fits.
#[tauri::command]
pub(crate) async fn read_window(app: AppHandle, window: WindowDef) -> Result<Option<u64>, String> {
    try_ask(&app, "read_window", move |owner| owner.read_window(&window)).await
}

/// Keeps `region` of the screenshot `screenshot` as a click picture, with
/// only what `selection` picks compared.
#[tauri::command]
pub(crate) async fn add_click_picture(
    app: AppHandle,
    screenshot: usize,
    region: RegionSpec,
    selection: Option<SelectionSpec>,
) -> Result<ClickPictureView, String> {
    try_ask(&app, "add_click_picture", move |owner| {
        owner.add_click_picture(screenshot, region, selection.as_ref())
    })
    .await
}

/// The click picture `image`, at most `max_width` wide.
#[tauri::command]
pub(crate) async fn click_picture(
    app: AppHandle,
    image: String,
    max_width: u32,
) -> Result<Response, String> {
    try_ask(&app, "click_picture", move |owner| {
        owner.click_picture_pixels(&image, max_width)
    })
    .await
    .map(|bytes| pixels(Some(bytes)))
}

/// Where a click at `at` lands on the picked window's newest frame.
#[tauri::command]
pub(crate) async fn find_click(app: AppHandle, at: ClickAtDef) -> Result<FoundClick, String> {
    try_ask(&app, "find_click", move |owner| owner.find_click(&at)).await
}

/// The saved applications and their values.
#[tauri::command]
pub(crate) async fn list_apps(app: AppHandle) -> Result<Vec<AppSummary>, String> {
    try_ask(&app, "list_apps", Owner::apps).await
}

/// Saves an application's name and window match over the application
/// `existing`, or as a new application; answers its ID.
#[tauri::command]
pub(crate) async fn save_app(
    app: AppHandle,
    existing: Option<String>,
    spec: AppSpec,
) -> Result<String, String> {
    try_ask(&app, "save_app", move |owner| {
        owner.save_app(existing.as_deref(), spec)
    })
    .await
}

/// Removes an application with its values, unless an automation watches one
/// of them, then closes its overlay's window. The removal is done once it
/// answers `Ok`, which holds why an overlay window could not open or close,
/// if one could not.
#[tauri::command]
pub(crate) async fn remove_app(
    app: AppHandle,
    application: String,
) -> Result<Option<String>, String> {
    try_ask(&app, "remove_app", move |owner| {
        owner.remove_app(&application)
    })
    .await
}

/// Saves a value to the application `spec` names.
#[tauri::command]
pub(crate) async fn save_value(app: AppHandle, spec: SaveSpec) -> Result<SavedValue, String> {
    try_ask(&app, "save_value", move |owner| owner.save(&spec)).await
}

/// Where the saved value `value` of the application `application` starts
/// on a window of `width` by `height` in the fix flow: its fix at that
/// size, or its first drawing scaled to it, or `None` when that scales past
/// the window.
#[tauri::command]
pub(crate) async fn place_value(
    app: AppHandle,
    application: String,
    value: String,
    width: u32,
    height: u32,
) -> Result<Option<RegionSpec>, String> {
    try_ask(&app, "place_value", move |owner| {
        owner.place_value(&application, &value, SizeDef { width, height })
    })
    .await
}

/// Saves a fix of a saved value at the size of the screenshot `spec` names,
/// replacing its fix at that size.
#[tauri::command]
pub(crate) async fn save_fix(app: AppHandle, spec: FixSpec) -> Result<SavedValue, String> {
    try_ask(&app, "save_fix", move |owner| owner.save_fix(&spec)).await
}

/// Removes the fix of a saved value at `width` by `height`.
#[tauri::command]
pub(crate) async fn remove_fix(
    app: AppHandle,
    application: String,
    value: String,
    width: u32,
    height: u32,
) -> Result<(), String> {
    try_ask(&app, "remove_fix", move |owner| {
        owner.remove_fix(&application, &value, SizeDef { width, height })
    })
    .await
}

/// Saves a reference, replacing the saved one it names.
#[tauri::command]
pub(crate) async fn save_reference(
    app: AppHandle,
    spec: ReferenceSaveSpec,
) -> Result<SavedReferenceId, String> {
    try_ask(&app, "save_reference", move |owner| {
        owner.save_reference(&spec)
    })
    .await
}

/// Removes a saved reference, unless a value reads with it.
#[tauri::command]
pub(crate) async fn delete_reference(
    app: AppHandle,
    application: String,
    reference: String,
) -> Result<(), String> {
    try_ask(&app, "delete_reference", move |owner| {
        owner.delete_reference(&application, &reference)
    })
    .await
}

/// Removes a saved value.
#[tauri::command]
pub(crate) async fn delete_value(
    app: AppHandle,
    application: String,
    value: String,
) -> Result<(), String> {
    try_ask(&app, "delete_value", move |owner| {
        owner.delete(&application, &value)
    })
    .await
}

/// Records the build an application runs now as the one its values were
/// checked against.
#[tauri::command]
pub(crate) async fn mark_checked(app: AppHandle, application: String) -> Result<(), String> {
    try_ask(&app, "mark_checked", move |owner| {
        owner.mark_checked(&application)
    })
    .await
}

/// Sets how an application protects itself from other programs.
#[tauri::command]
pub(crate) async fn set_anti_cheat(
    app: AppHandle,
    application: String,
    anti_cheat: AntiCheatDef,
) -> Result<(), String> {
    try_ask(&app, "set_anti_cheat", move |owner| {
        owner.set_anti_cheat(&application, anti_cheat)
    })
    .await
}

/// The saved automations, with what stops each one from running.
#[tauri::command]
pub(crate) async fn list_automations(app: AppHandle) -> Result<Vec<AutomationSummary>, String> {
    try_ask(&app, "list_automations", |owner| owner.list_automations()).await
}

/// Saves `automation` over `existing`, or as a new one; returns its ID.
#[tauri::command]
pub(crate) async fn save_automation(
    app: AppHandle,
    existing: Option<String>,
    automation: AutomationDef,
) -> Result<String, String> {
    try_ask(&app, "save_automation", move |owner| {
        owner.save_automation(existing.as_deref(), automation)
    })
    .await
}

/// Turns an automation on or off.
#[tauri::command]
pub(crate) async fn set_automation_enabled(
    app: AppHandle,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    try_ask(&app, "set_automation_enabled", move |owner| {
        owner.set_automation_enabled(&id, enabled)
    })
    .await
}

/// Removes an automation.
#[tauri::command]
pub(crate) async fn delete_automation(app: AppHandle, id: String) -> Result<(), String> {
    try_ask(&app, "delete_automation", move |owner| {
        owner.delete_automation(&id)
    })
    .await
}

/// Opens a window for each overlay that is on, and closes the rest.
#[tauri::command]
pub(crate) async fn sync_overlays(app: AppHandle) -> Result<(), String> {
    try_ask(&app, "sync_overlays", |owner| owner.sync_overlay_windows()).await
}

/// Turns `application`'s overlay on or off and sets where it shows, then
/// opens or closes its window to match. The setting is saved once it
/// answers `Ok`, which holds why an overlay window could not open or close,
/// if one could not.
#[tauri::command]
pub(crate) async fn set_overlay(
    app: AppHandle,
    application: String,
    enabled: bool,
    mode: OverlayModeDef,
) -> Result<Option<String>, String> {
    try_ask(&app, "set_overlay", move |owner| {
        owner.set_overlay(&application, enabled, mode)
    })
    .await
}

/// What `application`'s overlay shows now.
#[tauri::command]
pub(crate) async fn overlay_state(
    app: AppHandle,
    application: String,
) -> Result<OverlayView, String> {
    try_ask(&app, "overlay_state", move |owner| {
        owner.overlay_state(&application)
    })
    .await
}

/// What `application`'s values read now, for the Values page, whether its
/// overlay is on or not. The first call starts reading its window.
#[tauri::command]
pub(crate) async fn live_values(
    app: AppHandle,
    application: String,
) -> Result<OverlayView, String> {
    try_ask(&app, "live_values", move |owner| {
        owner.live_values(&application)
    })
    .await
}

/// Stops reading values for the Values page.
#[tauri::command]
pub(crate) async fn stop_live_values(app: AppHandle) -> Result<(), String> {
    ask(&app, "stop_live_values", |owner| {
        owner.stop_live_values(None);
    })
    .await
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
    ask(&app, "overlay_card", move |owner| {
        owner.overlay_card(&application, Card { width, height });
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
    try_ask(&app, "start_recording", move |owner| {
        owner.start_recording(region, ink, tolerance, screenshot, more)
    })
    .await
}

/// Stops the recording, keeping its readings for review.
#[tauri::command]
pub(crate) async fn stop_recording(app: AppHandle) -> Result<(), String> {
    ask(&app, "stop_recording", Owner::stop_recording).await
}

/// Forgets the recording and its readings.
#[tauri::command]
pub(crate) async fn discard_recording(app: AppHandle) -> Result<(), String> {
    ask(&app, "discard_recording", Owner::discard_recording).await
}

/// The recording and its readings so far; `None` without one.
#[tauri::command]
pub(crate) async fn recording_state(app: AppHandle) -> Result<Option<RecordingView>, String> {
    ask(&app, "recording_state", |owner| owner.recording_state()).await
}

/// A recorded reading, at most `max_width` wide; no bytes for an unknown
/// index.
#[tauri::command]
pub(crate) async fn recorded_reading(
    app: AppHandle,
    index: usize,
    max_width: u32,
) -> Result<Response, String> {
    ask(&app, "recorded_reading", move |owner| {
        owner.recorded_pixels(index, max_width)
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
    try_ask(&app, "review_problems", move |owner| {
        owner.review_problems(&spec)
    })
    .await
}

/// The saved settings, or the defaults and why the file cannot be read.
/// `setup.log` starts or stops as they say.
#[tauri::command]
pub(crate) async fn load_settings(app: AppHandle) -> Result<SettingsView, String> {
    ask(&app, "load_settings", Owner::settings).await
}

/// Saves every setting, or says what to fix. `setup.log` starts or stops
/// as the saved settings say.
#[tauri::command]
pub(crate) async fn save_settings(app: AppHandle, settings: SettingsFile) -> Result<(), String> {
    try_ask(&app, "save_settings", move |owner| {
        owner.save_settings(&settings)
    })
    .await
}

/// The folder the runtime writes its trace files to.
#[tauri::command]
pub(crate) async fn trace_folder(app: AppHandle) -> Result<String, String> {
    try_ask(&app, "trace_folder", |owner| owner.trace_folder()).await
}

/// Opens the trace folder in File Explorer.
#[tauri::command]
pub(crate) async fn open_trace_folder(app: AppHandle) -> Result<(), String> {
    try_ask(&app, "open_trace_folder", |owner| owner.open_trace_folder()).await
}

/// Whether the runtime starts when the user signs in to Windows.
#[tauri::command]
pub(crate) async fn starts_with_windows(app: AppHandle) -> Result<bool, String> {
    try_ask(&app, "starts_with_windows", |owner| {
        owner.starts_with_windows()
    })
    .await
}

/// Makes the runtime start when the user signs in to Windows, or not.
#[tauri::command]
pub(crate) async fn set_starts_with_windows(app: AppHandle, on: bool) -> Result<(), String> {
    try_ask(&app, "set_starts_with_windows", move |owner| {
        owner.set_starts_with_windows(on)
    })
    .await
}

/// Whether the runtime runs, and the newest trace file read so far.
#[tauri::command]
pub(crate) async fn debug_overview(app: AppHandle) -> Result<DebugOverview, String> {
    try_ask(&app, "debug_overview", Owner::debug_overview).await
}

/// The end of `log`, or nothing before anything was logged.
#[tauri::command]
pub(crate) async fn read_log(app: AppHandle, log: LogName) -> Result<String, String> {
    try_ask(&app, "read_log", move |owner| owner.read_log(log)).await
}

/// Opens the logs folder in File Explorer.
#[tauri::command]
pub(crate) async fn open_logs_folder(app: AppHandle) -> Result<(), String> {
    try_ask(&app, "open_logs_folder", |owner| owner.open_logs_folder()).await
}

/// The debug bundle, as the bytes of a zip file.
#[tauri::command]
pub(crate) async fn debug_bundle(app: AppHandle, contents: BundleSpec) -> Result<Response, String> {
    try_ask(&app, "debug_bundle", move |owner| {
        owner.debug_bundle(contents)
    })
    .await
    .map(Response::new)
}
