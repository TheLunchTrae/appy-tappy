//! App detection: which watched apps are running, from window events and
//! process snapshots.
//!
//! A window event only says that something changed on the desktop. A burst
//! of events becomes one process snapshot [`SETTLE`] after the first event,
//! and the snapshot decides which apps started and exited. A snapshot lists
//! executable names without opening any process, so detection is safe for
//! every anti-cheat class. While no watched app runs, only a new top-level
//! window leads to a snapshot, so nothing polls. While an app runs, a
//! destroyed window leads to one too, and a backstop snapshot runs every
//! [`RESCAN`] in case an app exits without an event, such as a process that
//! never made a window.
//!
//! When several running processes fit an app's executable pattern, such as
//! a launcher or helper beside the app itself, the session follows the one
//! that owns the window its reader reads, which both pick with
//! [`read_window`]. Detection lists the windows only then.

use std::collections::BTreeMap;
use std::io;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use appytappy_config::anti_cheat::ProcessId;
use appytappy_config::window_match::WindowMatch;
use appytappy_platform::WindowEvent;
use appytappy_sources::system::{ProcessEntry, WindowEntry, best_windows, read_window};

/// How long after the first window event of a burst the snapshot runs.
pub(crate) const SETTLE: Duration = Duration::from_millis(250);

/// How often a snapshot runs while an app runs or snapshots fail.
pub(crate) const RESCAN: Duration = Duration::from_secs(5);

/// The apps to watch: each app's ID and how to find its window, whose
/// executable pattern names its processes, such as `Example.exe` or
/// `Example*.exe`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Watchlist(BTreeMap<String, WindowMatch>);

impl Watchlist {
    /// Watches `apps`, each an app's ID and its window match.
    pub(crate) fn new(apps: impl IntoIterator<Item = (String, WindowMatch)>) -> Self {
        Self(apps.into_iter().collect())
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// What reaches the lifecycle thread.
#[derive(Debug)]
pub(crate) enum Input {
    /// The desktop's windows changed.
    Window(WindowEvent),
    /// The saved files changed, so these are the apps to watch now.
    Watch(Watchlist),
}

impl From<WindowEvent> for Input {
    fn from(event: WindowEvent) -> Self {
        Self::Window(event)
    }
}

/// What detection found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Change {
    /// A watched app started; the session follows this process.
    Started {
        /// The app's ID.
        app: String,
        /// The process its session follows.
        process: ProcessId,
        /// The file name of the process's executable, such as `Example.exe`.
        executable: String,
    },
    /// The process an app's session followed exited, or the app is no longer
    /// watched.
    Exited {
        /// The app's ID.
        app: String,
        /// The process that exited.
        process: ProcessId,
    },
    /// A process snapshot failed, so detection cannot see starts and exits.
    ScanFailed(String),
    /// Snapshots work again after [`Change::ScanFailed`].
    ScanRestored,
}

/// Which watched apps run, and when the next snapshot is due.
#[derive(Debug)]
pub(crate) struct Lifecycle {
    watchlist: Watchlist,
    /// The process each running app's session follows, by app ID.
    running: BTreeMap<String, ProcessId>,
    failing: bool,
    /// When the snapshot that window events asked for runs.
    scan_at: Option<Instant>,
    /// When the backstop snapshot runs.
    rescan_at: Option<Instant>,
}

fn later(now: Instant, delay: Duration) -> Instant {
    now.checked_add(delay).unwrap_or(now)
}

impl Lifecycle {
    /// Starts detection at `now`. The first snapshot is due at once, so an
    /// app that already runs is found.
    pub(crate) fn new(watchlist: Watchlist, now: Instant) -> Self {
        let scan_at = (!watchlist.is_empty()).then_some(now);
        Self {
            watchlist,
            running: BTreeMap::new(),
            failing: false,
            scan_at,
            rescan_at: None,
        }
    }

    /// Notes a window event at `now`, which may make a snapshot due.
    pub(crate) fn window_event(&mut self, event: WindowEvent, now: Instant) {
        let relevant = match event {
            WindowEvent::Created => !self.watchlist.is_empty(),
            WindowEvent::Destroyed => !self.running.is_empty(),
        };
        if relevant && self.scan_at.is_none() {
            self.scan_at = Some(later(now, SETTLE));
        }
    }

