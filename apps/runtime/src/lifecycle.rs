//! Game detection: which watched games are running, from window events and
//! process snapshots.
//!
//! A window event only says that something changed on the desktop. A burst
//! of events becomes one process snapshot [`SETTLE`] after the first event,
//! and the snapshot decides which games started and exited. A snapshot lists
//! executable names without opening any process, so detection is safe for
//! every anti-cheat class. While no watched game runs, only a new top-level
//! window leads to a snapshot, so nothing polls. While a game runs, a
//! destroyed window leads to one too, and a backstop snapshot runs every
//! [`RESCAN`] in case a game exits without an event, such as a process that
//! never made a window.

use std::collections::BTreeMap;
use std::io;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use appytappy_config::anti_cheat::ProcessId;
use appytappy_platform::WindowEvent;
use appytappy_sources::system::ProcessEntry;

/// How long after the first window event of a burst the snapshot runs.
pub(crate) const SETTLE: Duration = Duration::from_millis(250);

/// How often a snapshot runs while a game runs or snapshots fail.
pub(crate) const RESCAN: Duration = Duration::from_secs(5);

/// The position of a game in the watchlist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct GameIndex(pub(crate) usize);

/// The games to watch, each named by the executables that run it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Watchlist {
    /// Each game's executable names, lowercased, because Windows compares
    /// file names without case.
    games: Vec<Vec<String>>,
    /// Each game's first executable name as given, for the log.
    names: Vec<String>,
}

impl Watchlist {
    /// Watches `games`, each a list of executable names such as
    /// `GeometryDash.exe`. A game with no names is never detected.
    pub(crate) fn new(games: Vec<Vec<String>>) -> Self {
        let names = games
            .iter()
            .map(|executables| executables.first().cloned().unwrap_or_default())
            .collect();
        let games = games
            .into_iter()
            .map(|executables| {
                executables
                    .iter()
                    .map(|executable| executable.to_lowercase())
                    .collect()
            })
            .collect();
        Self { games, names }
    }

    /// A name for `game` in the log.
    pub(crate) fn name(&self, game: GameIndex) -> &str {
        self.names
            .get(game.0)
            .map_or("an unknown game", String::as_str)
    }

    fn is_empty(&self) -> bool {
        self.games.is_empty()
    }
}

/// What detection found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Change {
    /// A watched game started; the session follows this process.
    Started {
        /// The game.
        game: GameIndex,
        /// The process its session follows.
        process: ProcessId,
    },
    /// The process a game's session followed exited.
    Exited {
        /// The game.
        game: GameIndex,
        /// The process that exited.
        process: ProcessId,
    },
    /// A process snapshot failed, so detection cannot see starts and exits.
    ScanFailed(String),
    /// Snapshots work again after [`Change::ScanFailed`].
    ScanRestored,
}

/// Which watched games run, and when the next snapshot is due.
#[derive(Debug)]
pub(crate) struct Lifecycle {
    watchlist: Watchlist,
    /// The process each running game's session follows.
    running: BTreeMap<GameIndex, ProcessId>,
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
    /// Starts detection at `now`. The first snapshot is due at once, so a
    /// game that already runs is found.
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

    /// Applies a snapshot taken at `now` and returns what changed.
    pub(crate) fn scan(
        &mut self,
        snapshot: io::Result<Vec<ProcessEntry>>,
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
                self.reconcile(&processes, &mut changes);
            },
        }
        self.rescan_at = (self.failing || !self.running.is_empty()).then(|| later(now, RESCAN));
        changes
    }

    /// Ends the session of each game whose process is gone, then starts one
    /// for each game that runs without one. A session follows the process
    /// with the lowest ID, so a second copy of a game does not start a second
    /// session.
    fn reconcile(&mut self, processes: &[ProcessEntry], changes: &mut Vec<Change>) {
        let processes: Vec<(ProcessId, String)> = processes
            .iter()
            .map(|entry| (entry.id, entry.executable.to_lowercase()))
            .collect();
        for (position, executables) in self.watchlist.games.iter().enumerate() {
            let game = GameIndex(position);
            let runs = |name: &String| executables.contains(name);
            if let Some(&process) = self.running.get(&game) {
                // The name check catches a process ID that Windows reused.
                if processes
                    .iter()
                    .any(|(id, name)| *id == process && runs(name))
                {
                    continue;
                }
                self.running.remove(&game);
                changes.push(Change::Exited { game, process });
            }
            if let Some(&(process, _)) = processes
                .iter()
                .filter(|(_, name)| runs(name))
                .min_by_key(|(id, _)| *id)
            {
                self.running.insert(game, process);
                changes.push(Change::Started { game, process });
            }
        }
    }
}

