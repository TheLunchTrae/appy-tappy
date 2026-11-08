//! The operating system services the guided scan needs.
//!
//! They are process and window discovery that never opens a process, the
//! user's answer before memory access, opening a process under a permit, and
//! screen pixels.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::io;

use appytappy_config::anti_cheat::{ProcessAccessPermit, ProcessId};
use appytappy_config::app_file::BuildDef;
use appytappy_config::window_match::{WindowFacts, WindowMatch};

use crate::memory::ProcessMemory;

/// A running process, as a system-wide snapshot reports it without opening
/// the process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessEntry {
    /// The process ID.
    pub id: ProcessId,
    /// The ID of the process that started it.
    pub parent: ProcessId,
    /// The executable's file name, such as `Example.exe`.
    pub executable: String,
    /// How many threads it runs.
    pub threads: u32,
}

/// A top-level window, as window enumeration reports it without opening the
/// owning process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowEntry {
    /// The window handle as a number.
    pub handle: u64,
    /// The process that owns the window.
    pub process: ProcessId,
    /// The window's title.
    pub title: String,
    /// The window's class name.
    pub class: String,
    /// Whether the window is visible.
    pub visible: bool,
    /// The window's bounds in screen pixels.
    pub bounds: Bounds,
}

/// Where a window is on the screen, and whether the user can see it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowPlace {
    /// The window's bounds in screen pixels.
    pub bounds: Bounds,
    /// Whether it is visible and not minimized.
    pub shown: bool,
    /// Whether it is the window in front, the one with the keyboard focus.
    pub front: bool,
}

/// A rectangle in screen pixels; `right` and `bottom` are exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    /// The left edge.
    pub left: i32,
    /// The top edge.
    pub top: i32,
    /// The right edge.
    pub right: i32,
    /// The bottom edge.
    pub bottom: i32,
}

/// A pixel's color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    /// Red.
    pub red: u8,
    /// Green.
    pub green: u8,
    /// Blue.
    pub blue: u8,
}

/// The user's answer to a request to read a process's memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessAnswer {
    /// The user confirmed that the app has no anti-cheat.
    Allowed,
    /// The user refused, or closed the prompt.
    Refused,
}

/// The operating system services the guided scan uses.
pub trait System {
    /// An opened process.
    type Process: ProcessMemory;

    /// Lists running processes without opening any of them.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn processes(&self) -> io::Result<Vec<ProcessEntry>>;

    /// Lists top-level windows without opening their processes.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn windows(&self) -> io::Result<Vec<WindowEntry>>;

    /// Where `window` is and whether it shows, or `None` once it closed.
    /// It asks only about that window, so it is cheap enough to call many
    /// times a second.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn window_place(&self, window: u64) -> io::Result<Option<WindowPlace>>;

    /// The build of `process`'s executable, read from the file without
    /// opening the process: see [`crate::build`].
    ///
    /// # Errors
    ///
    /// Returns the operating system's error, such as for a process that
    /// exited or a file it cannot read.
    fn executable_build(&self, process: ProcessId) -> io::Result<BuildDef>;

    /// Asks the user, outside the agent's reach, whether `process` may be
    /// read. `anti_cheat` names anti-cheat products that are running.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error when the prompt cannot be shown.
    fn ask_memory_access(
        &self,
        process: &ProcessEntry,
        anti_cheat: &[&str],
    ) -> io::Result<AccessAnswer>;

    /// Opens the permitted process for reading only.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn open(&self, permit: &ProcessAccessPermit) -> io::Result<Self::Process>;

    /// Reads one pixel of the screen.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn screen_pixel(&self, x: i32, y: i32) -> io::Result<Rgb>;
}

/// The system of a platform AppyTappy does not support: every call fails
/// with [`io::ErrorKind::Unsupported`].
#[derive(Debug, Default)]
pub struct Unsupported;

/// The process type of [`Unsupported`], which cannot be constructed.
#[derive(Debug)]
pub struct NoProcess(Infallible);

fn unsupported<T>() -> io::Result<T> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "AppyTappy reads application state on Windows only",
    ))
}

// `NoProcess` holds `Infallible`, so no value exists and these bodies never
// run. No test can call them, so mutating them cannot be caught.
impl ProcessMemory for NoProcess {
    #[mutants::skip]
    fn regions(&self) -> io::Result<Vec<crate::memory::Region>> {
        match self.0 {}
    }

    #[mutants::skip]
    fn read(&self, _address: u64, _buffer: &mut [u8]) -> io::Result<()> {
        match self.0 {}
    }

    #[mutants::skip]
    fn modules(&self) -> io::Result<Vec<crate::memory::Module>> {
        match self.0 {}
    }

    #[mutants::skip]
    fn pointer_width(&self) -> crate::memory::PointerWidth {
        match self.0 {}
    }

    #[mutants::skip]
    fn image_name(&self) -> io::Result<String> {
        match self.0 {}
    }
}