    /// When the next snapshot is due, or `None` when only a window event can
    /// make one due.
    pub(crate) fn deadline(&self) -> Option<Instant> {
        match (self.scan_at, self.rescan_at) {
            (Some(scan), Some(rescan)) => Some(scan.min(rescan)),
            (scan, rescan) => scan.or(rescan),
        }
    }

    /// Watches `watchlist` from `now` on. Each running app that it drops, or
    /// whose executable's pattern it changes, exits; a snapshot is due at once
    /// to find the apps it adds.
    pub(crate) fn watch(&mut self, watchlist: Watchlist, now: Instant) -> Vec<Change> {
        let mut changes = Vec::new();
        let executable = |watchlist: &Watchlist, app: &str| {
            watchlist.0.get(app).map(|window| window.executable.clone())
        };
        self.running.retain(|app, &mut process| {
            let kept = executable(&self.watchlist, app) == executable(&watchlist, app);
            if !kept {
                changes.push(Change::Exited {
                    app: app.clone(),
                    process,
                });
            }
            kept
        });
        self.watchlist = watchlist;
        if !self.watchlist.is_empty() {
            self.scan_at = Some(now);
        }
        if self.running.is_empty() && !self.failing {
            self.rescan_at = None;
        }
        changes
    }

    /// Applies a snapshot taken at `now` and returns what changed. `windows`
    /// lists the desktop's windows and `shown` says whether a window shows,
    /// which it asks only when several processes fit one app.
    pub(crate) fn scan(
        &mut self,
        snapshot: io::Result<Vec<ProcessEntry>>,
        windows: impl FnOnce() -> io::Result<Vec<WindowEntry>>,
        shown: impl FnMut(u64) -> bool,
        now: Instant,
    ) -> Vec<Change> {
        self.scan_at = None;
        let mut changes = Vec::new();
        match snapshot {
            Err(error) => {
                if !self.failing {
                    self.failing = true;
                    changes.push(Change::ScanFailed(error.to_string()));
                }
            },
            Ok(processes) => {
                if self.failing {
                    self.failing = false;
                    changes.push(Change::ScanRestored);
                }
                self.reconcile(&processes, windows, shown, &mut changes);
            },
        }
        self.rescan_at = (self.failing || !self.running.is_empty()).then(|| later(now, RESCAN));
        changes
    }

