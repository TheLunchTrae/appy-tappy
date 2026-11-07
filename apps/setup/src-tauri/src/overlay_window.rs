//! The overlays' windows: plain always-on-top windows of the setup app's
//! own, one for each application whose overlay is on.
//!
//! In overlay mode the window is transparent, lets clicks through, never
//! takes the focus and has no taskbar entry. It is only as big as its card,
//! and a thread of its own puts it in the corner of the application's window
//! while that window is in front, and off the screen otherwise, so it is
//! shown once and only moved after that. In picture-in-picture
//! mode it is a small panel the user moves and resizes, which stays on top
//! and remembers its place. Neither hooks, injects into or opens anything
//! in the application's process.

use std::collections::BTreeSet;
use std::sync::mpsc::Sender;
use std::time::Duration;

use appytappy_config::app_file::{OverlayModeDef, PanelDef};
use tauri::{
    AppHandle, LogicalPosition, LogicalSize, Manager, PhysicalPosition, PhysicalSize, Runtime,
    WebviewUrl, WebviewWindow, WebviewWindowBuilder, Window, WindowEvent,
};
use tokio::sync::oneshot;

use crate::commands::Bridge;
use crate::overlay::{OverlayWindow, Place};
use crate::owner::{Message, Request};

/// The label of the setup app's main window.
const MAIN: &str = "main";

/// The start of an overlay window's label, before the application's ID.
const OVERLAY_PREFIX: &str = "overlay-";

/// The start of a picture-in-picture panel's label, before the
/// application's ID.
const PANEL_PREFIX: &str = "panel-";

/// A new panel's size in logical pixels, before the user resizes it.
const PANEL_SIZE: (f64, f64) = (260.0, 200.0);

/// The smallest a panel gets, in logical pixels.
const PANEL_MIN: (f64, f64) = (160.0, 90.0);

/// How much of a saved panel, in physical pixels each way, has to lie in a
/// monitor's work area for it to open there: enough to grab its title bar.
const PANEL_REACH: i64 = 48;

/// The size an overlay window has before its card is measured, in logical
/// pixels: room for the card to take its own size.
const OVERLAY_START: (f64, f64) = (400.0, 400.0);

/// How far the card sits from the application window's corner, in logical
/// pixels, past the border Windows leaves around a window.
const CARD_INSET: f64 = 16.0;

/// How often an overlay window looks for where its application's window is:
/// often enough to keep up with a dragged window.
const FOLLOW_INTERVAL: Duration = Duration::from_millis(50);

/// A rectangle in physical screen pixels, such as a monitor's work area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Area {
    /// The left edge.
    pub(crate) x: i32,
    /// The top edge.
    pub(crate) y: i32,
    /// The width.
    pub(crate) width: u32,
    /// The height.
    pub(crate) height: u32,
}

/// The label of `app`'s window for `mode`. Application IDs hold only
/// lowercase letters, digits, dashes and underscores, which labels allow.
pub(crate) fn label(app: &str, mode: OverlayModeDef) -> String {
    match mode {
        OverlayModeDef::Overlay => format!("{OVERLAY_PREFIX}{app}"),
        OverlayModeDef::PictureInPicture => format!("{PANEL_PREFIX}{app}"),
    }
}

/// The application ID and mode of an overlay window's `label`, or `None`
/// for another window.
pub(crate) fn parse_label(label: &str) -> Option<(&str, OverlayModeDef)> {
    label
        .strip_prefix(OVERLAY_PREFIX)
        .map(|app| (app, OverlayModeDef::Overlay))
        .or_else(|| {
            label
                .strip_prefix(PANEL_PREFIX)
                .map(|app| (app, OverlayModeDef::PictureInPicture))
        })
        .filter(|(app, _mode)| !app.is_empty())
}

/// The labels in `open` of overlay windows that nothing in `wanted` wants,
/// such as one whose overlay was turned off or changed mode.
pub(crate) fn unwanted<'a>(
    open: impl IntoIterator<Item = &'a str>,
    wanted: &[OverlayWindow],
) -> Vec<&'a str> {
    let labels: BTreeSet<String> = wanted
        .iter()
        .map(|overlay| label(&overlay.app, overlay.mode))
        .collect();
    open.into_iter()
        .filter(|open| parse_label(open).is_some() && !labels.contains(*open))
        .collect()
}

