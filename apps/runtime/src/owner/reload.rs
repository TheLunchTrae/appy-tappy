//! What the owner knows about reading the saved files, and what each read
//! means: which files to read again and when, whether automations start as
//! the tray last left them, and the orders that put what was read to use.
//! The owner reads the files, lends the read of the tray's last state for
//! the one decision that needs it, and carries out what comes back; every
//! decision is here, where a table of reads tests it without a disk.

use std::time::{Duration, Instant};

use appytappy_config::settings_file::{ReadInterval, SettingsFile};

use super::Step;
use super::lines::{interval_line, loaded_line};
use crate::lifecycle::Watchlist;
use crate::model::{Model, Order};
use crate::plan;

/// How long the owner waits before it reads the saved files again after
/// they could not be read, such as while another program held one.
const RETRY_WAIT: Duration = Duration::from_secs(2);

/// How many times in a row the owner reads the saved files again before it
/// waits for the next save.
const RETRIES: u32 = 5;

/// Which saved files the owner reads: the settings, the applications and
/// automations, or both.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Stale {
    /// The applications and automations.
    pub(super) files: bool,
    /// The settings.
    pub(super) settings: bool,
}

impl Stale {
    /// Every saved file.
    pub(super) const BOTH: Self = Self {
        files: true,
        settings: true,
    };
}

/// What the owner knows about reading the saved files: when it tries again,
/// which files that try reads, and what it has read so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Reloads {
    /// When it reads again the files that did not read.
    retry: Option<Retry>,
    /// The files that did not read at the last try.
    failing: Stale,
    /// Whether the applications and automations ever read.
    had: Had,
    /// Whether the settings ever read.
    had_settings: bool,
    /// Whether the user picked Enable or Disable from the tray in this run,
    /// which outranks how the tray left them in an earlier run.
    picked: bool,
}

impl Default for Reloads {
    /// Before the first read.
    fn default() -> Self {
        Self {
            retry: None,
            failing: Stale::default(),
            had: Had::Nothing,
            had_settings: false,
            picked: false,
        }
    }
}

impl Reloads {
    /// Notes what `step` means for the reads to come, before the owner takes
    /// it: a pick from the tray outranks how the tray left automations in an
    /// earlier run, and a save starts the tries over.
    pub(super) fn after(&mut self, step: &Step) {
        match step {
            Step::Picked(_) => self.picked = true,
            Step::Reload => self.retry = None,
            Step::Carry(_) | Step::Due(..) | Step::TraceFailed(_) | Step::Stop => {},
        }
    }

    /// The files to read again at `now`, when a retry is due.
    pub(super) fn due(&self, now: Instant) -> Option<Stale> {
        self.retry
            .is_some_and(|retry| retry.due(now))
            .then_some(self.failing)
    }
}

/// When the owner reads the saved files again after a read failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Retry {
    /// When.
    at: Instant,
    /// How many more tries follow this one, should it fail too.
    left: u32,
}

impl Retry {
    /// Whether the files are to be read again at `now`.
    fn due(self, now: Instant) -> bool {
        self.at <= now
    }
}

/// What the runtime runs while the saved files cannot be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Had {
    /// Nothing, since no read has worked since it started.
    Nothing,
    /// The plan from the last read that worked.
    Plan,
}

impl Had {
    /// What the runtime does meanwhile, as the log says it.
    const fn keeps(self) -> &'static str {
        match self {
            Self::Nothing => "runs no automations",
            Self::Plan => "keeps running what it ran",
        }
    }
}

/// What the runtime uses while the settings cannot be read at start.
const DEFAULTS: &str = "uses the default settings";

/// What the runtime uses while the settings cannot be read later.
const KEPT: &str = "keeps its settings";

/// What one read of the saved files gave: for each file the read named,
/// what it read or why it could not.
#[derive(Debug, Default)]
pub(super) struct Read {
    /// The settings and the read interval they ask for, when the read named
    /// them.
    pub(super) settings: Option<Result<(SettingsFile, ReadInterval), String>>,
    /// The plan of the saved applications and automations, when the read
    /// named them.
    pub(super) plan: Option<Result<plan::Plan, String>>,
}

