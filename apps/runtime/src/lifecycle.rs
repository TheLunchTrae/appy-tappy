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
//! [`session_window`], and keeps a process whose window went away for
//! [`OWNER_GRACE`]. Detection lists the windows only then.
//!
//! Each started app's executable is then identified: which build of it runs,
//! read from the file without opening the process, so the log can say when
//! it is not the build its values were checked against.

use std::collections::BTreeMap;
use std::io;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use appytappy_config::anti_cheat::ProcessId;
use appytappy_config::app_file::BuildDef;
use appytappy_config::window_match::WindowMatch;
use appytappy_platform::WindowEvent;
use appytappy_sources::system::{Pick, ProcessEntry, WindowEntry, session_window};

use crate::trace::{Detail, Record, Trace, What};

/// How long after the first window event of a burst the snapshot runs.
const SETTLE: Duration = Duration::from_millis(250);

/// How often a snapshot runs while an app runs or snapshots fail.
const RESCAN: Duration = Duration::from_secs(5);

/// How long a session keeps a process that owned one of the app's best
/// windows after it owns none, while another process does. An app that
/// goes full screen replaces its window, and the snapshot after the old
/// one closed may not list the new one yet. It is one [`RESCAN`], so the
/// grace lasts until the periodic snapshot that follows the one where the
/// process owned no window, and a change to how often snapshots run moves
/// it too.
const OWNER_GRACE: Duration = RESCAN;

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
    /// Record each process snapshot to this trace from now on.
    Trace(Trace),
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
    /// Which build of its executable an app that just started runs, or why
    /// that cannot be read. It follows the app's [`Change::Started`].
    Identified {
        /// The app's ID.
        app: String,
        /// The build, or why it cannot be read.
        build: Result<BuildDef, String>,
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
    /// For each app whose session's process owned one of the app's best
    /// windows, since when it owns none, or `None` while it owns one.
    owned: BTreeMap<String, Option<Instant>>,
    failing: bool,
    /// When the snapshot that window events asked for runs.
    scan_at: Option<Instant>,
    /// When the backstop snapshot runs.
    rescan_at: Option<Instant>,
}

fn later(now: Instant, delay: Duration) -> Instant {
    now.checked_add(delay).unwrap_or(now)
}

/// Whether `app`'s session keeps its process at `now` by [`OWNER_GRACE`]:
/// the process owned one of the app's best windows, and owns one still or
/// has owned none for less than the grace.
fn in_grace(owned: &BTreeMap<String, Option<Instant>>, app: &str, now: Instant) -> bool {
    match owned.get(app) {
        Some(Some(since)) => now < later(*since, OWNER_GRACE),
        Some(None) => true,
        None => false,
    }
}

