//! The overlays' windows: plain always-on-top windows of the setup app's
//! own, one for each application whose overlay is on.
//!
//! In overlay mode the window is transparent, lets clicks through, never
//! takes the focus and has no taskbar entry; it sits on the application's
//! window while that window is in front, and hides otherwise. In
//! picture-in-picture mode it is a small panel the user moves and resizes,
//! which stays on top and remembers its place. Neither hooks, injects into
//! or opens anything in the application's process.

use std::collections::BTreeSet;

use appytappy_config::app_file::{OverlayModeDef, PanelDef};
use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, Window, WindowEvent,
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

/// The label of `app`'s window for `mode`. Application IDs hold only
/// lowercase letters, digits and dashes, which labels allow.
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
    let monitors: Vec<Place> = handle
        .available_monitors()?
        .into_iter()
        .map(|monitor| Place {
            x: monitor.position().x,
            y: monitor.position().y,
            width: monitor.size().width,
            height: monitor.size().height,
            front: false,
        })
        .collect();
    for overlay in wanted {
        let label = label(&overlay.app, overlay.mode);
        if handle.get_webview_window(&label).is_none() {
            open(handle, &label, overlay, &monitors)?;
        }
    }
    Ok(())
}

/// Opens `overlay`'s window, labeled `label`. It needs a running app, so no
/// unit test can reach it.
#[mutants::skip]
fn open<R: Runtime>(
    handle: &AppHandle<R>,
    label: &str,
    overlay: &OverlayWindow,
    monitors: &[Place],
) -> tauri::Result<()> {
    let builder = WebviewWindowBuilder::new(handle, label, WebviewUrl::App("index.html".into()))
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .visible(false);
    match overlay.mode {
        OverlayModeDef::Overlay => {
            let window = builder
                .title(format!("{} overlay", overlay.name))
                .transparent(true)
                .decorations(false)
                .shadow(false)
                .resizable(false)
                .focusable(false)
                .build()?;
            // Clicks reach the application under it. It shows once its
            // first poll finds the application's window in front.
            window.set_ignore_cursor_events(true)?;
        },
        OverlayModeDef::PictureInPicture => {
            let window = builder
                .title(format!("{} values", overlay.name))
                .inner_size(PANEL_SIZE.0, PANEL_SIZE.1)
                .min_inner_size(PANEL_MIN.0, PANEL_MIN.1)
                .minimizable(false)
                .maximizable(false)
                .closable(false)
                .build()?;
            if let Some(panel) = overlay.panel.filter(|panel| on_a_monitor(*panel, monitors)) {
                window.set_position(PhysicalPosition::new(panel.x, panel.y))?;
                window.set_size(PhysicalSize::new(panel.width, panel.height))?;
            }
            window.show()?;
        },
    }
    Ok(())
}

/// Whether some of `panel` lies on one of `monitors`, so a panel left on a
/// monitor that was unplugged opens where the user can see it.
pub(crate) fn on_a_monitor(panel: PanelDef, monitors: &[Place]) -> bool {
    let edge = |start: i32, length: u32| i64::from(start).saturating_add(i64::from(length));
    monitors.iter().any(|monitor| {
        i64::from(panel.x) < edge(monitor.x, monitor.width)
            && edge(panel.x, panel.width) > i64::from(monitor.x)
            && i64::from(panel.y) < edge(monitor.y, monitor.height)
            && edge(panel.y, panel.height) > i64::from(monitor.y)
    })
}

/// Puts an overlay-mode `window` on its application's window while that
/// window is in front, and hides it otherwise. It needs a running app, so
/// no unit test can reach it.
///
/// # Errors
///
/// Returns Tauri's error when the window cannot be moved, shown or hidden.
#[mutants::skip]
pub(crate) fn follow<R: Runtime>(
    window: &WebviewWindow<R>,
    place: Option<Place>,
) -> tauri::Result<()> {
    let Some(place) = place.filter(|place| place.front) else {
        if window.is_visible()? {
            window.hide()?;
        }
        return Ok(());
    };
    let position = PhysicalPosition::new(place.x, place.y);
    let size = PhysicalSize::new(place.width, place.height);
    if window.outer_position()? != position {
        window.set_position(position)?;
    }
    if window.inner_size()? != size {
        window.set_size(size)?;
    }
    if !window.is_visible()? {
        window.show()?;
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
            if let Some((app, _mode)) = parse_label(label) {
                let (reply, _answer) = oneshot::channel();
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "a failed send means the owner stopped, and its captures with it"
                )]
                let _ = bridge.0.send(Message::Request(Request::CloseOverlay {
                    app: app.to_owned(),
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
    fn a_panel_opens_where_it_was_only_while_that_place_is_on_a_monitor() {
        let monitor = |x, y| Place {
            x,
            y,
            width: 1920,
            height: 1080,
            front: false,
        };
        let monitors = [monitor(0, 0), monitor(-1920, 0)];
        let panel = |x, y| PanelDef {
            x,
            y,
            width: 260,
            height: 200,
        };
        assert!(on_a_monitor(panel(100, 100), &monitors));
        assert!(on_a_monitor(panel(-1800, 900), &monitors));
        // Partly on the left monitor's right edge.
        assert!(on_a_monitor(panel(-261 + 2, 0), &monitors[1..]));
        // Just past each edge.
        assert!(!on_a_monitor(panel(1920, 0), &monitors));
        assert!(!on_a_monitor(panel(-2180, 0), &monitors));
        assert!(!on_a_monitor(panel(0, 1080), &monitors));
        assert!(!on_a_monitor(panel(0, -200), &monitors));
        assert!(!on_a_monitor(panel(100, 100), &[]));
    }
}