impl System for Unsupported {
    type Process = NoProcess;

    fn processes(&self) -> io::Result<Vec<ProcessEntry>> {
        unsupported()
    }

    fn windows(&self) -> io::Result<Vec<WindowEntry>> {
        unsupported()
    }

    fn window_place(&self, _window: u64) -> io::Result<Option<WindowPlace>> {
        unsupported()
    }

    fn executable_build(&self, _process: ProcessId) -> io::Result<BuildDef> {
        unsupported()
    }

    fn ask_memory_access(
        &self,
        _process: &ProcessEntry,
        _anti_cheat: &[&str],
    ) -> io::Result<AccessAnswer> {
        unsupported()
    }

    fn open(&self, _permit: &ProcessAccessPermit) -> io::Result<NoProcess> {
        unsupported()
    }

    fn screen_pixel(&self, _x: i32, _y: i32) -> io::Result<Rgb> {
        unsupported()
    }
}

/// The handle of the visible window that `window` fits best.
///
/// Only windows of a process whose executable `window` fits count, and ties
/// go to the window listed first, usually the front-most. The setup app
/// finds an application's window this way; the runtime picks among the
/// same tied windows with [`read_window`].
#[must_use]
pub fn best_window(
    window: &WindowMatch,
    processes: Vec<ProcessEntry>,
    windows: Vec<WindowEntry>,
) -> Option<u64> {
    best_windows(window, processes, windows)
        .first()
        .map(|entry| entry.handle)
}

/// The visible windows that `window` fits best, all with the same fit, in
/// the order they are listed; [`best_window`] takes the first.
///
/// Only windows of a process whose executable `window` fits count.
#[must_use]
pub fn best_windows(
    window: &WindowMatch,
    processes: Vec<ProcessEntry>,
    windows: Vec<WindowEntry>,
) -> Vec<WindowEntry> {
    let executables: BTreeMap<_, _> = processes
        .into_iter()
        .filter(|process| window.executable.matches(&process.executable))
        .map(|process| (process.id, process.executable))
        .collect();
    let candidates: Vec<_> = windows
        .into_iter()
        .filter(|entry| entry.visible)
        .filter_map(|entry| {
            let executable = executables.get(&entry.process)?;
            Some((entry, executable))
        })
        .collect();
    let ranked = window.rank(candidates.iter().map(|(entry, executable)| WindowFacts {
        title: &entry.title,
        class: &entry.class,
        executable,
    }));
    let best = ranked.first().map(|&(_, fit)| fit);
    // Within a fit, `rank` keeps the listed order.
    let tied: Vec<usize> = ranked
        .into_iter()
        .take_while(|&(_, fit)| Some(fit) == best)
        .map(|(position, _)| position)
        .collect();
    candidates
        .into_iter()
        .enumerate()
        .filter(|(position, _)| tied.binary_search(position).is_ok())
        .map(|(_, (entry, _))| entry)
        .collect()
}

/// The window the runtime reads among `best`, the windows tied for the best
/// fit from [`best_windows`], and whether it shows.
///
/// When `current`, the process a session follows, owns one of them, it is
/// one of its windows, so a second copy of the application does not take
/// the session over. Otherwise any of them counts. Among those it is the
/// first that `shown` says shows, or the first when none does.
///
/// Detection follows the process that owns this window and the reader
/// reads it, so the two agree on one process.
#[must_use]
pub fn read_window(
    best: &[WindowEntry],
    current: Option<ProcessId>,
    mut shown: impl FnMut(u64) -> bool,
) -> Option<(&WindowEntry, bool)> {
    let owns = |entry: &&WindowEntry| Some(entry.process) == current;
    let pool: Vec<&WindowEntry> = if best.iter().any(|entry| owns(&entry)) {
        best.iter().filter(owns).collect()
    } else {
        best.iter().collect()
    };
    match pool.iter().copied().find(|entry| shown(entry.handle)) {
        Some(entry) => Some((entry, true)),
        None => pool.first().map(|&entry| (entry, false)),
    }
}

/// Whether a window shows, from its place. A window whose place cannot be
/// read counts as shown, so a failing check never stops a capture that
/// works.
#[must_use]
pub fn shows(place: io::Result<Option<WindowPlace>>) -> bool {
    !matches!(place, Ok(None | Some(WindowPlace { shown: false, .. })))
}

#[cfg(test)]
mod tests {
    use appytappy_config::anti_cheat::AntiCheatClass;

    use super::*;

    fn process(id: u32, executable: &str) -> ProcessEntry {
        ProcessEntry {
            id: ProcessId::new(id),
            parent: ProcessId::new(0),
            executable: executable.to_owned(),
            threads: 1,
        }
    }

    fn window(handle: u64, process: u32, title: &str, visible: bool) -> WindowEntry {
        WindowEntry {
            handle,
            process: ProcessId::new(process),
            title: title.to_owned(),
            class: "ExampleWindow".to_owned(),
            visible,
            bounds: Bounds {
                left: 0,
                top: 0,
                right: 4,
                bottom: 2,
            },
        }
    }