impl Lifecycle {
    /// Starts detection at `now`. The first snapshot is due at once, so an
    /// app that already runs is found.
    pub(crate) fn new(watchlist: Watchlist, now: Instant) -> Self {
        let scan_at = (!watchlist.is_empty()).then_some(now);
        Self {
            watchlist,
            running: BTreeMap::new(),
            owned: BTreeMap::new(),
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
        self.owned.retain(|app, _| self.running.contains_key(app));
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
                self.reconcile(&processes, windows, shown, now, &mut changes);
            },
        }
        self.rescan_at = (self.failing || !self.running.is_empty()).then(|| later(now, RESCAN));
        changes
    }

    /// Ends the session of each app whose process is gone, then starts one
    /// for each app that runs without one. When several processes fit an
    /// app, its session follows the one that owns the window its reader
    /// reads, which [`session_window`] picks: one of the session's own
    /// process while it owns a window that fits best, so a second copy does
    /// not take over, and otherwise the first that shows. A process that
    /// owned such a window keeps the session for [`OWNER_GRACE`] after it
    /// owns none ([`Pick::Only`]), so a window it replaces does not hand the
    /// session to another copy. A helper started first, which never owned
    /// one, gives the session up to the process that opens the window at
    /// once. While no window fits, a session keeps its process, and a new one
    /// follows the process with the lowest ID.
    fn reconcile(
        &mut self,
        processes: &[ProcessEntry],
        windows: impl FnOnce() -> io::Result<Vec<WindowEntry>>,
        mut shown: impl FnMut(u64) -> bool,
        now: Instant,
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
            let several = fitting.len() > 1;
            let owner = if several {
                let all = listed.get_or_insert_with(|| {
                    windows
                        .take()
                        .and_then(|list| list().ok())
                        .unwrap_or_default()
                });
                let pick = match current {
                    Some(process) if in_grace(&self.owned, app, now) => Pick::Only {
                        process,
                        reading: None,
                    },
                    Some(process) => Pick::Prefer(process),
                    None => Pick::Any,
                };
                let candidates = fitting.iter().map(|&entry| entry.clone()).collect();
                session_window(window, pick, candidates, all.clone(), &mut shown)
                    .map(|picked| picked.entry.process)
            } else {
                None
            };
            let follow = owner
                .or(current)
                .or_else(|| fitting.iter().map(|entry| entry.id).min());
            let previous = self.running.get(app).copied();
            if previous != follow {
                if let Some(process) = previous {
                    self.running.remove(app);
                    self.owned.remove(app);
                    changes.push(Change::Exited {
                        app: app.clone(),
                        process,
                    });
                }
                if let Some(entry) =
                    follow.and_then(|id| fitting.iter().find(|entry| entry.id == id))
                {
                    self.running.insert(app.clone(), entry.id);
                    changes.push(Change::Started {
                        app: app.clone(),
                        process: entry.id,
                        executable: entry.executable.clone(),
                    });
                }
            }
            if owner.is_some() {
                // The session follows the process that owns the window it
                // reads.
                self.owned.insert(app.clone(), None);
            } else if let Some(since) = self.owned.get_mut(app).filter(|_| several) {
                // The grace starts at the first snapshot where the session's
                // process owns none of the windows it would read.
                since.get_or_insert(now);
            }
        }
    }
}

/// Sends each of `found` with `send`, and after each [`Change::Started`] a
/// [`Change::Identified`] with the build `identify` reads. The build is read
/// only once the start is sent, so reading it never holds back a session.
/// False once `send` fails.
fn send_identified(
    found: Vec<Change>,
    identify: &mut impl FnMut(ProcessId) -> io::Result<BuildDef>,
    mut send: impl FnMut(Change) -> bool,
) -> bool {
    for change in found {
        let started = match &change {
            Change::Started { app, process, .. } => Some((app.clone(), *process)),
            Change::Exited { .. }
            | Change::Identified { .. }
            | Change::ScanFailed(_)
            | Change::ScanRestored => None,
        };
        if !send(change) {
            return false;
        }
        if let Some((app, process)) = started {
            let build = identify(process).map_err(|error| error.to_string());
            if !send(Change::Identified { app, build }) {
                return false;
            }
        }
    }
    true
}

