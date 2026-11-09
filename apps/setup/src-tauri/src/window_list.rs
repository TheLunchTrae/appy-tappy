//! The windows the user can pick an application from.
//!
//! Only a running window can be captured, so the setup app lists windows
//! rather than asking for an executable file. Each choice names its process's
//! executable, which the setup app saves so the runtime finds the application later.

use std::collections::BTreeMap;

use appytappy_config::anti_cheat::{ProcessId, known_anti_cheat};
use appytappy_config::app_file::WindowDef;
use appytappy_config::window_match::WindowFacts;
use appytappy_sources::system::{ProcessEntry, WindowEntry};
use serde::Serialize;

/// Window classes that belong to the desktop itself, not to an app.
const DESKTOP_CLASSES: [&str; 3] = ["Progman", "WorkerW", "Shell_TrayWnd"];

/// A window the user can pick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct WindowChoice {
    /// The window's handle, which the capture takes.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) handle: u64,
    /// The process that owns it, whose executable's build the setup app
    /// reads. The UI does not need it.
    #[serde(skip)]
    pub(crate) process: ProcessId,
    /// The window's title.
    pub(crate) title: String,
    /// The window's class name, which a window match can use.
    pub(crate) class: String,
    /// The file name of the window's executable, such as `Example.exe`.
    pub(crate) executable: String,
    /// The window's width on screen, in pixels.
    pub(crate) width: u32,
    /// The window's height on screen, in pixels.
    pub(crate) height: u32,
}

/// The windows to offer, with the anti-cheat products running now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct WindowList {
    /// The windows the user can pick, sorted by title.
    pub(crate) windows: Vec<WindowChoice>,
    /// The anti-cheat products whose processes run now, sorted and without
    /// repeats. A running product may protect any application, so the list is a
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
                process: window.process,
                title: window.title,
                class: window.class,
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

/// A running window that an application's window match takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct WindowFit {
    /// The window's handle.
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) handle: u64,
    /// Whether it is taken only because no window qualifies.
    pub(crate) fallback: bool,
    /// Whether it is the window a new session would read, so a value has
    /// to be drawn on it.
    pub(crate) best: bool,
}

/// The `windows` that `window` takes: `read`, the window a new session
/// would read, first, then the rest by fit.
#[must_use]
pub(crate) fn matching(
    windows: &[WindowChoice],
    window: &WindowDef,
    read: Option<u64>,
) -> Vec<WindowFit> {
    let matcher = window.to_match();
    let facts = windows.iter().map(|choice| WindowFacts {
        title: &choice.title,
        class: &choice.class,
        executable: &choice.executable,
    });
    let mut fits: Vec<WindowFit> = matcher
        .rank(facts)
        .into_iter()
        .filter_map(|(position, fit)| {
            windows.get(position).map(|choice| WindowFit {
                handle: choice.handle,
                fallback: fit.is_fallback(),
                best: Some(choice.handle) == read,
            })
        })
        .collect();
    fits.sort_by_key(|fit| !fit.best);
    fits
}

#[cfg(test)]
mod tests {
    use appytappy_config::app_file::FallbackDef;
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
            class: "AppWindow".to_owned(),
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
            process(1, "Example.exe"),
            process(2, "Player.exe"),
            process(3, "explorer.exe"),
            process(9, "appytappy-setup.exe"),
        ];
        let mut hidden = window(5, 2, "Hidden");
        hidden.visible = false;
        let mut desktop = window(6, 3, "Program Manager");
        desktop.class = "Progman".to_owned();
        let mut collapsed = window(7, 2, "Collapsed");
        collapsed.bounds.right = collapsed.bounds.left;
        let mut flat = window(12, 2, "Flat");
        flat.bounds.bottom = flat.bounds.top;
        let windows = vec![
            window(4, 2, "media Player"),
            window(3, 1, "Example App"),
            hidden,
            desktop,
            collapsed,
            flat,
            window(8, 2, "   "),
            window(10, 9, "AppyTappy Setup"),
            window(11, 42, "Process already gone"),
        ];

        let list = list(windows, &processes, 9);

        assert_eq!(
            list.windows,
            [
                WindowChoice {
                    handle: 3,
                    process: ProcessId::new(1),
                    title: "Example App".to_owned(),
                    class: "AppWindow".to_owned(),
                    executable: "Example.exe".to_owned(),
                    width: 800,
                    height: 600,
                },
                WindowChoice {
                    handle: 4,
                    process: ProcessId::new(2),
                    title: "media Player".to_owned(),
                    class: "AppWindow".to_owned(),
                    executable: "Player.exe".to_owned(),
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

    #[test]
    fn matching_ranks_the_offered_windows_by_fit() {
        let processes = [process(1, "Example.exe"), process(2, "Player.exe")];
        let mut settings = window(5, 1, "Settings");
        settings.class = "Dialog".to_owned();
        let windows = vec![
            window(4, 2, "Media Player"),
            settings,
            window(3, 1, "Example App"),
        ];
        let offered = list(windows.clone(), &processes, 0).windows;
        let exact = WindowDef::exact(WindowFacts {
            title: "Example App",
            class: "AppWindow",
            executable: "example.exe",
        });
        let fit = |handle, fallback, best| WindowFit {
            handle,
            fallback,
            best,
        };
        assert_eq!(
            matching(&offered, &exact, Some(3)),
            [fit(3, false, true), fit(5, false, false)]
        );
        assert_eq!(
            matching(&offered, &exact, None),
            [fit(3, false, false), fit(5, false, false)],
            "the runtime reads a window that is not offered"
        );
        // A second instance fits as well, and the runtime reads one of them.
        let mut twins = windows;
        twins.push(window(8, 1, "Example App"));
        assert_eq!(
            matching(&list(twins, &processes, 0).windows, &exact, Some(8)),
            [
                fit(8, false, true),
                fit(3, false, false),
                fit(5, false, false)
            ]
        );
        let mut title_only = exact.clone();
        title_only.must_match.title = true;
        title_only.fallback = FallbackDef::Executable;
        assert_eq!(
            matching(&offered, &title_only, Some(3)),
            [fit(3, false, true)]
        );
        title_only.title = "Missing".to_owned();
        assert_eq!(
            matching(&offered, &title_only, Some(3)),
            [fit(3, true, true), fit(5, true, false)]
        );
        let nothing = WindowDef {
            executable: "Other.exe".to_owned(),
            ..exact
        };
        assert_eq!(matching(&offered, &nothing, None), []);
    }
}