/// Opens a window for each overlay in `wanted` that has none, and closes
/// the overlay windows nothing wants any more.
///
/// It needs a running app, so no unit test can reach it; the choice of
/// windows to close is [`unwanted`]'s.
///
/// # Errors
///
/// Returns Tauri's error when a window cannot be opened or closed.
#[mutants::skip]
pub(crate) fn sync<R: Runtime>(
    handle: &AppHandle<R>,
    wanted: &[OverlayWindow],
) -> tauri::Result<()> {
    let windows = handle.webview_windows();
    for label in unwanted(windows.keys().map(String::as_str), wanted) {
        if let Some(window) = windows.get(label) {
            window.destroy()?;
        }
    }
    let areas: Vec<Area> = handle
        .available_monitors()?
        .iter()
        .map(|monitor| {
            let area = monitor.work_area();
            Area {
                x: area.position.x,
                y: area.position.y,
                width: area.size.width,
                height: area.size.height,
            }
        })
        .collect();
    for overlay in wanted {
        let label = label(&overlay.app, overlay.mode);
        if handle.get_webview_window(&label).is_none() {
            open(handle, &label, overlay, &areas)?;
        }
    }
    Ok(())
}

/// Opens `overlay`'s window, labeled `label`. It needs a running app, so no
/// unit test can reach it.
///
/// Both kinds are built so that showing them leaves the focus where it is:
/// the window library activates a window that it shows, or that it changes
/// once it shows, unless it was built without the focus and shown with it.
#[mutants::skip]
fn open<R: Runtime>(
    handle: &AppHandle<R>,
    label: &str,
    overlay: &OverlayWindow,
    areas: &[Area],
) -> tauri::Result<()> {
    // Where a panel opens, in logical pixels each way, before it goes back to
    // its saved place: off every screen, where Windows also puts minimized
    // windows.
    const PANEL_OFF_SCREEN: f64 = -32_000.0;
    let builder = WebviewWindowBuilder::new(handle, label, WebviewUrl::App("index.html".into()))
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false);
    let panel = overlay.panel.filter(|panel| in_reach(*panel, areas));
    let built = match overlay.mode {
        // Hidden until it lets clicks through, since that change would show
        // it with the focus.
        OverlayModeDef::Overlay => builder
            .title(format!("{} overlay", overlay.name))
            .transparent(true)
            .decorations(false)
            .shadow(false)
            .resizable(false)
            .focusable(false)
            .inner_size(OVERLAY_START.0, OVERLAY_START.1)
            .visible(false)
            .build(),
        // Shown as it is built, off the screen when it has a place to go
        // back to, since moving it there afterwards keeps the focus too.
        OverlayModeDef::PictureInPicture => {
            let builder = builder
                .title(format!("{} values", overlay.name))
                .inner_size(PANEL_SIZE.0, PANEL_SIZE.1)
                .min_inner_size(PANEL_MIN.0, PANEL_MIN.1)
                .minimizable(false)
                .maximizable(false)
                .closable(false)
                .visible(true);
            match panel {
                Some(_) => builder.position(PANEL_OFF_SCREEN, PANEL_OFF_SCREEN),
                None => builder.center(),
            }
            .build()
        },
    };
    let window = match built {
        Ok(window) => window,
        // Two syncs can race to open the same window, such as one from the
        // main window opening and one from a changed setting; the window the
        // other opened is the one wanted.
        Err(_) if handle.get_webview_window(label).is_some() => return Ok(()),
        Err(error) => return Err(error),
    };
    let set_up = match (overlay.mode, panel) {
        (OverlayModeDef::Overlay, _) => set_up_overlay(handle, &window, &overlay.app),
        (OverlayModeDef::PictureInPicture, Some(panel)) => window
            .set_position(PhysicalPosition::new(panel.x, panel.y))
            .and_then(|()| window.set_size(PhysicalSize::new(panel.width, panel.height))),
        (OverlayModeDef::PictureInPicture, None) => Ok(()),
    };
    // A window left half set up could sit over the screen without letting
    // clicks through, so it goes, and the next sync tries again.
    match set_up {
        Ok(()) => Ok(()),
        Err(error) => window.destroy().and(Err(error)),
    }
}