    #[test]
    fn the_best_window_is_visible_belongs_to_the_executable_and_fits_best() {
        let example = WindowMatch::exact(WindowFacts {
            title: "Example App",
            class: "ExampleWindow",
            executable: "Example.exe",
        });
        let processes = || {
            vec![
                process(1, "Example.exe"),
                process(2, "Other.exe"),
                process(3, "Example.exe"),
            ]
        };
        let best = |windows| best_window(&example, processes(), windows);
        assert_eq!(best(vec![window(5, 1, "Example App", false)]), None);
        assert_eq!(best(vec![window(5, 2, "Example App", true)]), None);
        assert_eq!(best(vec![window(5, 9, "Example App", true)]), None);
        // An untitled window of the application still counts.
        assert_eq!(best(vec![window(5, 1, "", true)]), Some(5));
        assert_eq!(
            best(vec![
                window(5, 1, "Launcher", true),
                window(6, 1, "Example App", true),
                window(7, 1, "Example App", true),
            ]),
            Some(6)
        );
        let tied = best_windows(
            &example,
            processes(),
            vec![
                window(5, 1, "Launcher", true),
                window(9, 3, "Example App", true),
                window(6, 1, "Example App", true),
                window(7, 2, "Example App", true),
                window(8, 1, "Example App", true),
            ],
        );
        assert_eq!(
            tied.iter().map(|entry| entry.handle).collect::<Vec<_>>(),
            [9, 6, 8],
            "every window with the best fit, in listed order"
        );
    }

    #[test]
    fn the_window_read_is_the_current_process_window_then_one_that_shows() {
        // Copy 40's window is listed first but minimized; copy 42 shows.
        let tied = [
            window(5, 40, "Example App", true),
            window(6, 42, "Example App", true),
            window(7, 40, "Example App", true),
        ];
        let minimized = [5];
        let read = |current: Option<u32>, minimized: &[u64]| {
            read_window(&tied, current.map(ProcessId::new), |handle| {
                !minimized.contains(&handle)
            })
            .map(|(entry, shown)| (entry.handle, shown))
        };
        assert_eq!(
            read(None, &minimized),
            Some((6, true)),
            "the first that shows"
        );
        assert_eq!(read(None, &[]), Some((5, true)), "the first listed");
        assert_eq!(
            read(Some(42), &[]),
            Some((6, true)),
            "the current process's"
        );
        assert_eq!(
            read(Some(40), &minimized),
            Some((7, true)),
            "the current process's window that shows"
        );
        assert_eq!(
            read(Some(40), &[5, 7]),
            Some((5, false)),
            "the current process keeps it while its windows are minimized"
        );
        assert_eq!(read(Some(43), &[5, 6, 7]), Some((5, false)), "none shows");
        assert_eq!(read_window(&[], None, |_| true), None);
    }

    #[test]
    fn a_window_shows_unless_its_place_says_otherwise() {
        let place = |shown| {
            Ok(Some(WindowPlace {
                bounds: Bounds {
                    left: 0,
                    top: 0,
                    right: 4,
                    bottom: 2,
                },
                shown,
                front: false,
            }))
        };
        assert!(shows(place(true)));
        assert!(!shows(place(false)));
        assert!(!shows(Ok(None)), "gone");
        assert!(shows(Err(io::Error::other("denied"))), "unknown");
    }

    #[test]
    fn the_fallback_takes_a_window_of_the_executable_only() {
        let mut example = WindowMatch::exact(WindowFacts {
            title: "Example App",
            class: "ExampleWindow",
            executable: "Example.exe",
        });
        example.must_match.title = true;
        let processes = || vec![process(1, "Example.exe"), process(2, "Other.exe")];
        let windows = || {
            vec![
                window(5, 2, "Example App", true),
                window(6, 1, "Launcher", true),
            ]
        };
        assert_eq!(best_window(&example, processes(), windows()), None);
        example.fallback = true;
        assert_eq!(best_window(&example, processes(), windows()), Some(6));
    }

    #[test]
    fn unsupported_system_fails_every_call() {
        let system = Unsupported;
        let entry = ProcessEntry {
            id: ProcessId::new(1),
            parent: ProcessId::new(0),
            executable: "example.exe".to_owned(),
            threads: 1,
        };
        let permit = ProcessAccessPermit::issue(entry.id, AntiCheatClass::None)
            .expect("class none permits access");
        let kinds = [
            system.processes().map(drop),
            system.windows().map(drop),
            system.window_place(7).map(drop),
            system.executable_build(entry.id).map(drop),
            system.ask_memory_access(&entry, &[]).map(drop),
            system.open(&permit).map(drop),
            system.screen_pixel(0, 0).map(drop),
        ]
        .map(|result| result.expect_err("unsupported").kind());
        assert_eq!(kinds, [io::ErrorKind::Unsupported; 7]);
    }
}
