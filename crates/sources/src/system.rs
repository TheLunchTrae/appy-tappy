//! The operating system services the runtime, the setup app, and the MCP
//! server read: process and window discovery that never opens a process, an
//! executable's build, and screen pixels.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::io;

use appytappy_config::anti_cheat::ProcessId;
use appytappy_config::app_file::BuildDef;
use appytappy_config::window_match::{WindowFacts, WindowMatch};

use crate::screen::{Rgb, Size};

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

impl Bounds {
    /// The width and height, or `None` when an edge lies before its
    /// opposite one.
    #[must_use]
    pub fn size(self) -> Option<Size> {
        (self.right >= self.left && self.bottom >= self.top).then(|| Size {
            width: self.right.abs_diff(self.left),
            height: self.bottom.abs_diff(self.top),
        })
    }
}

/// The operating system services that read processes, windows, and the
/// screen.
pub trait System {
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

    /// The size of `window`'s content as a capture of it lends it now, or
    /// `None` once it closed or while it is minimized: see
    /// [`crate::screen::content`]. A capture can then start for the size its
    /// first frame has.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn content_size(&self, window: u64) -> io::Result<Option<Size>>;

    /// The bounds of `window`'s content on the screen, which a capture of it
    /// lends and a region's coordinates are measured from, or `None` once it
    /// closed: see [`crate::screen::content_on_screen`]. On Windows they
    /// leave out the title bar and borders that [`System::window_place`]'s
    /// bounds take in. This default gives those bounds.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn content_bounds(&self, window: u64) -> io::Result<Option<Bounds>> {
        Ok(self.window_place(window)?.map(|place| place.bounds))
    }

    /// The build of `process`'s executable, read from the file without
    /// opening the process: its file version, its size, and when it was
    /// written.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error, such as for a process that
    /// exited or a file it cannot read.
    fn executable_build(&self, process: ProcessId) -> io::Result<BuildDef>;
}

/// Reading the screen itself, which only the MCP server does. It is apart
/// from [`System`] so that nothing else carries it.
pub trait ScreenPixels {
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

fn unsupported<T>() -> io::Result<T> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "AppyTappy reads application state on Windows only",
    ))
}

impl System for Unsupported {
    fn processes(&self) -> io::Result<Vec<ProcessEntry>> {
        unsupported()
    }

    fn windows(&self) -> io::Result<Vec<WindowEntry>> {
        unsupported()
    }

    fn window_place(&self, _window: u64) -> io::Result<Option<WindowPlace>> {
        unsupported()
    }

    fn content_size(&self, _window: u64) -> io::Result<Option<Size>> {
        unsupported()
    }

    fn executable_build(&self, _process: ProcessId) -> io::Result<BuildDef> {
        unsupported()
    }
}

impl ScreenPixels for Unsupported {
    fn screen_pixel(&self, _x: i32, _y: i32) -> io::Result<Rgb> {
        unsupported()
    }
}

/// Each process's ID with its executable's name.
#[must_use]
pub fn executables(processes: Vec<ProcessEntry>) -> Vec<(ProcessId, String)> {
    processes
        .into_iter()
        .map(|process| (process.id, process.executable))
        .collect()
}

/// Which processes' windows a session may read, for [`session_window`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    /// No session yet, such as for a click or the setup app: a window of
    /// any process.
    Any,
    /// A session that follows this process and may move to another: one of
    /// the process's windows while it owns one, so a second copy of the
    /// application does not take the session over, and otherwise another
    /// process's, which the session then follows instead.
    Prefer(ProcessId),
    /// A session that keeps this process: only the process's windows. A
    /// reader reads only its session's process, and detection keeps a
    /// process for a grace period after it owns no such window.
    Only {
        /// The process.
        process: ProcessId,
        /// The window the session reads now, which it keeps while it shows,
        /// so windows that fit equally well do not swap back and forth.
        reading: Option<u64>,
    },
}

/// The window [`session_window`] picks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionWindow {
    /// The window.
    pub entry: WindowEntry,
    /// Whether it shows, so it can be read now.
    pub shown: bool,
}