/// Moves an overlay-mode `window` off the screen, lets clicks through it,
/// shows it without the focus and starts the thread that moves it from then
/// on. It needs a running app, so no unit test can reach it.
#[mutants::skip]
fn set_up_overlay<R: Runtime>(
    handle: &AppHandle<R>,
    window: &WebviewWindow<R>,
    app: &str,
) -> tauri::Result<()> {
    follow(window, None)?;
    // Clicks reach the application under it.
    window.set_ignore_cursor_events(true)?;
    show_without_focus(window)?;
    let messages = handle.state::<Bridge>().0.clone();
    let follower = window.clone();
    let app = app.to_owned();
    std::thread::Builder::new()
        .name(format!("overlay {app}"))
        .spawn(move || follow_while_open(&follower, &app, &messages))?;
    Ok(())
}

/// Shows `window` without taking the focus. Windows is the only platform
/// the setup app runs its overlays on; elsewhere it shows `window` as the
/// window library does.
///
/// Asking for the window's handle waits for the window library to apply the
/// changes asked of it before, so the window is shown with them.
#[cfg(windows)]
#[mutants::skip]
fn show_without_focus<R: Runtime>(window: &WebviewWindow<R>) -> tauri::Result<()> {
    let handle = window.hwnd()?.0.addr();
    appytappy_platform::show_without_focus(handle).map_err(tauri::Error::Io)
}

/// Shows `window` as the window library does, on a platform where the setup
/// app runs no overlays.
#[cfg(not(windows))]
#[mutants::skip]
fn show_without_focus<R: Runtime>(window: &WebviewWindow<R>) -> tauri::Result<()> {
    window.show()
}

/// Whether enough of `panel` lies in one of the monitors' work `areas` for
/// the user to grab it, so a panel left on a monitor that was unplugged, or
/// under the taskbar, opens where the user can reach it.
pub(crate) fn in_reach(panel: PanelDef, areas: &[Area]) -> bool {
    let overlap = |start: i32, length: u32, area_start: i32, area_length: u32| {
        let end = i64::from(start).saturating_add(i64::from(length));
        let area_end = i64::from(area_start).saturating_add(i64::from(area_length));
        end.min(area_end)
            .saturating_sub(i64::from(start).max(i64::from(area_start)))
    };
    areas.iter().any(|area| {
        overlap(panel.x, panel.width, area.x, area.width) >= PANEL_REACH
            && overlap(panel.y, panel.height, area.y, area.height) >= PANEL_REACH
    })
}

/// Asks the owner where `app`'s overlay goes and puts `window` there, until
/// the window or the owner is gone. It runs on the window's own thread, so
/// the window keeps up with the application's even while its page is
/// throttled.
#[mutants::skip]
fn follow_while_open<R: Runtime>(window: &WebviewWindow<R>, app: &str, messages: &Sender<Message>) {
    loop {
        if window
            .app_handle()
            .get_webview_window(window.label())
            .is_none()
        {
            return;
        }
        let (reply, answer) = oneshot::channel();
        let request = Request::OverlayPlace {
            app: app.to_owned(),
            reply,
        };
        if messages.send(Message::Request(request)).is_err() {
            return;
        }
        let Ok(place) = answer.blocking_recv() else {
            return;
        };
        if follow(window, place).is_err() {
            return;
        }
        std::thread::sleep(FOLLOW_INTERVAL);
    }
}

/// Puts an overlay-mode `window` in the corner of its application's window,
/// at its card's size, or off the screen with `None`. It needs a running
/// app, so no unit test can reach it.
///
/// # Errors
///
/// Returns Tauri's error when the window cannot be moved or resized.
#[mutants::skip]
fn follow<R: Runtime>(window: &WebviewWindow<R>, place: Option<Place>) -> tauri::Result<()> {
    // Off every screen, where Windows also puts minimized windows.
    const OFF_SCREEN: PhysicalPosition<i32> = PhysicalPosition::new(-32_000, -32_000);
    let Some(place) = place else {
        if window.outer_position()? != OFF_SCREEN {
            window.set_position(OFF_SCREEN)?;
        }
        return Ok(());
    };
    // The card takes the scale of the monitor the application's window is
    // on. Its corner can lie outside every monitor, as a maximized window's
    // border does, so its middle is the point that picks the monitor.
    let middle_x = f64::from(place.x) + f64::from(place.width) / 2.0;
    let middle_y = f64::from(place.y) + f64::from(place.height) / 2.0;
    let scale = match window.monitor_from_point(middle_x, middle_y)? {
        Some(monitor) => monitor.scale_factor(),
        None => window.scale_factor()?,
    };
    let card: PhysicalSize<u32> =
        LogicalSize::new(place.card.width.ceil(), place.card.height.ceil()).to_physical(scale);
    let inset: PhysicalPosition<i32> =
        LogicalPosition::new(CARD_INSET, CARD_INSET).to_physical(scale);
    let size = PhysicalSize::new(card.width.min(place.width), card.height.min(place.height));
    let position = PhysicalPosition::new(
        place.x.saturating_add(inset.x),
        place.y.saturating_add(inset.y),
    );
    // Moved first, since Windows rescales a window that moves onto a
    // monitor of another scale, which would undo a new size.
    if window.outer_position()? != position {
        window.set_position(position)?;
    }
    if window.inner_size()? != size {
        window.set_size(size)?;
    }
    Ok(())
}

