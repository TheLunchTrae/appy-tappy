//! The commands the web UI calls.
//!
//! Each one sends a request to the owner thread and awaits its answer, so a
//! slow answer never freezes the window. The commands take the app's handle
//! and look the bridge up on it, rather than borrowing it as an argument,
//! because Tauri checks a borrowing async command with code the lint wall
//! refuses. Pixels go back as raw bytes rather
//! than JSON, which would be several times larger.

use std::sync::mpsc::Sender;

use tauri::ipc::Response;
use tauri::{AppHandle, Manager};
use tokio::sync::oneshot;

use crate::check_spec::CheckSpec;
use crate::owner::{Message, Reading, Reply, Request};
use crate::window_list::WindowList;

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