/// The window a session reads for `window`, an application's window match,
/// among `windows`.
///
/// Detection follows the process that owns it, the reader reads it, a click
/// lands in it, and the setup app marks it as the best match, so they agree
/// on one window.
///
/// Only visible windows of a process whose executable `window` fits count,
/// and of those only the ones that fit best. `pick` says whose of those
/// count. Among them it is the window the session reads now while `shown`
/// says it shows, then the first listed that shows, usually the front-most,
/// then the first listed.
#[must_use]
pub fn session_window(
    window: &WindowMatch,
    pick: Pick,
    processes: Vec<ProcessEntry>,
    windows: Vec<WindowEntry>,
    mut shown: impl FnMut(u64) -> bool,
) -> Option<SessionWindow> {
    let best = best_windows(window, processes, windows);
    let (owner, reading) = match pick {
        Pick::Any => (None, None),
        Pick::Prefer(process) => (
            best.iter()
                .any(|entry| entry.process == process)
                .then_some(process),
            None,
        ),
        Pick::Only { process, reading } => (Some(process), reading),
    };
    let pool: Vec<WindowEntry> = best
        .into_iter()
        .filter(|entry| owner.is_none_or(|owner| entry.process == owner))
        .collect();
    let showing = pool
        .iter()
        .position(|entry| Some(entry.handle) == reading && shown(entry.handle))
        .or_else(|| pool.iter().position(|entry| shown(entry.handle)));
    let at = showing.unwrap_or(0);
    pool.into_iter().nth(at).map(|entry| SessionWindow {
        entry,
        shown: showing.is_some(),
    })
}