/// Reacts to a window's `event`: a moved or resized panel's place is kept,
/// a closed overlay window stops its capture, and closing the main window
/// closes the overlays and ends the app.
#[mutants::skip]
pub(crate) fn on_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    let label = window.label();
    let bridge = window.state::<Bridge>();
    match event {
        WindowEvent::Destroyed if label == MAIN => window.app_handle().exit(0),
        WindowEvent::Destroyed => {
            if let Some((app, mode)) = parse_label(label) {
                let (reply, _answer) = oneshot::channel();
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "a failed send means the owner stopped, and its captures with it"
                )]
                let _ = bridge.0.send(Message::Request(Request::CloseOverlay {
                    app: app.to_owned(),
                    mode,
                    reply,
                }));
            }
        },
        WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
            let Some((app, OverlayModeDef::PictureInPicture)) = parse_label(label) else {
                return;
            };
            let (Ok(position), Ok(size)) = (window.outer_position(), window.inner_size()) else {
                return;
            };
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a failed send means the owner stopped; the panel opens where it was saved"
            )]
            let _ = bridge.0.send(Message::PanelPlaced {
                app: app.to_owned(),
                panel: PanelDef {
                    x: position.x,
                    y: position.y,
                    width: size.width,
                    height: size.height,
                },
            });
        },
        _ => {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_name_the_application_and_mode_and_nothing_else_parses() {
        for mode in [OverlayModeDef::Overlay, OverlayModeDef::PictureInPicture] {
            let label = label("example-2", mode);
            assert_eq!(parse_label(&label), Some(("example-2", mode)));
        }
        assert_eq!(label("example", OverlayModeDef::Overlay), "overlay-example");
        assert_eq!(
            label("example", OverlayModeDef::PictureInPicture),
            "panel-example"
        );
        for other in ["main", "overlay-", "panel-", "overlays"] {
            assert_eq!(parse_label(other), None, "{other}");
        }
    }

    #[test]
    fn only_overlay_windows_nothing_wants_are_closed() {
        let wanted = [OverlayWindow {
            app: "example".to_owned(),
            name: "Example App".to_owned(),
            mode: OverlayModeDef::Overlay,
            panel: None,
        }];
        let open = [
            "main",
            "overlay-example",
            "panel-example",
            "overlay-other",
            "overlay-",
        ];
        assert_eq!(unwanted(open, &wanted), ["panel-example", "overlay-other"]);
        assert_eq!(
            unwanted(open, &[]),
            ["overlay-example", "panel-example", "overlay-other"]
        );
    }

    #[test]
    fn a_panel_opens_where_it_was_only_while_the_user_can_reach_it() {
        let area = |x, y| Area {
            x,
            y,
            width: 1920,
            height: 1040,
        };
        let areas = [area(0, 0), area(-1920, 0)];
        let panel = |x, y| PanelDef {
            x,
            y,
            width: 260,
            height: 200,
        };
        assert!(in_reach(panel(100, 100), &areas));
        assert!(in_reach(panel(-1800, 800), &areas));
        // Just enough of it on the right monitor's left edge, and just too
        // little.
        assert!(in_reach(panel(-212, 0), &areas[..1]));
        assert!(!in_reach(panel(-213, 0), &areas[..1]));
        assert!(in_reach(panel(1872, 0), &areas));
        assert!(!in_reach(panel(1873, 0), &areas));
        // Under the taskbar, below the work area.
        assert!(in_reach(panel(0, 992), &areas));
        assert!(!in_reach(panel(0, 993), &areas));
        assert!(!in_reach(panel(0, -153), &areas));
        assert!(!in_reach(panel(100, 100), &[]));
    }
}