    /// Ends the session of each app whose process is gone, then starts one
    /// for each app that runs without one. When several processes fit an
    /// app, its session follows the one that owns the window its reader
    /// reads, which [`read_window`] picks: one of the session's own process
    /// while it owns a window that fits best, so a second copy does not take
    /// over, and otherwise the first that shows. A helper started first gives
    /// the session up to the process that opens the window. While no window
    /// fits, a session keeps its process, and a new one follows the process
    /// with the lowest ID.
    fn reconcile(
        &mut self,
        processes: &[ProcessEntry],
        windows: impl FnOnce() -> io::Result<Vec<WindowEntry>>,
        mut shown: impl FnMut(u64) -> bool,
        changes: &mut Vec<Change>,
    ) {
        let mut windows = Some(windows);
        // Listed once, on first need. Without the list, detection follows
        // processes alone, and the reader reports why it cannot find the
        // window.
        let mut listed: Option<Vec<WindowEntry>> = None;
        for (app, window) in &self.watchlist.0 {
            let fitting: Vec<&ProcessEntry> = processes
                .iter()
                .filter(|entry| window.executable.matches(&entry.executable))
                .collect();
            let current = self.running.get(app).copied();
            // The name check catches a process ID that Windows reused.
            let current =
                current.filter(|&process| fitting.iter().any(|entry| entry.id == process));
            let owner = if fitting.len() > 1 {
                let all = listed.get_or_insert_with(|| {
                    windows
                        .take()
                        .and_then(|list| list().ok())
                        .unwrap_or_default()
                });
                let candidates = fitting.iter().map(|&entry| entry.clone()).collect();
                let best = best_windows(window, candidates, all.clone());
                read_window(&best, current, &mut shown).map(|(best, _)| best.process)
            } else {
                None
            };
            let follow = owner
                .or(current)
                .or_else(|| fitting.iter().map(|entry| entry.id).min());
            if let Some(&process) = self.running.get(app) {
                if Some(process) == follow {
                    continue;
                }
                self.running.remove(app);
                changes.push(Change::Exited {
                    app: app.clone(),
                    process,
                });
            }
            if let Some(entry) = follow.and_then(|id| fitting.iter().find(|entry| entry.id == id)) {
                self.running.insert(app.clone(), entry.id);
                changes.push(Change::Started {
                    app: app.clone(),
                    process: entry.id,
                    executable: entry.executable.clone(),
                });
            }
        }
    }
}

/// The lifecycle thread: runs snapshots when they are due and sends what
/// changed. `snapshot` lists the processes, `windows` the desktop's windows,
/// and `shown` says whether a window shows. It ends when its input stops or
/// nobody receives changes.
pub(crate) fn run<E: From<Change>>(
    mut lifecycle: Lifecycle,
    inputs: &Receiver<Input>,
    changes: &Sender<E>,
    mut snapshot: impl FnMut() -> io::Result<Vec<ProcessEntry>>,
    mut windows: impl FnMut() -> io::Result<Vec<WindowEntry>>,
    mut shown: impl FnMut(u64) -> bool,
) {
    let send = |found: Vec<Change>| {
        found
            .into_iter()
            .all(|change| changes.send(E::from(change)).is_ok())
    };
    loop {
        let now = Instant::now();
        let input = match lifecycle.deadline() {
            Some(due) if due <= now => {
                if !send(lifecycle.scan(snapshot(), &mut windows, &mut shown, now)) {
                    return;
                }
                continue;
            },
            Some(due) => match inputs.recv_timeout(due.saturating_duration_since(now)) {
                Ok(input) => input,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => return,
            },
            None => match inputs.recv() {
                Ok(input) => input,
                Err(_disconnected) => return,
            },
        };
        match input {
            Input::Window(event) => lifecycle.window_event(event, Instant::now()),
            Input::Watch(watchlist) => {
                if !send(lifecycle.watch(watchlist, Instant::now())) {
                    return;
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::thread;

    use appytappy_config::window_match::{MustMatch, Pattern};
    use appytappy_sources::system::Bounds;

    use super::*;

    impl Lifecycle {
        /// Applies a snapshot that needs no window list.
        fn scan_unlisted(
            &mut self,
            snapshot: io::Result<Vec<ProcessEntry>>,
            now: Instant,
        ) -> Vec<Change> {
            self.scan(snapshot, unlisted, unasked, now)
        }
    }

    fn unlisted() -> io::Result<Vec<WindowEntry>> {
        panic!("one process fits each app, so no window list is needed")
    }

    fn unasked(_: u64) -> bool {
        panic!("one process fits each app, so no window is checked")
    }

    /// Every window shows.
    fn all_shown(_: u64) -> bool {
        true
    }

    /// Matches `executable`'s windows titled `Example App`.
    fn matching(executable: &str) -> WindowMatch {
        WindowMatch {
            title: Pattern::new("Example App"),
            class: Pattern::new("*"),
            executable: Pattern::new(executable),
            must_match: MustMatch {
                title: true,
                class: false,
            },
            fallback: false,
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
                right: 800,
                bottom: 600,
            },
        }
    }

    fn entry(id: u32, executable: &str) -> ProcessEntry {
        ProcessEntry {
            id: ProcessId::new(id),
            parent: ProcessId::new(0),
            executable: executable.to_owned(),
            threads: 1,
        }
    }

    fn watching(apps: &[(&str, &str)]) -> Watchlist {
        Watchlist::new(
            apps.iter()
                .map(|&(app, executable)| (app.to_owned(), matching(executable))),
        )
    }

    fn example_app() -> Watchlist {
        watching(&[(APP, "Example.exe")])
    }

    const APP: &str = "example";

    fn started_as(app: &str, id: u32, executable: &str) -> Change {
        Change::Started {
            app: app.to_owned(),
            process: ProcessId::new(id),
            executable: executable.to_owned(),
        }
    }

    fn started(id: u32) -> Change {
        started_as(APP, id, "Example.exe")
    }

    fn exited(id: u32) -> Change {
        Change::Exited {
            app: APP.to_owned(),
            process: ProcessId::new(id),
        }
    }

    #[test]
    fn the_first_snapshot_is_due_at_once_and_finds_a_running_app() {
        let now = Instant::now();
        let mut lifecycle = Lifecycle::new(example_app(), now);
        assert_eq!(lifecycle.deadline(), Some(now));
        let changes = lifecycle.scan_unlisted(
            Ok(vec![entry(7, "explorer.exe"), entry(42, "example.EXE")]),
            now,
        );
        assert_eq!(changes, [started_as(APP, 42, "example.EXE")]);
    }

    #[test]
    fn an_empty_watchlist_never_scans() {
        let now = Instant::now();
        let mut lifecycle = Lifecycle::new(watching(&[]), now);
        lifecycle.window_event(WindowEvent::Created, now);
        assert_eq!(lifecycle.deadline(), None);
    }

    #[test]
    fn while_idle_only_a_new_window_makes_a_snapshot_due() {
        let now = Instant::now();
        let mut lifecycle = Lifecycle::new(example_app(), now);
        assert_eq!(lifecycle.scan_unlisted(Ok(Vec::new()), now), []);
        assert_eq!(lifecycle.deadline(), None, "idle detection does not poll");
        lifecycle.window_event(WindowEvent::Destroyed, now);
        assert_eq!(lifecycle.deadline(), None);
        lifecycle.window_event(WindowEvent::Created, now);
        assert_eq!(lifecycle.deadline(), Some(later(now, SETTLE)));
    }

    #[test]
    fn a_burst_of_events_keeps_the_first_events_deadline() {
        let now = Instant::now();
        let mut lifecycle = Lifecycle::new(example_app(), now);
        lifecycle.scan_unlisted(Ok(Vec::new()), now);
        lifecycle.window_event(WindowEvent::Created, now);
        lifecycle.window_event(WindowEvent::Created, later(now, Duration::from_millis(100)));
        assert_eq!(lifecycle.deadline(), Some(later(now, SETTLE)));
    }

    #[test]
    fn a_running_app_is_rechecked_on_destroyed_windows_and_by_the_backstop() {
        let now = Instant::now();
        let mut lifecycle = Lifecycle::new(example_app(), now);
        lifecycle.scan_unlisted(Ok(vec![entry(42, "Example.exe")]), now);
        assert_eq!(lifecycle.deadline(), Some(later(now, RESCAN)));
        lifecycle.window_event(WindowEvent::Destroyed, now);
        assert_eq!(lifecycle.deadline(), Some(later(now, SETTLE)));
    }

    #[test]
    fn an_app_whose_process_is_gone_exits_once() {
        let now = Instant::now();
        let mut lifecycle = Lifecycle::new(example_app(), now);
        lifecycle.scan_unlisted(Ok(vec![entry(42, "Example.exe")]), now);
        assert_eq!(
            lifecycle.scan_unlisted(Ok(vec![entry(42, "Example.exe")]), now),
            []
        );
        assert_eq!(lifecycle.scan_unlisted(Ok(Vec::new()), now), [exited(42)]);
        assert_eq!(lifecycle.scan_unlisted(Ok(Vec::new()), now), []);
        assert_eq!(lifecycle.deadline(), None, "detection is idle again");
    }

    #[test]
    fn a_reused_process_id_ends_the_session() {
        let now = Instant::now();
        let mut lifecycle = Lifecycle::new(example_app(), now);
        lifecycle.scan_unlisted(Ok(vec![entry(42, "Example.exe")]), now);
        assert_eq!(
            lifecycle.scan_unlisted(Ok(vec![entry(42, "Other.exe")]), now),
            [exited(42)]
        );
    }

    #[test]
    fn a_second_copy_takes_over_when_the_first_exits() {
        let now = Instant::now();
        let mut lifecycle = Lifecycle::new(example_app(), now);
        let both = vec![entry(50, "Example.exe"), entry(42, "Example.exe")];
        assert_eq!(
            lifecycle.scan(Ok(both), || Ok(Vec::new()), unasked, now),
            [started(42)]
        );
        assert_eq!(
            lifecycle.scan_unlisted(Ok(vec![entry(50, "Example.exe")]), now),
            [exited(42), started(50)]
        );
    }

    #[test]
    fn wildcards_detect_an_app_and_apps_are_independent() {
        let now = Instant::now();
        let mut lifecycle = Lifecycle::new(watching(&[("a", "a*.exe"), ("b", "b.exe")]), now);
        let changes =
            lifecycle.scan_unlisted(Ok(vec![entry(1, "b.exe"), entry(2, "A64.exe")]), now);
        assert_eq!(
            changes,
            [started_as("a", 2, "A64.exe"), started_as("b", 1, "b.exe")]
        );
    }

    #[test]
    fn a_new_watchlist_ends_dropped_apps_and_scans_for_new_ones() {
        let now = Instant::now();
        let both = watching(&[("a", "a.exe"), ("b", "b.exe")]);
        let mut lifecycle = Lifecycle::new(both, now);
        lifecycle.scan_unlisted(Ok(vec![entry(1, "a.exe"), entry(2, "b.exe")]), now);
        let later_on = later(now, Duration::from_secs(1));
        let changes = lifecycle.watch(watching(&[("a", "a.exe"), ("b", "b2.exe")]), later_on);
        assert_eq!(
            changes,
            [Change::Exited {
                app: "b".to_owned(),
                process: ProcessId::new(2)
            }],
            "a keeps its session; b's pattern changed"
        );
        assert_eq!(
            lifecycle.deadline(),
            Some(later_on),
            "a snapshot is due at once"
        );
        assert_eq!(
            lifecycle.scan_unlisted(Ok(vec![entry(1, "a.exe"), entry(3, "b2.exe")]), later_on),
            [started_as("b", 3, "b2.exe")]
        );
        let none = lifecycle.watch(watching(&[]), later_on);
        assert_eq!(none.len(), 2, "{none:?}");
        assert_eq!(lifecycle.deadline(), None, "nothing is left to watch");
    }

    #[test]
    fn a_failing_snapshot_is_reported_once_and_retried() {
        let now = Instant::now();
        let mut lifecycle = Lifecycle::new(example_app(), now);
        let failure = || Err(io::Error::other("snapshot failed"));
        assert_eq!(
            lifecycle.scan_unlisted(failure(), now),
            [Change::ScanFailed("snapshot failed".to_owned())]
        );
        assert_eq!(lifecycle.deadline(), Some(later(now, RESCAN)));
        assert_eq!(lifecycle.scan_unlisted(failure(), now), []);
        lifecycle.watch(watching(&[]), now);
        assert_eq!(
            lifecycle.deadline(),
            Some(later(now, RESCAN)),
            "a failing scan keeps retrying"
        );
        lifecycle.watch(example_app(), now);
        assert_eq!(
            lifecycle.scan_unlisted(Ok(vec![entry(42, "Example.exe")]), now),
            [Change::ScanRestored, started(42)]
        );
    }

    #[test]
    fn the_thread_follows_window_events_and_new_watchlists() {
        let (inputs, received) = mpsc::channel();
        let (sender, changes) = mpsc::channel::<Change>();
        let mut snapshots = vec![
            vec![entry(42, "Example.exe"), entry(7, "Other.exe")],
            Vec::new(),
            vec![entry(42, "Example.exe")],
        ];
        let lifecycle = Lifecycle::new(example_app(), Instant::now());
        let thread = thread::spawn(move || {
            run(
                lifecycle,
                &received,
                &sender,
                || Ok(snapshots.pop().unwrap_or_default()),
                unlisted,
                unasked,
            );
        });
        let wait = Duration::from_secs(5);
        assert_eq!(changes.recv_timeout(wait).expect("start"), started(42));
        let destroyed_at = Instant::now();
        inputs
            .send(Input::from(WindowEvent::Destroyed))
            .expect("send");
        assert_eq!(changes.recv_timeout(wait).expect("exit"), exited(42));
        assert!(
            destroyed_at.elapsed() >= SETTLE,
            "the snapshot waited for the burst to settle"
        );
        inputs
            .send(Input::Watch(watching(&[("other", "Other.exe")])))
            .expect("send");
        assert_eq!(
            changes.recv_timeout(wait).expect("start"),
            started_as("other", 7, "Other.exe")
        );
        drop(inputs);
        thread.join().expect("the thread ends when its input stops");
    }

    #[test]
    fn the_session_follows_the_process_that_owns_the_best_window() {
        let now = Instant::now();
        let mut lifecycle = Lifecycle::new(watching(&[(APP, "Example*.exe")]), now);
        let helper_and_app = || vec![entry(40, "ExampleHelper.exe"), entry(42, "Example.exe")];
        let app_window = || Ok(vec![window(5, 42, "Example App", true)]);
        let app = || started_as(APP, 42, "Example.exe");
        assert_eq!(
            lifecycle.scan(Ok(helper_and_app()), app_window, all_shown, now),
            [app()],
            "the helper has the lowest ID and no window"
        );
        let mut lifecycle = Lifecycle::new(watching(&[(APP, "Example*.exe")]), now);
        let hidden = || Ok(vec![window(5, 42, "Example App", false)]);
        assert_eq!(
            lifecycle.scan(Ok(helper_and_app()), hidden, all_shown, now),
            [started_as(APP, 40, "ExampleHelper.exe")],
            "while no window shows, the lowest ID"
        );
        assert_eq!(
            lifecycle.scan(Ok(helper_and_app()), app_window, all_shown, now),
            [
                Change::Exited {
                    app: APP.to_owned(),
                    process: ProcessId::new(40)
                },
                app()
            ],
            "the session moves to the process whose window opened"
        );
        let failing = || Err(io::Error::other("denied"));
        assert_eq!(
            lifecycle.scan(Ok(helper_and_app()), hidden, all_shown, now),
            [],
            "a session keeps its process while no window fits"
        );
        assert_eq!(
            lifecycle.scan(Ok(helper_and_app()), failing, all_shown, now),
            []
        );
    }

    #[test]
    fn a_minimized_copy_with_the_lower_id_gives_way_to_the_one_that_shows() {
        let now = Instant::now();
        let mut lifecycle = Lifecycle::new(example_app(), now);
        let copies = || vec![entry(40, "Example.exe"), entry(42, "Example.exe")];
        // Copy 40's window is listed first but minimized; copy 42's shows.
        let windows = || {
            Ok(vec![
                window(5, 40, "Example App", true),
                window(6, 42, "Example App", true),
            ])
        };
        let shown = |handle| handle != 5;
        assert_eq!(
            lifecycle.scan(Ok(copies()), windows, shown, now),
            [started(42)],
            "the copy whose window the reader reads"
        );
        assert_eq!(
            lifecycle.scan(Ok(copies()), windows, all_shown, now),
            [],
            "copy 40's window showing again does not take the session"
        );
        assert_eq!(
            lifecycle.scan(Ok(copies()), windows, |handle| handle != 6, now),
            [],
            "nor does copy 42's window being minimized"
        );
    }

    #[test]
    fn a_second_copy_takes_over_only_when_the_first_exits() {
        let now = Instant::now();
        let mut lifecycle = Lifecycle::new(example_app(), now);
        assert_eq!(
            lifecycle.scan_unlisted(Ok(vec![entry(42, "Example.exe")]), now),
            [started(42)]
        );
        // The second copy got a lower, reused ID, and its window is in front.
        let both = || vec![entry(40, "Example.exe"), entry(42, "Example.exe")];
        let windows = || {
            Ok(vec![
                window(5, 40, "Example App", true),
                window(6, 42, "Example App", true),
            ])
        };
        assert_eq!(lifecycle.scan(Ok(both()), windows, all_shown, now), []);
        assert_eq!(
            lifecycle.scan_unlisted(Ok(vec![entry(40, "Example.exe")]), now),
            [exited(42), started(40)]
        );
    }
}