/// The lifecycle thread: runs snapshots when they are due and sends what
/// changed. It ends when the window events stop or nobody receives changes.
pub(crate) fn run(
    mut lifecycle: Lifecycle,
    events: &Receiver<WindowEvent>,
    changes: &Sender<Change>,
    mut snapshot: impl FnMut() -> io::Result<Vec<ProcessEntry>>,
) {
    loop {
        let now = Instant::now();
        let event = match lifecycle.deadline() {
            Some(due) if due <= now => {
                for change in lifecycle.scan(snapshot(), now) {
                    if changes.send(change).is_err() {
                        return;
                    }
                }
                continue;
            },
            Some(due) => match events.recv_timeout(due.saturating_duration_since(now)) {
                Ok(event) => event,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => return,
            },
            None => match events.recv() {
                Ok(event) => event,
                Err(_disconnected) => return,
            },
        };
        lifecycle.window_event(event, Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::thread;

    use super::*;

    fn entry(id: u32, executable: &str) -> ProcessEntry {
        ProcessEntry {
            id: ProcessId::new(id),
            parent: ProcessId::new(0),
            executable: executable.to_owned(),
            threads: 1,
        }
    }

    fn watching(executables: &[&[&str]]) -> Watchlist {
        Watchlist::new(
            executables
                .iter()
                .map(|names| names.iter().map(|&name| name.to_owned()).collect())
                .collect(),
        )
    }

    fn geometry_dash() -> Watchlist {
        watching(&[&["GeometryDash.exe"]])
    }

    const GAME: GameIndex = GameIndex(0);

    fn started(id: u32) -> Change {
        Change::Started {
            game: GAME,
            process: ProcessId::new(id),
        }
    }

    fn exited(id: u32) -> Change {
        Change::Exited {
            game: GAME,
            process: ProcessId::new(id),
        }
    }

    #[test]
    fn the_first_snapshot_is_due_at_once_and_finds_a_running_game() {
        let now = Instant::now();
        let mut lifecycle = Lifecycle::new(geometry_dash(), now);
        assert_eq!(lifecycle.deadline(), Some(now));
        let changes = lifecycle.scan(
            Ok(vec![
                entry(7, "explorer.exe"),
                entry(42, "geometrydash.EXE"),
            ]),
            now,
        );
        assert_eq!(changes, [started(42)]);
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
        let mut lifecycle = Lifecycle::new(geometry_dash(), now);
        assert_eq!(lifecycle.scan(Ok(Vec::new()), now), []);
        assert_eq!(lifecycle.deadline(), None, "idle detection does not poll");
        lifecycle.window_event(WindowEvent::Destroyed, now);
        assert_eq!(lifecycle.deadline(), None);
        lifecycle.window_event(WindowEvent::Created, now);
        assert_eq!(lifecycle.deadline(), Some(later(now, SETTLE)));
    }

    #[test]
    fn a_burst_of_events_keeps_the_first_events_deadline() {
        let now = Instant::now();
        let mut lifecycle = Lifecycle::new(geometry_dash(), now);
        lifecycle.scan(Ok(Vec::new()), now);
        lifecycle.window_event(WindowEvent::Created, now);
        lifecycle.window_event(WindowEvent::Created, later(now, Duration::from_millis(100)));
        assert_eq!(lifecycle.deadline(), Some(later(now, SETTLE)));
    }

    #[test]
    fn a_running_game_is_rechecked_on_destroyed_windows_and_by_the_backstop() {
        let now = Instant::now();
        let mut lifecycle = Lifecycle::new(geometry_dash(), now);
        lifecycle.scan(Ok(vec![entry(42, "GeometryDash.exe")]), now);
        assert_eq!(lifecycle.deadline(), Some(later(now, RESCAN)));
        lifecycle.window_event(WindowEvent::Destroyed, now);
        assert_eq!(lifecycle.deadline(), Some(later(now, SETTLE)));
    }

    #[test]
    fn a_game_whose_process_is_gone_exits_once() {
        let now = Instant::now();
        let mut lifecycle = Lifecycle::new(geometry_dash(), now);
        lifecycle.scan(Ok(vec![entry(42, "GeometryDash.exe")]), now);
        assert_eq!(
            lifecycle.scan(Ok(vec![entry(42, "GeometryDash.exe")]), now),
            []
        );
        assert_eq!(lifecycle.scan(Ok(Vec::new()), now), [exited(42)]);
        assert_eq!(lifecycle.scan(Ok(Vec::new()), now), []);
        assert_eq!(lifecycle.deadline(), None, "detection is idle again");
    }

    #[test]
    fn a_reused_process_id_ends_the_session() {
        let now = Instant::now();
        let mut lifecycle = Lifecycle::new(geometry_dash(), now);
        lifecycle.scan(Ok(vec![entry(42, "GeometryDash.exe")]), now);
        assert_eq!(
            lifecycle.scan(Ok(vec![entry(42, "notepad.exe")]), now),
            [exited(42)]
        );
    }

    #[test]
    fn a_second_copy_takes_over_when_the_first_exits() {
        let now = Instant::now();
        let mut lifecycle = Lifecycle::new(geometry_dash(), now);
        let both = vec![entry(50, "GeometryDash.exe"), entry(42, "GeometryDash.exe")];
        assert_eq!(lifecycle.scan(Ok(both), now), [started(42)]);
        assert_eq!(
            lifecycle.scan(Ok(vec![entry(50, "GeometryDash.exe")]), now),
            [exited(42), started(50)]
        );
    }

    #[test]
    fn any_of_a_games_executables_detects_it_and_games_are_independent() {
        let now = Instant::now();
        let mut lifecycle = Lifecycle::new(watching(&[&["a.exe", "a64.exe"], &["b.exe"]]), now);
        let changes = lifecycle.scan(Ok(vec![entry(1, "b.exe"), entry(2, "A64.exe")]), now);
        assert_eq!(
            changes,
            [
                started(2),
                Change::Started {
                    game: GameIndex(1),
                    process: ProcessId::new(1)
                }
            ]
        );
    }

    #[test]
    fn a_failing_snapshot_is_reported_once_and_retried() {
        let now = Instant::now();
        let mut lifecycle = Lifecycle::new(geometry_dash(), now);
        let failure = || Err(io::Error::other("snapshot failed"));
        assert_eq!(
            lifecycle.scan(failure(), now),
            [Change::ScanFailed("snapshot failed".to_owned())]
        );
        assert_eq!(lifecycle.deadline(), Some(later(now, RESCAN)));
        assert_eq!(lifecycle.scan(failure(), now), []);
        assert_eq!(
            lifecycle.scan(Ok(vec![entry(42, "GeometryDash.exe")]), now),
            [Change::ScanRestored, started(42)]
        );
    }

    #[test]
    fn the_watchlist_names_each_game_by_its_first_executable() {
        let watchlist = watching(&[&["GeometryDash.exe", "other.exe"], &[]]);
        assert_eq!(watchlist.name(GameIndex(0)), "GeometryDash.exe");
        assert_eq!(watchlist.name(GameIndex(1)), "");
        assert_eq!(watchlist.name(GameIndex(2)), "an unknown game");
    }

    #[test]
    fn the_thread_reports_a_start_then_an_exit_after_a_destroyed_window() {
        let (window_events, events) = mpsc::channel();
        let (sender, changes) = mpsc::channel();
        let mut snapshots = vec![Vec::new(), vec![entry(42, "GeometryDash.exe")]];
        let lifecycle = Lifecycle::new(geometry_dash(), Instant::now());
        let thread = thread::spawn(move || {
            run(lifecycle, &events, &sender, || {
                Ok(snapshots.pop().unwrap_or_default())
            });
        });
        let wait = Duration::from_secs(5);
        assert_eq!(changes.recv_timeout(wait).expect("start"), started(42));
        let destroyed_at = Instant::now();
        window_events.send(WindowEvent::Destroyed).expect("send");
        assert_eq!(changes.recv_timeout(wait).expect("exit"), exited(42));
        assert!(
            destroyed_at.elapsed() >= SETTLE,
            "the snapshot waited for the burst to settle"
        );
        drop(window_events);
        thread
            .join()
            .expect("the thread ends when window events stop");
    }
}