/// What a read gives the owner to carry out.
#[derive(Debug)]
pub(super) struct Reloaded {
    /// Lines for the log, in order.
    pub(super) notes: Vec<String>,
    /// The orders that put what was read to use.
    pub(super) orders: Vec<Order>,
    /// The applications to watch for, when they read.
    pub(super) watchlist: Option<Watchlist>,
    /// The settings, when they read, for the trace to follow.
    pub(super) settings: Option<SettingsFile>,
}

#[cfg(test)]
impl Reloads {
    /// After a read whose applications and automations did not read.
    pub(super) fn unread() -> Self {
        Self {
            failing: Stale {
                files: true,
                settings: false,
            },
            ..Self::default()
        }
    }
}

impl Reloads {
    /// When the owner reads the files that did not read again, if it does.
    pub(super) fn retry_at(&self) -> Option<Instant> {
        self.retry.map(|retry| retry.at)
    }

    /// Whether the applications and automations did not read at the last
    /// try, so the plan the model runs may not be what was saved.
    pub(super) const fn files_failing(&self) -> bool {
        self.failing.files
    }

    /// Puts `read`, made at `now`, to use in `model`. `enabled` says how the
    /// tray last left automations, and is asked only when it decides
    /// anything.
    ///
    /// Files that do not read are kept as they were: the model runs the
    /// plan and settings it has, and a retry follows unless the tries ran
    /// out. The first settings that read, even on a retry, decide whether
    /// automations start enabled: as the tray last left them when they say
    /// to remember, and otherwise enabled, as the model starts. Settings read
    /// later, or after the user picked Enable or Disable from the tray in
    /// this run, leave it to the tray.
    pub(super) fn reloaded(
        &mut self,
        model: &mut Model,
        read: Read,
        now: Instant,
        enabled: impl FnOnce() -> bool,
    ) -> Reloaded {
        let previous = self.retry;
        let mut retry = None;
        let mut failing = Stale::default();
        let mut notes = Vec::new();
        let mut orders = Vec::new();
        let mut interval = None;
        let mut kept = None;
        match read.settings {
            Some(Ok((settings, read_interval))) => {
                if settings.remember_enabled && !self.had_settings && !self.picked {
                    let on = enabled();
                    if !on {
                        notes.push("automations disabled, as the tray last left them".to_owned());
                    }
                    orders = model.set_enabled(on);
                }
                self.had_settings = true;
                if read_interval != model.interval() {
                    notes.push(interval_line(read_interval));
                }
                interval = Some(read_interval);
                kept = Some(settings);
            },
            Some(Err(error)) => {
                failing.settings = true;
                let keeps = if self.had_settings { KEPT } else { DEFAULTS };
                retry = failed(&mut notes, &error, previous, now, keeps);
            },
            None => {},
        }
        let mut watchlist = None;
        let mut apps = None;
        match read.plan {
            Some(Ok(plan)) => {
                notes.extend(plan.problems.iter().cloned());
                notes.push(loaded_line(&plan));
                self.had = Had::Plan;
                watchlist = Some(Watchlist::new(plan.windows()));
                apps = Some(plan.apps);
            },
            Some(Err(error)) => {
                failing.files = true;
                retry = failed(&mut notes, &error, previous, now, self.had.keeps());
            },
            None => {},
        }
        orders.extend(model.reload(interval, apps, now));
        self.retry = retry;
        self.failing = failing;
        Reloaded {
            notes,
            orders,
            watchlist,
            settings: kept,
        }
    }
}