/// The visible windows that `window` fits best, all with the same fit, in
/// the order they are listed.
///
/// Only windows of a process whose executable `window` fits count.
fn best_windows(
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

/// Whether a window shows, from its place. A window whose place cannot be
/// read counts as shown, so a failing check never stops a capture that
/// works.
#[must_use]
pub fn shows(place: io::Result<Option<WindowPlace>>) -> bool {
    !matches!(place, Ok(None | Some(WindowPlace { shown: false, .. })))
}

#[cfg(test)]
mod tests {
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
    fn executables_name_each_process_by_its_id() {
        assert_eq!(
            executables(vec![process(4, "Player.exe"), process(9, "Chat.exe")]),
            [
                (ProcessId::new(4), "Player.exe".to_owned()),
                (ProcessId::new(9), "Chat.exe".to_owned()),
            ]
        );
    }

    fn example() -> WindowMatch {
        WindowMatch::exact(WindowFacts {
            title: "Example App",
            class: "ExampleWindow",
            executable: "Example.exe",
        })
    }

    /// The handle of the window `pick` takes among `windows` of processes
    /// 1 to 3, where 2 is another executable, with `minimized` not shown,
    /// and whether it shows.
    fn picked(
        window: &WindowMatch,
        pick: Pick,
        windows: Vec<WindowEntry>,
        minimized: &[u64],
    ) -> Option<(u64, bool)> {
        let processes = vec![
            process(1, "Example.exe"),
            process(2, "Other.exe"),
            process(3, "Example.exe"),
        ];
        session_window(window, pick, processes, windows, |handle| {
            !minimized.contains(&handle)
        })
        .map(|picked| (picked.entry.handle, picked.shown))
    }

    #[test]
    fn the_session_window_is_visible_belongs_to_the_executable_and_fits_best() {
        let best = |windows| picked(&example(), Pick::Any, windows, &[]);
        assert_eq!(best(vec![window(5, 1, "Example App", false)]), None);
        assert_eq!(best(vec![window(5, 2, "Example App", true)]), None);
        assert_eq!(best(vec![window(5, 9, "Example App", true)]), None);
        // An untitled window of the application still counts.
        assert_eq!(best(vec![window(5, 1, "", true)]), Some((5, true)));
        assert_eq!(
            best(vec![
                window(5, 1, "Launcher", true),
                window(6, 1, "Example App", true),
                window(7, 1, "Example App", true),
            ]),
            Some((6, true)),
            "the first listed of the best"
        );
        assert_eq!(
            picked(
                &example(),
                Pick::Any,
                vec![
                    window(5, 1, "Launcher", true),
                    window(6, 1, "Example App", true),
                ],
                &[6],
            ),
            Some((6, false)),
            "a minimized window that fits best over one that shows"
        );
    }

    #[test]
    fn the_session_window_is_its_process_s_then_the_read_one_then_one_that_shows() {
        // Copy 1's window is listed first but minimized; copy 3 shows.
        let tied = || {
            vec![
                window(5, 1, "Example App", true),
                window(6, 3, "Example App", true),
                window(7, 1, "Example App", true),
                window(8, 1, "Launcher", true),
            ]
        };
        let pick = |pick, minimized: &[u64]| picked(&example(), pick, tied(), minimized);
        let only = |process, reading| Pick::Only {
            process: ProcessId::new(process),
            reading,
        };
        let prefer = |process| Pick::Prefer(ProcessId::new(process));
        let cases = [
            (Pick::Any, &[5][..], Some((6, true)), "the first that shows"),
            (Pick::Any, &[], Some((5, true)), "the first listed"),
            (Pick::Any, &[5, 6, 7], Some((5, false)), "none shows"),
            (prefer(3), &[], Some((6, true)), "the process's own"),
            (
                prefer(1),
                &[5],
                Some((7, true)),
                "the process's own that shows",
            ),
            (
                prefer(1),
                &[5, 7],
                Some((5, false)),
                "the process keeps it while its windows are minimized",
            ),
            (
                prefer(4),
                &[5],
                Some((6, true)),
                "another process's while it owns none",
            ),
            (only(3, None), &[], Some((6, true)), "the process's own"),
            (only(4, None), &[], None, "never another process's"),
            (only(1, Some(7)), &[], Some((7, true)), "the one it reads"),
            (
                only(1, Some(7)),
                &[7],
                Some((5, true)),
                "the one it reads gives way once it is minimized",
            ),
            (
                only(1, Some(8)),
                &[],
                Some((5, true)),
                "the one it reads gives way once it no longer fits best",
            ),
        ];
        for (with, minimized, expected, case) in cases {
            assert_eq!(pick(with, minimized), expected, "{case}");
        }
        assert_eq!(picked(&example(), Pick::Any, Vec::new(), &[]), None);
    }

    #[test]
    fn the_fallback_takes_a_window_of_the_executable_only() {
        let mut example = example();
        example.must_match.title = true;
        let windows = || {
            vec![
                window(5, 2, "Example App", true),
                window(6, 1, "Launcher", true),
            ]
        };
        assert_eq!(picked(&example, Pick::Any, windows(), &[]), None);
        example.fallback = true;
        assert_eq!(picked(&example, Pick::Any, windows(), &[]), Some((6, true)));
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
    fn bounds_have_a_size_unless_an_edge_is_before_its_opposite() {
        let bounds = Bounds {
            left: -10,
            top: 5,
            right: 30,
            bottom: 25,
        };
        assert_eq!(
            bounds.size(),
            Some(Size {
                width: 40,
                height: 20
            })
        );
        let empty = Bounds {
            right: -10,
            bottom: 5,
            ..bounds
        };
        assert_eq!(
            empty.size(),
            Some(Size {
                width: 0,
                height: 0
            })
        );
        assert_eq!(
            Bounds {
                right: -11,
                ..bounds
            }
            .size(),
            None
        );
        assert_eq!(
            Bounds {
                bottom: 4,
                ..bounds
            }
            .size(),
            None
        );
        // Two edges may be farther apart than an `i32` holds.
        let widest = Bounds {
            left: i32::MIN,
            right: i32::MAX,
            ..bounds
        };
        assert_eq!(widest.size().map(|size| size.width), Some(u32::MAX));
    }

    #[test]
    fn a_frame_s_bounds_default_to_the_window_s() {
        struct OneWindow;
        impl System for OneWindow {
            fn processes(&self) -> io::Result<Vec<ProcessEntry>> {
                Ok(Vec::new())
            }
            fn windows(&self) -> io::Result<Vec<WindowEntry>> {
                Ok(Vec::new())
            }
            fn window_place(&self, window: u64) -> io::Result<Option<WindowPlace>> {
                Ok((window == 7).then_some(WindowPlace {
                    bounds: Bounds {
                        left: 1,
                        top: 2,
                        right: 30,
                        bottom: 40,
                    },
                    shown: true,
                    front: false,
                }))
            }
            fn content_size(&self, _window: u64) -> io::Result<Option<Size>> {
                unsupported()
            }
            fn executable_build(&self, _process: ProcessId) -> io::Result<BuildDef> {
                unsupported()
            }
        }
        assert_eq!(
            OneWindow.content_bounds(7).unwrap(),
            Some(Bounds {
                left: 1,
                top: 2,
                right: 30,
                bottom: 40
            })
        );
        assert_eq!(OneWindow.content_bounds(8).unwrap(), None);
    }

    #[test]
    fn unsupported_system_fails_every_call() {
        let system = Unsupported;
        let kinds = [
            system.processes().map(drop),
            system.windows().map(drop),
            system.window_place(7).map(drop),
            system.content_size(7).map(drop),
            system.content_bounds(7).map(drop),
            system.executable_build(ProcessId::new(1)).map(drop),
            system.screen_pixel(0, 0).map(drop),
        ]
        .map(|result| result.expect_err("unsupported").kind());
        assert_eq!(kinds, [io::ErrorKind::Unsupported; 7]);
    }
}
