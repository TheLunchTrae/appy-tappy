//! The windows the user can pick a game from.
//!
//! Only a running window can be captured, so the setup app lists windows
//! rather than asking for an executable file. Each choice names its process's
//! executable, which the setup app saves so the runtime finds the game later.

use std::collections::BTreeMap;

use appytappy_config::anti_cheat::known_anti_cheat;
use appytappy_sources::system::{ProcessEntry, WindowEntry};
use serde::Serialize;

/// Window classes that belong to the desktop itself, not to an app.
const DESKTOP_CLASSES: [&str; 3] = ["Progman", "WorkerW", "Shell_TrayWnd"];

/// A window the user can pick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WindowChoice {
    /// The window's handle, which the capture takes.
    pub(crate) handle: u64,
    /// The window's title.
    pub(crate) title: String,
    /// The file name of the window's executable, such as `GeometryDash.exe`.
    pub(crate) executable: String,
    /// The window's width on screen, in pixels.
    pub(crate) width: u32,
    /// The window's height on screen, in pixels.
    pub(crate) height: u32,
}

/// The windows to offer, with the anti-cheat products running now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WindowList {
    /// The windows the user can pick, sorted by title.
    pub(crate) windows: Vec<WindowChoice>,
    /// The anti-cheat products whose processes run now, sorted and without
    /// repeats. A running product may protect any game, so the list is a
    /// warning, not a classification.
    pub(crate) anti_cheat: Vec<&'static str>,
}

/// Picks the windows worth offering from every top-level window: visible,
/// titled, on screen, and not the desktop's or the setup app's own.
#[must_use]
pub(crate) fn list(
    windows: Vec<WindowEntry>,
    processes: &[ProcessEntry],
    own_process: u32,
) -> WindowList {
    let executables: BTreeMap<u32, &str> = processes
        .iter()
        .map(|process| (process.id.raw(), process.executable.as_str()))
        .collect();
    let mut choices: Vec<WindowChoice> = windows
        .into_iter()
        .filter(|window| {
            window.visible
                && window.process.raw() != own_process
                && !window.title.trim().is_empty()
                && !DESKTOP_CLASSES.contains(&window.class.as_str())
        })
        .filter_map(|window| {
            let width = u32::try_from(window.bounds.right.checked_sub(window.bounds.left)?).ok()?;
            let height =
                u32::try_from(window.bounds.bottom.checked_sub(window.bounds.top)?).ok()?;
            let executable = executables.get(&window.process.raw())?;
            (width > 0 && height > 0).then(|| WindowChoice {
                handle: window.handle,
                title: window.title,
                executable: (*executable).to_owned(),
                width,
                height,
            })
        })
        .collect();
    choices.sort_by_cached_key(|choice| (choice.title.to_lowercase(), choice.handle));
    let mut anti_cheat: Vec<&'static str> = processes
        .iter()
        .filter_map(|process| known_anti_cheat(&process.executable))
        .map(|known| known.product)
        .collect();
    anti_cheat.sort_unstable();
    anti_cheat.dedup();
    WindowList {
        windows: choices,
        anti_cheat,
    }
}

#[cfg(test)]
mod tests {
    use appytappy_config::anti_cheat::ProcessId;
    use appytappy_sources::system::Bounds;

    use super::*;

    fn process(id: u32, executable: &str) -> ProcessEntry {
        ProcessEntry {
            id: ProcessId::new(id),
            parent: ProcessId::new(0),
            executable: executable.to_owned(),
            threads: 1,
        }
    }

    fn window(handle: u64, process: u32, title: &str) -> WindowEntry {
        WindowEntry {
            handle,
            process: ProcessId::new(process),
            title: title.to_owned(),
            class: "GameWindow".to_owned(),
            visible: true,
            bounds: Bounds {
                left: 10,
                top: 20,
                right: 810,
                bottom: 620,
            },
        }
    }

    #[test]
    fn offers_visible_titled_app_windows_sorted_by_title() {
        let processes = [
            process(1, "GeometryDash.exe"),
            process(2, "Spotify.exe"),
            process(3, "explorer.exe"),
            process(9, "appytappy-setup.exe"),
        ];
        let mut hidden = window(5, 2, "Hidden");
        hidden.visible = false;
        let mut desktop = window(6, 3, "Program Manager");
        desktop.class = "Progman".to_owned();
        let mut collapsed = window(7, 2, "Collapsed");
        collapsed.bounds.right = collapsed.bounds.left;
        let windows = vec![
            window(4, 2, "spotify Premium"),
            window(3, 1, "Geometry Dash"),
            hidden,
            desktop,
            collapsed,
            window(8, 2, "   "),
            window(10, 9, "AppyTappy setup"),
            window(11, 42, "Process already gone"),
        ];

        let list = list(windows, &processes, 9);

        assert_eq!(
            list.windows,
            [
                WindowChoice {
                    handle: 3,
                    title: "Geometry Dash".to_owned(),
                    executable: "GeometryDash.exe".to_owned(),
                    width: 800,
                    height: 600,
                },
                WindowChoice {
                    handle: 4,
                    title: "spotify Premium".to_owned(),
                    executable: "Spotify.exe".to_owned(),
                    width: 800,
                    height: 600,
                },
            ]
        );
        assert_eq!(list.anti_cheat, [""; 0]);
    }

    #[test]
    fn names_each_running_anti_cheat_product_once() {
        let processes = [
            process(1, "BEService.exe"),
            process(2, "vgc.exe"),
            process(3, "vgtray.exe"),
        ];
        let list = list(Vec::new(), &processes, 0);
        assert_eq!(list.anti_cheat, ["BattlEye", "Riot Vanguard"]);
    }
}