/// The lifecycle thread: runs snapshots when they are due and sends what
/// changed. `snapshot` lists the processes, `windows` the desktop's windows,
/// `shown` says whether a window shows, and `identify` reads each started
/// app's build. It ends when its input stops or nobody receives changes.
pub(crate) fn run<E: From<Change>>(
    mut lifecycle: Lifecycle,
    inputs: &Receiver<Input>,
    changes: &Sender<E>,
    mut snapshot: impl FnMut() -> io::Result<Vec<ProcessEntry>>,
    mut windows: impl FnMut() -> io::Result<Vec<WindowEntry>>,
    mut shown: impl FnMut(u64) -> bool,
    mut identify: impl FnMut(ProcessId) -> io::Result<BuildDef>,
) {
    let mut send = |found: Vec<Change>| {
        send_identified(found, &mut identify, |change| {
            changes.send(E::from(change)).is_ok()
        })
    };
    let mut trace = Trace::OFF;
    loop {
        let now = Instant::now();
        let input = match lifecycle.deadline() {
            Some(due) if due <= now => {
                let processes = snapshot();
                if trace.wants(What::Snapshot.category()) {
                    let took = now.elapsed();
                    let count = processes.as_ref().map_or(0, Vec::len);
                    trace.record(What::Snapshot, |record| Record {
                        took: Some(took),
                        detail: Detail::Count(count),
                        ..record
                    });
                }
                if !send(lifecycle.scan(processes, &mut windows, &mut shown, now)) {
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
            Input::Trace(next) => trace = next,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::thread;

    use appytappy_config::settings_file::TraceCategory;
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

    fn build() -> BuildDef {
        BuildDef {
            version: Some("1.4.2.0".to_owned()),
            size: 1_024,
            modified: 1_762_588_800,
        }
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
                |process| {
                    if process == ProcessId::new(42) {
                        Ok(build())
                    } else {
                        Err(io::Error::other("unreadable"))
                    }
                },
            );
        });
        let wait = Duration::from_secs(5);
        assert_eq!(changes.recv_timeout(wait).expect("start"), started(42));
        assert_eq!(
            changes.recv_timeout(wait).expect("identified"),
            Change::Identified {
                app: APP.to_owned(),
                build: Ok(build()),
            }
        );
        let (trace, entries) = Trace::for_test(1, &[TraceCategory::Lifecycle]);
        inputs.send(Input::Trace(trace)).expect("send");
        let destroyed_at = Instant::now();
        inputs
            .send(Input::from(WindowEvent::Destroyed))
            .expect("send");
        assert_eq!(changes.recv_timeout(wait).expect("exit"), exited(42));
        let Ok(crate::trace::writer::Entry::Record(snapshot)) = entries.try_recv() else {
            panic!("the snapshot after the trace started is recorded");
        };
        assert_eq!(
            (snapshot.what, snapshot.detail),
            (What::Snapshot, Detail::Count(0))
        );
        assert!(snapshot.took.is_some());
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
        assert_eq!(
            changes.recv_timeout(wait).expect("identified"),
            Change::Identified {
                app: "other".to_owned(),
                build: Err("unreadable".to_owned()),
            }
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
            "while no window fits, the lowest ID"
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
    fn a_build_is_read_after_its_start_is_sent_and_not_once_sending_fails() {
        #[derive(Debug, PartialEq)]
        enum Step {
            Sent(Change),
            Read(ProcessId),
        }
        let started = |app: &str, process| Change::Started {
            app: app.to_owned(),
            process: ProcessId::new(process),
            executable: "Example.exe".to_owned(),
        };
        let exited = || Change::Exited {
            app: "two".to_owned(),
            process: ProcessId::new(2),
        };
        let found = || vec![started("one", 1), exited(), started("three", 3)];
        let build = || BuildDef {
            version: Some("1.0.0.0".to_owned()),
            size: 6,
            modified: 1,
        };
        let (log, steps) = mpsc::channel();
        let reads = log.clone();
        let mut identify = |process: ProcessId| {
            reads.send(Step::Read(process)).expect("log");
            if process == ProcessId::new(1) {
                Ok(build())
            } else {
                Err(io::Error::other("unreadable"))
            }
        };
        assert!(send_identified(found(), &mut identify, |change| {
            log.send(Step::Sent(change)).is_ok()
        }));
        let identified = |app: &str, build| {
            Step::Sent(Change::Identified {
                app: app.to_owned(),
                build,
            })
        };
        assert_eq!(
            steps.try_iter().collect::<Vec<_>>(),
            [
                Step::Sent(started("one", 1)),
                Step::Read(ProcessId::new(1)),
                identified("one", Ok(build())),
                Step::Sent(exited()),
                Step::Sent(started("three", 3)),
                Step::Read(ProcessId::new(3)),
                identified("three", Err("unreadable".to_owned())),
            ]
        );

        // Once nobody receives, nothing more is read or sent.
        assert!(!send_identified(found(), &mut identify, |_| false));
        assert_eq!(steps.try_iter().count(), 0);
        let mut count = 0;
        assert!(!send_identified(found(), &mut identify, |_| {
            count += 1;
            count < 2
        }));
        assert_eq!(
            steps.try_iter().collect::<Vec<_>>(),
            [Step::Read(ProcessId::new(1))]
        );
        assert_eq!(count, 2, "the build of the first start was the last send");
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

    #[test]
    fn a_copy_whose_window_is_replaced_keeps_the_session_for_a_while() {
        let now = Instant::now();
        let mut lifecycle = Lifecycle::new(example_app(), now);
        let copies = || Ok(vec![entry(40, "Example.exe"), entry(42, "Example.exe")]);
        let both = || {
            Ok(vec![
                window(5, 40, "Example App", true),
                window(6, 42, "Example App", true),
            ])
        };
        // Copy 40 goes full screen: its old window closed, and the new one is
        // not listed yet.
        let only_42 = || Ok(vec![window(6, 42, "Example App", true)]);
        assert_eq!(
            lifecycle.scan(copies(), both, all_shown, now),
            [started(40)]
        );
        let gap = later(now, SETTLE);
        assert_eq!(
            lifecycle.scan(copies(), only_42, all_shown, gap),
            [],
            "one snapshot without its window does not hand the session over"
        );
        let back = later(gap, RESCAN);
        assert_eq!(lifecycle.scan(copies(), both, all_shown, back), []);
        let gone = later(back, SETTLE);
        assert_eq!(
            lifecycle.scan(copies(), only_42, all_shown, gone),
            [],
            "its window came back, so the wait starts again"
        );
        assert_eq!(
            lifecycle.scan(copies(), only_42, all_shown, later(gone, OWNER_GRACE)),
            [exited(40), started(42)],
            "after the grace, the copy that owns the window takes over"
        );
    }

    #[test]
    fn a_session_keeps_its_process_until_the_grace_ends() {
        let since = Instant::now();
        let owned = BTreeMap::from([("gone".to_owned(), Some(since)), ("owns".to_owned(), None)]);
        let ends = later(since, OWNER_GRACE);
        assert!(
            in_grace(&owned, "gone", since),
            "the grace starts when the window goes"
        );
        assert!(!in_grace(&owned, "gone", ends), "and ends after the grace");
        assert!(
            in_grace(&owned, "owns", ends),
            "a process that owns a window keeps it"
        );
        assert!(
            !in_grace(&owned, "none", since),
            "a process that owned none has no grace"
        );
    }

    #[test]
    fn the_grace_starts_only_while_another_copy_runs() {
        let now = Instant::now();
        let mut lifecycle = Lifecycle::new(example_app(), now);
        let copies = || Ok(vec![entry(40, "Example.exe"), entry(42, "Example.exe")]);
        let both = || {
            Ok(vec![
                window(5, 40, "Example App", true),
                window(6, 42, "Example App", true),
            ])
        };
        let only_42 = || Ok(vec![window(6, 42, "Example App", true)]);
        assert_eq!(
            lifecycle.scan(copies(), both, all_shown, now),
            [started(40)]
        );
        // Copy 42 exits for a while, so no window is listed.
        let alone = later(now, RESCAN);
        assert_eq!(
            lifecycle.scan_unlisted(Ok(vec![entry(40, "Example.exe")]), alone),
            []
        );
        let back = later(alone, OWNER_GRACE);
        assert_eq!(
            lifecycle.scan(copies(), only_42, all_shown, back),
            [],
            "the grace starts when copy 42 is back and owns the only window"
        );
    }

    #[test]
    fn an_app_watched_again_forgets_which_process_owned_its_window() {
        let now = Instant::now();
        let mut lifecycle = Lifecycle::new(example_app(), now);
        let both = || Ok(vec![entry(40, "Example.exe"), entry(42, "Example.exe")]);
        let app_window = || Ok(vec![window(6, 42, "Example App", true)]);
        let hidden = || Ok(vec![window(6, 42, "Example App", false)]);
        assert_eq!(
            lifecycle.scan(both(), app_window, all_shown, now),
            [started(42)]
        );
        assert_eq!(lifecycle.watch(watching(&[]), now), [exited(42)]);
        lifecycle.watch(example_app(), now);
        assert_eq!(
            lifecycle.scan(both(), hidden, all_shown, now),
            [started(40)],
            "while no window fits, the lowest ID"
        );
        assert_eq!(
            lifecycle.scan(both(), app_window, all_shown, now),
            [exited(40), started(42)],
            "process 40 never owned the window, so it gets no grace"
        );
    }
}