/// Notes in `notes` that the saved files cannot be read for `error`, and
/// what the runtime `keeps` doing meanwhile, and returns when to read them
/// again: soon, unless `previous`, the try that just failed, was the last.
fn failed(
    notes: &mut Vec<String>,
    error: &str,
    previous: Option<Retry>,
    now: Instant,
    keeps: &str,
) -> Option<Retry> {
    let left = previous
        .map_or(RETRIES, |previous| previous.left)
        .checked_sub(1);
    let retry = left.and_then(|left| {
        Some(Retry {
            at: now.checked_add(RETRY_WAIT)?,
            left,
        })
    });
    let then = if retry.is_some() {
        format!("reads them again in {} seconds", RETRY_WAIT.as_secs())
    } else {
        "waits for the next save".to_owned()
    };
    notes.push(format!("{error}, so the runtime {keeps} and {then}"));
    retry
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use appytappy_config::process::ProcessId;
    use appytappy_config::window_match::Pattern;

    use super::*;
    use crate::lifecycle::Change;
    use crate::model::SessionId;
    use crate::owner::tests::app_plan;

    const SETTINGS_ONLY: Stale = Stale {
        files: false,
        settings: true,
    };

    /// Settings that read, remembering how the tray left automations or
    /// not, with the default read interval.
    fn settings(remember: bool) -> (SettingsFile, ReadInterval) {
        let settings = SettingsFile {
            remember_enabled: remember,
            ..SettingsFile::default()
        };
        (settings, ReadInterval::DEFAULT)
    }

    /// Settings that read and ask for `interval`.
    fn reading_every(interval: ReadInterval) -> Read {
        Read {
            settings: Some(Ok((SettingsFile::default(), interval))),
            ..Read::default()
        }
    }

    /// A file that does not read, for `why`.
    fn unreadable<T>(why: &str) -> Result<T, String> {
        Err(why.to_owned())
    }

    /// A plan of the applications `apps`, by ID.
    fn plan(apps: &[(&str, Arc<plan::AppPlan>)]) -> plan::Plan {
        plan::Plan {
            apps: apps
                .iter()
                .map(|(id, app)| ((*id).to_owned(), Arc::clone(app)))
                .collect(),
            problems: Vec::new(),
        }
    }

    /// A model that runs the application `a`, with one automation, whose
    /// reader started.
    fn running() -> Model {
        let mut model = Model::default();
        model.replan(BTreeMap::from([(
            "a".to_owned(),
            app_plan("Example App", &["Mute"]),
        )]));
        let orders = model.apply(Change::Started {
            app: "a".to_owned(),
            process: ProcessId::new(7),
            executable: "Example.exe".to_owned(),
        });
        assert!(matches!(orders.first(), Some(Order::Read { .. })));
        model
    }

    /// Asks how the tray left automations, which says `on`, and notes in
    /// `asked` that it was asked.
    fn tray_left(on: bool, asked: &mut bool) -> impl FnOnce() -> bool + '_ {
        move || {
            *asked = true;
            on
        }
    }

    #[test]
    fn settings_set_the_read_interval_and_unreadable_ones_are_kept() {
        let now = Instant::now();
        let later = now.checked_add(RETRY_WAIT).expect("a later time");
        let mut model = Model::default();
        let mut reloads = Reloads::default();
        let same = reloads.reloaded(
            &mut model,
            reading_every(ReadInterval::DEFAULT),
            now,
            || true,
        );
        assert_eq!((same.orders, same.notes), (Vec::new(), Vec::new()));
        let slower =
            ReadInterval::new(Duration::from_millis(40), Duration::from_secs(2)).expect("in order");
        let read = reloads.reloaded(&mut model, reading_every(slower), now, || true);
        assert_eq!(read.notes, ["values are read every 40 to 2000 ms"]);
        assert_eq!(model.interval(), slower);
        let failed = reloads.reloaded(
            &mut model,
            Read {
                settings: Some(unreadable("the settings cannot be read: not JSON")),
                ..Read::default()
            },
            now,
            || true,
        );
        assert_eq!(model.interval(), slower, "the settings are kept");
        assert_eq!(
            failed.notes,
            [
                "the settings cannot be read: not JSON, so the runtime keeps its settings and \
                 reads them again in 2 seconds"
            ]
        );
        assert_eq!(reloads.due(now), None, "not yet");
        assert_eq!(
            reloads.due(later),
            Some(SETTINGS_ONLY),
            "only the settings are read again"
        );
        assert_eq!(reloads.retry_at(), Some(later));
    }

    #[test]
    fn settings_that_never_read_leave_the_defaults() {
        let now = Instant::now();
        let later = now.checked_add(RETRY_WAIT).expect("a later time");
        let mut model = Model::default();
        let mut reloads = Reloads::default();
        let read = reloads.reloaded(
            &mut model,
            Read {
                settings: Some(unreadable("the settings cannot be read")),
                plan: Some(Ok(plan(&[]))),
            },
            now,
            || true,
        );
        assert!(read.watchlist.is_some(), "the files read");
        assert_eq!(read.settings, None);
        assert_eq!(model.interval(), ReadInterval::DEFAULT);
        assert_eq!(
            read.notes,
            [
                "the settings cannot be read, so the runtime uses the default settings and \
                 reads them again in 2 seconds",
                "no saved automation can run",
            ]
        );
        assert!(!reloads.files_failing());
        let retry = reloads.due(later).expect("a retry");
        assert_eq!(retry, SETTINGS_ONLY);
        let again = Read {
            settings: Some(Ok(settings(false))),
            ..Read::default()
        };
        let read = reloads.reloaded(&mut model, again, later, || true);
        assert_eq!(
            read.notes,
            Vec::<String>::new(),
            "the plan is not read again"
        );
        assert!(read.settings.is_some(), "the trace follows the settings");
        assert_eq!(reloads.retry_at(), None);
    }

    #[test]
    fn the_first_settings_start_automations_as_the_tray_left_them_if_they_remember() {
        // Each case: whether the settings remember, how the tray left
        // automations, whether they start enabled, and whether the tray was
        // asked.
        for (remember, left_on, starts, asks) in [
            (false, false, true, false),
            (true, false, false, true),
            (true, true, true, true),
        ] {
            let mut model = Model::default();
            let mut reloads = Reloads::default();
            let mut asked = false;
            let read = Read {
                settings: Some(Ok(settings(remember))),
                ..Read::default()
            };
            let read = reloads.reloaded(
                &mut model,
                read,
                Instant::now(),
                tray_left(left_on, &mut asked),
            );
            assert_eq!(model.enabled(), starts, "{remember} {left_on}");
            assert_eq!(asked, asks, "{remember} {left_on}");
            assert_eq!(
                read.notes
                    .contains(&"automations disabled, as the tray last left them".to_owned()),
                !starts,
                "{remember} {left_on}"
            );
        }
    }

    #[test]
    fn settings_read_later_or_after_a_pick_leave_automations_as_the_tray_has_them() {
        let remembering = || Read {
            settings: Some(Ok(settings(true))),
            ..Read::default()
        };
        let mut model = Model::default();
        let mut reloads = Reloads::default();
        let mut asked = false;
        let first = Read {
            settings: Some(Ok(settings(false))),
            ..Read::default()
        };
        reloads.reloaded(&mut model, first, Instant::now(), || true);
        reloads.reloaded(
            &mut model,
            remembering(),
            Instant::now(),
            tray_left(false, &mut asked),
        );
        assert!(
            model.enabled(),
            "turning the setting on changes nothing now"
        );
        assert!(!asked);

        let mut model = running();
        let mut reloads = Reloads::default();
        reloads.reloaded(
            &mut model,
            Read {
                settings: Some(unreadable("the settings cannot be read")),
                ..Read::default()
            },
            Instant::now(),
            || true,
        );
        reloads.after(&Step::Picked(Vec::new()));
        let read = reloads.reloaded(
            &mut model,
            remembering(),
            Instant::now(),
            tray_left(false, &mut asked),
        );
        assert_eq!(read.orders, []);
        assert!(model.enabled(), "the pick stands");
        assert!(!asked);
        assert_eq!(reloads.retry_at(), None);
    }

    #[test]
    fn settings_that_first_read_late_stop_what_runs_when_the_tray_left_it_off() {
        let mut model = running();
        let mut reloads = Reloads::default();
        // The first read fails, so the first settings that work come with a
        // retry, while the application runs.
        reloads.reloaded(
            &mut model,
            Read {
                settings: Some(unreadable("the settings cannot be read")),
                ..Read::default()
            },
            Instant::now(),
            || true,
        );
        assert!(reloads.retry_at().is_some() && model.enabled());
        let read = Read {
            settings: Some(Ok(settings(true))),
            ..Read::default()
        };
        assert_eq!(
            reloads
                .reloaded(&mut model, read, Instant::now(), || false)
                .orders,
            [Order::StopReading(SessionId(0), "Example App".to_owned())]
        );
        assert!(!model.enabled());
    }

    #[test]
    fn only_a_pick_marks_the_reloads_picked_and_only_a_save_ends_the_tries() {
        let retrying = || Reloads {
            retry: Some(Retry {
                at: Instant::now(),
                left: RETRIES,
            }),
            ..Reloads::default()
        };
        for step in [
            Step::Carry(Vec::new()),
            Step::Due(Instant::now(), Vec::new()),
            Step::TraceFailed(0),
            Step::Stop,
        ] {
            let mut reloads = retrying();
            reloads.after(&step);
            assert!(!reloads.picked && reloads.retry.is_some(), "{step:?}");
        }
        let mut reloads = retrying();
        reloads.after(&Step::Picked(Vec::new()));
        assert!(reloads.picked && reloads.retry.is_some());
        let mut reloads = retrying();
        reloads.after(&Step::Reload);
        assert!(!reloads.picked && reloads.retry.is_none());
    }

    #[test]
    fn files_that_do_not_read_keep_the_plan_and_are_tried_a_few_times() {
        let now = Instant::now();
        let later = now.checked_add(RETRY_WAIT).expect("a later time");
        let mut model = running();
        let mut reloads = Reloads {
            had: Had::Plan,
            ..Reloads::default()
        };
        let broken = || Read {
            plan: Some(unreadable("the automations cannot be read")),
            ..Read::default()
        };
        let mut tries = Vec::new();
        let mut notes = Vec::new();
        for _ in 0..=RETRIES {
            let read = reloads.reloaded(&mut model, broken(), now, || true);
            assert_eq!(read.orders, []);
            assert!(read.watchlist.is_none());
            notes.extend(read.notes);
            tries.push(reloads.retry.map(|retry| retry.left));
        }
        assert_eq!(
            model.active_automations(),
            ["Mute"],
            "the plan keeps running"
        );
        assert_eq!(
            tries,
            [Some(4), Some(3), Some(2), Some(1), Some(0), None],
            "the tries are bounded"
        );
        assert_eq!(
            notes.last().map(String::as_str),
            Some(
                "the automations cannot be read, so the runtime keeps running what it ran and \
                 waits for the next save"
            )
        );
        assert_eq!(reloads.due(later), None, "only a save reads them again");
        assert!(reloads.files_failing());
        // A save starts the tries over; both files failing share one try.
        reloads.after(&Step::Reload);
        let both = Read {
            settings: Some(unreadable("the settings cannot be read")),
            ..broken()
        };
        reloads.reloaded(&mut model, both, now, || true);
        assert_eq!(reloads.retry.map(|retry| retry.left), Some(4));
        assert_eq!(reloads.due(later), Some(Stale::BOTH));
        let fixed = Read {
            settings: Some(Ok(settings(false))),
            plan: Some(Ok(plan(&[]))),
        };
        let read = reloads.reloaded(&mut model, fixed, now, || true);
        assert!(read.watchlist.is_some(), "the files read");
        assert_eq!(reloads.retry_at(), None, "a read that works needs no retry");
        assert!(!reloads.files_failing());
        assert_eq!(
            read.orders,
            [Order::StopReading(SessionId(0), "Example App".to_owned())],
            "the app the plan dropped stops and does not start again"
        );
    }

    #[test]
    fn the_first_read_that_fails_runs_nothing() {
        let mut reloads = Reloads::default();
        let read = Read {
            plan: Some(unreadable("the automations cannot be read")),
            ..Read::default()
        };
        let read = reloads.reloaded(&mut Model::default(), read, Instant::now(), || true);
        assert_eq!(
            read.notes,
            [
                "the automations cannot be read, so the runtime runs no automations and reads \
                 them again in 2 seconds"
            ]
        );
    }

    #[test]
    fn a_plan_s_problems_and_what_it_runs_are_logged() {
        let mut reloads = Reloads::default();
        let read = Read {
            plan: Some(Ok(plan::Plan {
                apps: BTreeMap::from([("a".to_owned(), app_plan("Example App", &["Mute"]))]),
                problems: vec!["Pause cannot run: its value is gone".to_owned()],
            })),
            ..Read::default()
        };
        let read = reloads.reloaded(&mut Model::default(), read, Instant::now(), || true);
        assert_eq!(
            read.notes,
            [
                "Pause cannot run: its value is gone",
                "1 automation runs for Example App"
            ]
        );
    }

    #[test]
    fn a_new_executable_pattern_stops_the_session_and_starts_nothing_on_the_old_process() {
        let mut model = running();
        let mut renamed = app_plan("Example App", &["Mute"]);
        Arc::get_mut(&mut renamed)
            .expect("unshared")
            .runs
            .window
            .executable = Pattern::new("Other.exe");
        let mut reloads = Reloads::default();
        let read = Read {
            plan: Some(Ok(plan(&[("a", renamed)]))),
            ..Read::default()
        };
        let read = reloads.reloaded(&mut model, read, Instant::now(), || true);
        assert_eq!(
            read.orders,
            [Order::StopReading(SessionId(0), "Example App".to_owned())],
            "one stop, and no reader on the old process until detection finds one"
        );
        assert_eq!(model.active_automations(), Vec::<&str>::new());
    }
}
