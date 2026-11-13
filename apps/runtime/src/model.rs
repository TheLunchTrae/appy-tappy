//! The root model on the owner thread: which apps are attached, what
//! problem, if any, stops the runtime from watching, whether automations
//! are enabled, whether their actions can run, and the rule session of
//! each app whose automations run.
//!
//! The model does no I/O and holds no channel. Each change returns the
//! [`Order`]s that carry it out: start or stop a reader, and run or undo an
//! automation's actions. The owner turns each order into a message for the
//! thread that carries it out. An app's automations run while it is
//! attached, automations are enabled, and the plan has automations for it.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use appytappy_actions::Watched;
use appytappy_config::action::Action;
use appytappy_config::rule::Restore;
use appytappy_config::signal::Value;
use appytappy_engine::{Effect, RuleIndex, Session};
use appytappy_sources::schedule::ReadInterval;

use crate::click::ClickTargets;
use crate::lifecycle::Change;
use crate::plan::{AppPlan, RulePlan};

/// One session of an attached application. A new session starts each time
/// the runtime starts reading an app, so a reading from a session that ended
/// is told apart and dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SessionId(pub(crate) u64);

/// One of a session's readers, counted from 0. A session starts its next
/// reader when its read interval changes or its reader ended, so what an
/// earlier reader read, or that it ended, is told apart and dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReaderNumber(pub(crate) u32);

/// How long a session waits before it starts a reader again, the first time
/// its reader ended. The wait doubles each time a reader of the session
/// ends, so a reader that ends as often as it starts cannot keep the owner
/// and the log busy.
pub(crate) const READ_AGAIN: Duration = Duration::from_secs(1);

/// The longest wait before a session starts a reader again.
pub(crate) const READ_AGAIN_MAX: Duration = Duration::from_secs(30);

/// Names a fired automation: its session and its rule's position, so two
/// sessions' changes are never mixed up.
pub(crate) type Owner = (SessionId, RuleIndex);

/// The runtime's state, as the log and the tray report it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum State {
    /// No watched app runs.
    Idle,
    /// At least one watched app runs and is attached.
    Attached,
    /// A problem stops the runtime from watching apps: process snapshots
    /// fail. It overrides the other two states until the problem clears.
    Degraded,
}

impl fmt::Display for State {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Idle => "idle",
            Self::Attached => "attached",
            Self::Degraded => "degraded",
        })
    }
}

/// What the owner carries out after a change.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Order {
    /// Start reading an app's values.
    Read {
        /// The session the readings belong to.
        session: SessionId,
        /// Which of the session's readers this starts.
        reader: ReaderNumber,
        /// The application's values and window.
        plan: Arc<AppPlan>,
        /// The process the session follows, whose window is read.
        watched: Watched,
        /// How often to read the values, from the settings.
        interval: ReadInterval,
    },
    /// Stop the reader of a session, named by its app's name for the log.
    StopReading(SessionId, String),
    /// Run an automation's actions in order.
    Run {
        /// The fired automation.
        owner: Owner,
        /// Its name, for the log.
        automation: String,
        /// The process its session follows: a key press presses keys only
        /// while that process's window is in front.
        watched: Watched,
        /// Its actions.
        actions: Box<[Action]>,
        /// What placing its clicks needs.
        clicks: Arc<ClickTargets>,
        /// Whether its changes are undone when it is released.
        restore: Restore,
        /// When the frame arrived whose reading fired it, for the trace.
        cause: Option<Instant>,
    },
    /// Undo what an automation changed.
    Undo {
        /// The released automation.
        owner: Owner,
        /// Its name, for the log.
        automation: String,
        /// When the frame arrived whose reading released it, for the trace.
        cause: Option<Instant>,
    },
    /// Nothing runs: an automation's cooldown held it back. The trace
    /// records it, with when the frame arrived whose reading it was.
    Held(Owner, Option<Instant>),
}

/// An app whose automations run.
#[derive(Debug)]
struct Running {
    /// The app's ID.
    app: String,
    plan: Arc<AppPlan>,
    session: Session,
    /// The process the session follows.
    watched: Watched,
    /// How often its reader reads.
    interval: ReadInterval,
    /// The reader that reads its values, or the last that did while
    /// `read_again` waits.
    reader: ReaderNumber,
    /// When the session starts a reader again, while its last one ended
    /// and none reads its values.
    read_again: Option<Instant>,
    /// How long the session waits before it starts a reader again, the
    /// next time one ends.
    wait: Duration,
    /// When the session last started a reader again after one ended.
    read_again_at: Option<Instant>,
}

impl Running {
    /// The order that starts the next reader of this session, `id`. The
    /// values are forgotten first, as when a reader can no longer see them,
    /// because a new reader sends each value as it first reads it.
    fn next_reader(&mut self, id: SessionId) -> Order {
        self.session.forget_values();
        self.reader = ReaderNumber(self.reader.0.wrapping_add(1));
        self.read_again = None;
        Order::Read {
            session: id,
            reader: self.reader,
            plan: Arc::clone(&self.plan),
            watched: self.watched.clone(),
            interval: self.interval,
        }
    }
}

/// The owner thread's state.
#[derive(Debug)]
pub(crate) struct Model {
    /// The process each attached app follows, by app ID.
    attached: BTreeMap<String, Watched>,
    /// The problem that degrades the runtime.
    problem: Option<String>,
    /// Whether rules may fire. Turning it off from the tray keeps detection
    /// running, so turning it back on finds the apps already attached.
    enabled: bool,
    /// Whether the action thread can run actions, as it last said. It is
    /// false while the operating system's controls cannot be made: rules
    /// still fire, but their actions do not run.
    actions_can_run: bool,
    /// What each app with automations runs, by app ID.
    plan: BTreeMap<String, Arc<AppPlan>>,
    /// The apps whose automations run now, by session.
    running: BTreeMap<SessionId, Running>,
    /// How often readers read, from the settings.
    interval: ReadInterval,
    /// The next session's ID.
    next: u64,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            attached: BTreeMap::new(),
            problem: None,
            enabled: true,
            actions_can_run: true,
            plan: BTreeMap::new(),
            running: BTreeMap::new(),
            interval: ReadInterval::DEFAULT,
            next: 0,
        }
    }
}

impl Model {
    /// Whether rules may fire.
    pub(crate) const fn enabled(&self) -> bool {
        self.enabled
    }

    /// Whether actions can run, as the action thread last said.
    pub(crate) const fn actions_can_run(&self) -> bool {
        self.actions_can_run
    }

    /// Records what the action thread said about whether actions can run.
    pub(crate) const fn set_actions_can_run(&mut self, can_run: bool) {
        self.actions_can_run = can_run;
    }

    /// The current state.
    pub(crate) fn state(&self) -> State {
        if self.problem.is_some() {
            State::Degraded
        } else if self.attached.is_empty() {
            State::Idle
        } else {
            State::Attached
        }
    }

    /// The names of the automations that run now, app by app in the order
    /// of their IDs.
    pub(crate) fn active_automations(&self) -> Vec<&str> {
        let mut running: Vec<&Running> = self.running.values().collect();
        running.sort_by(|one, other| one.app.cmp(&other.app));
        running
            .into_iter()
            .flat_map(|running| running.plan.runs.rules.iter())
            .map(|rule| rule.automation.as_str())
            .collect()
    }

    /// The plan of `app`, if it has automations that can run.
    pub(crate) fn plan(&self, app: &str) -> Option<&AppPlan> {
        self.plan.get(app).map(AsRef::as_ref)
    }

    /// The name of the app a session reads, for the log.
    pub(crate) fn app_name(&self, id: SessionId) -> Option<&str> {
        self.session_plan(id).map(|plan| plan.labels.name.as_str())
    }

    /// The newest plan of the application that session `id` runs.
    pub(crate) fn session_plan(&self, id: SessionId) -> Option<&AppPlan> {
        self.running.get(&id).map(|running| &*running.plan)
    }

    /// Each running session with its application's newest plan, so the trace
    /// names what a reload renamed without starting it over.
    pub(crate) fn sessions(&self) -> impl Iterator<Item = (SessionId, &Arc<AppPlan>)> {
        self.running
            .iter()
            .map(|(id, running)| (*id, &running.plan))
    }

    /// Applies what detection found.
    pub(crate) fn apply(&mut self, change: Change) -> Vec<Order> {
        match change {
            Change::Started {
                app,
                process,
                executable,
            } => {
                self.attached.insert(
                    app.clone(),
                    Watched {
                        process,
                        executable,
                    },
                );
                self.start(&app)
            },
            Change::Exited { app, .. } => {
                self.attached.remove(&app);
                self.session_of(&app)
                    .map_or_else(Vec::new, |id| self.stop(id))
            },
            // Which build an app runs changes nothing the model holds; the
            // owner notes it in the log.
            Change::Identified { .. } => Vec::new(),
            Change::ScanFailed(problem) => {
                self.problem = Some(problem);
                Vec::new()
            },
            Change::ScanRestored => {
                self.problem = None;
                Vec::new()
            },
        }
    }

    /// Lets rules fire, or stops them, without stopping detection. Stopping
    /// undoes what the running automations changed.
    pub(crate) fn set_enabled(&mut self, enabled: bool) -> Vec<Order> {
        self.enabled = enabled;
        if enabled {
            let apps: Vec<String> = self.attached.keys().cloned().collect();
            apps.iter().flat_map(|app| self.start(app)).collect()
        } else {
            std::mem::take(&mut self.running)
                .into_iter()
                .flat_map(|(id, running)| stopped(id, running))
                .collect()
        }
    }

    /// Switches to a new plan. An app whose part runs differently starts
    /// over with it, undoing what its automations changed. An app that runs
    /// the same keeps running untouched and only takes the new plan, so
    /// renaming it or its parts, or marking its values as checked, restarts
    /// nothing.
    pub(crate) fn replan(&mut self, plan: BTreeMap<String, Arc<AppPlan>>) -> Vec<Order> {
        let mut changed = Vec::new();
        for (id, running) in &mut self.running {
            match plan.get(&running.app) {
                Some(new) if new.runs == running.plan.runs => {
                    running.plan = Arc::clone(new);
                },
                _ => changed.push(*id),
            }
        }
        let mut orders: Vec<Order> = changed.into_iter().flat_map(|id| self.stop(id)).collect();
        self.plan = plan;
        let attached: Vec<String> = self.attached.keys().cloned().collect();
        for app in attached {
            orders.extend(self.start(&app));
        }
        orders
    }

    /// How often readers read.
    pub(crate) const fn interval(&self) -> ReadInterval {
        self.interval
    }

    /// Sets how often readers read. Readers that start from now on read at
    /// it; [`Model::restart_readers`] moves the running ones to it.
    pub(crate) const fn set_interval(&mut self, interval: ReadInterval) {
        self.interval = interval;
    }

    /// Starts over each running app's reader that reads at another interval
    /// than the model's, in the same session, so its automations keep their
    /// state and nothing is undone. What the old reader sent just before it
    /// stopped carries its number, so it is dropped. A session whose reader
    /// ended starts no reader here: the one it starts when its wait passes
    /// reads at the new interval.
    pub(crate) fn restart_readers(&mut self) -> Vec<Order> {
        let interval = self.interval;
        let mut orders = Vec::new();
        for (id, running) in &mut self.running {
            if running.interval == interval {
                continue;
            }
            running.interval = interval;
            if running.read_again.is_none() {
                orders.push(Order::StopReading(*id, running.plan.labels.name.clone()));
                orders.push(running.next_reader(*id));
            }
        }
        orders
    }

    /// Notes at `now` that reader `reader` of session `id` ended though
    /// nothing stopped it, such as when it panicked. The session keeps
    /// running: its values are forgotten, as when a reader can no longer
    /// see them, and [`Model::recheck`] starts its next reader after a wait
    /// that starts at [`READ_AGAIN`] and doubles each time a reader of the
    /// session ends, up to [`READ_AGAIN_MAX`]. A reader that ends after it
    /// ran for [`READ_AGAIN_MAX`] waits [`READ_AGAIN`] again. The order
    /// joins the reader that ended, which says why it did.
    ///
    /// A reader also says it ended once the session stopped it. That comes
    /// from a session that ended, or from a reader that is not the
    /// session's newest, and is dropped: no order.
    pub(crate) fn reader_ended(
        &mut self,
        id: SessionId,
        reader: ReaderNumber,
        now: Instant,
    ) -> Vec<Order> {
        let Some(running) = self.read_by(id, reader) else {
            return Vec::new();
        };
        let ran_long = |started| now.saturating_duration_since(started) >= READ_AGAIN_MAX;
        if running.read_again_at.is_some_and(ran_long) {
            running.wait = READ_AGAIN;
        }
        running.session.forget_values();
        running.read_again = Some(now.checked_add(running.wait).unwrap_or(now));
        running.wait = running.wait.saturating_mul(2).min(READ_AGAIN_MAX);
        vec![Order::StopReading(id, running.plan.labels.name.clone())]
    }

    /// Session `id` while `reader` reads its values: the session runs,
    /// `reader` is its newest reader, and it has not ended. What any other
    /// reader sent is dropped with this: one of a session that ended, one
    /// the session replaced, and one that ended, whose capture can still
    /// send what it read after the reader said it ended.
    fn read_by(&mut self, id: SessionId, reader: ReaderNumber) -> Option<&mut Running> {
        self.running
            .get_mut(&id)
            .filter(|running| running.reader == reader && running.read_again.is_none())
    }

    /// Records the value at `position` among the values that session `id`
    /// reads, as its reader `reader` read it at `at`. A reading from any
    /// reader but the one that reads the session's values is dropped.
    pub(crate) fn sample(
        &mut self,
        id: SessionId,
        reader: ReaderNumber,
        position: usize,
        value: Value,
        at: Instant,
    ) -> Vec<Order> {
        let Some(running) = self.read_by(id, reader) else {
            return Vec::new();
        };
        // The session reads the values of its plan, which a reload replaces
        // only with a plan that reads the same.
        let Some(read) = running.plan.runs.values.get(position) else {
            return Vec::new();
        };
        let effects = running.session.sample(read.signal.clone(), value, at);
        acts(id, &running.plan, &running.watched, effects, Some(at))
    }

    /// When [`Model::recheck`] next has something to do, across every app:
    /// a running automation that its cooldown held back is due, or a
    /// session whose reader ended starts one again.
    pub(crate) fn next_due(&self) -> Option<Instant> {
        self.running
            .values()
            .flat_map(|running| [running.session.next_due(), running.read_again])
            .flatten()
            .min()
    }

    /// Does what is due at `at`: starts a reader again for each session
    /// whose reader ended and whose wait has passed, and fires the
    /// automations whose cooldown held them back while their condition
    /// held, and whose cooldown has passed.
    pub(crate) fn recheck(&mut self, at: Instant) -> Vec<Order> {
        self.running
            .iter_mut()
            .flat_map(|(id, running)| {
                let read = running.read_again.is_some_and(|due| due <= at).then(|| {
                    running.read_again_at = Some(at);
                    running.next_reader(*id)
                });
                let effects = running.session.recheck(at);
                read.into_iter()
                    .chain(acts(*id, &running.plan, &running.watched, effects, None))
            })
            .collect()
    }

    /// Forgets the values that `reader`, which reads session `id`'s
    /// values, can no longer see, and returns whether it did: not for any
    /// other reader. Each rule keeps its state, so an automation that ran
    /// stays run, and nothing fires or is undone until the values read next
    /// decide its condition.
    pub(crate) fn forget(&mut self, id: SessionId, reader: ReaderNumber) -> bool {
        self.read_by(id, reader)
            .map(|running| running.session.forget_values())
            .is_some()
    }

    /// The session that runs `app`'s automations, if any.
    fn session_of(&self, app: &str) -> Option<SessionId> {
        self.running
            .iter()
            .find(|(_, running)| running.app == app)
            .map(|(id, _)| *id)
    }

    /// Starts running `app`'s automations, if it is attached, automations
    /// are enabled, and the plan has automations for it.
    fn start(&mut self, app: &str) -> Vec<Order> {
        let (Some(watched), Some(plan)) = (self.attached.get(app), self.plan.get(app)) else {
            return Vec::new();
        };
        if !self.enabled || self.session_of(app).is_some() {
            return Vec::new();
        }
        let id = SessionId(self.next);
        self.next = self.next.wrapping_add(1);
        let rules = plan
            .runs
            .rules
            .iter()
            .map(|rule| rule.rule.clone())
            .collect();
        let reader = ReaderNumber(0);
        let order = Order::Read {
            session: id,
            reader,
            plan: Arc::clone(plan),
            watched: watched.clone(),
            interval: self.interval,
        };
        self.running.insert(
            id,
            Running {
                app: app.to_owned(),
                plan: Arc::clone(plan),
                session: Session::new(rules),
                watched: watched.clone(),
                interval: self.interval,
                reader,
                read_again: None,
                wait: READ_AGAIN,
                read_again_at: None,
            },
        );
        vec![order]
    }

    /// Stops running session `id`'s automations, undoing what they changed.
    fn stop(&mut self, id: SessionId) -> Vec<Order> {
        self.running
            .remove(&id)
            .map_or_else(Vec::new, |running| stopped(id, running))
    }
}

/// The orders that stop `running`, session `id`, and undo what its
/// automations changed.
fn stopped(id: SessionId, running: Running) -> Vec<Order> {
    let effects = running.session.stop();
    let name = running.plan.labels.name.clone();
    let mut orders = vec![Order::StopReading(id, name)];
    orders.extend(acts(id, &running.plan, &running.watched, effects, None));
    orders
}

/// The orders for a session's `effects`, caused by the reading of the
/// frame that arrived at `cause`, if any.
fn acts(
    id: SessionId,
    plan: &AppPlan,
    watched: &Watched,
    effects: Vec<Effect>,
    cause: Option<Instant>,
) -> Vec<Order> {
    effects
        .into_iter()
        .filter_map(|effect| {
            let (Effect::Fire(index) | Effect::Release(index) | Effect::Held(index)) = effect;
            let owner = (id, index);
            // The session started with the rules of this plan, or of one
            // that a reload swapped in because its rules are the same, so
            // each index it gives names a rule here.
            let RulePlan { automation, rule } = plan.runs.rules.get(index.0)?;
            Some(match effect {
                Effect::Fire(_) => Order::Run {
                    owner,
                    automation: automation.clone(),
                    watched: watched.clone(),
                    actions: rule.actions().into(),
                    clicks: Arc::clone(&plan.runs.clicks),
                    restore: rule.restore(),
                    cause,
                },
                Effect::Release(_) => Order::Undo {
                    owner,
                    automation: automation.clone(),
                    cause,
                },
                Effect::Held(_) => Order::Held(owner, cause),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use appytappy_config::anti_cheat::ProcessId;
    use appytappy_config::app_file::BuildDef;
    use appytappy_config::rule::{Comparison, Condition, Restore, Rule};
    use appytappy_engine::RuleIndex;

    use appytappy_config::signal::SignalName;

    use super::*;
    use crate::plan::examples::{number, pause, plan, plan_with_cooldown, signal, value};
    use crate::plan::{Labels, Runs};

    fn plans(apps: &[(&str, &Arc<AppPlan>)]) -> BTreeMap<String, Arc<AppPlan>> {
        apps.iter()
            .map(|(app, plan)| ((*app).to_owned(), Arc::clone(plan)))
            .collect()
    }

    fn started(app: &str, process: u32) -> Change {
        Change::Started {
            app: app.to_owned(),
            process: ProcessId::new(process),
            executable: "Example.exe".to_owned(),
        }
    }

    fn exited(app: &str, process: u32) -> Change {
        Change::Exited {
            app: app.to_owned(),
            process: ProcessId::new(process),
        }
    }

    /// The orders as short words, for comparing sequences.
    fn kinds(orders: &[Order]) -> Vec<String> {
        orders
            .iter()
            .map(|order| match order {
                Order::Read { session, plan, .. } => {
                    format!("read {} {}", session.0, plan.labels.name)
                },
                Order::StopReading(id, name) => format!("stop {} {name}", id.0),
                Order::Run {
                    owner, automation, ..
                } => format!("run {} {automation}", owner.0.0),
                Order::Undo {
                    owner, automation, ..
                } => format!("undo {} {automation}", owner.0.0),
                Order::Held((id, rule), _) => format!("held {} {}", id.0, rule.0),
            })
            .collect()
    }

    #[test]
    fn the_runtime_is_attached_while_any_app_runs() {
        let mut model = Model::default();
        assert_eq!(model.state(), State::Idle);
        model.apply(started("a", 1));
        assert_eq!(model.state(), State::Attached);
        model.apply(started("b", 2));
        model.apply(exited("a", 1));
        assert_eq!(model.state(), State::Attached);
        model.apply(exited("b", 2));
        assert_eq!(model.state(), State::Idle);
    }

    #[test]
    fn a_failing_scan_degrades_the_runtime_until_it_recovers() {
        let mut model = Model::default();
        model.apply(started("a", 1));
        model.apply(Change::ScanFailed("denied".to_owned()));
        assert_eq!(model.state(), State::Degraded);
        model.apply(exited("a", 1));
        assert_eq!(model.state(), State::Degraded, "degraded overrides idle");
        model.apply(Change::ScanRestored);
        assert_eq!(model.state(), State::Idle);
    }

    #[test]
    fn an_attached_app_with_automations_is_read_and_its_automations_fire() {
        let mut model = Model::default();
        let now = Instant::now();
        let a = plan("A", 90.0);
        assert_eq!(model.replan(plans(&[("a", &a)])), []);
        let read = model.apply(started("a", 7));
        assert_eq!(kinds(&read), ["read 0 A"]);
        let Some(Order::Read { watched, .. }) = read.first() else {
            panic!("a read order");
        };
        assert_eq!(
            *watched,
            Watched {
                process: ProcessId::new(7),
                executable: "Example.exe".to_owned(),
            }
        );
        assert_eq!(model.active_automations(), ["Pause A"]);
        assert_eq!(model.app_name(SessionId(0)), Some("A"));
        assert_eq!(model.app_name(SessionId(9)), None);
        let fired = model.sample(SessionId(0), ReaderNumber(0), 0, number(95.0), now);
        assert_eq!(kinds(&fired), ["run 0 Pause A"]);
        let again = model.sample(SessionId(0), ReaderNumber(0), 0, number(96.0), now);
        assert_eq!(again, [], "it fires once until it rearms");
        let released = model.sample(SessionId(0), ReaderNumber(0), 0, number(10.0), now);
        assert_eq!(kinds(&released), ["undo 0 Pause A"]);
        let other = model.sample(SessionId(5), ReaderNumber(0), 0, number(95.0), now);
        assert_eq!(other, [], "a reading of another session is dropped");
        let nowhere = model.sample(SessionId(0), ReaderNumber(0), 1, number(95.0), now);
        assert_eq!(nowhere, [], "a reading of no value of the plan is dropped");
        model.sample(SessionId(0), ReaderNumber(0), 0, number(95.0), now);
        let ended = model.apply(exited("a", 7));
        assert_eq!(kinds(&ended), ["stop 0 A", "undo 0 Pause A"]);
        assert_eq!(model.active_automations(), Vec::<&str>::new());
    }

    #[test]
    fn values_out_of_sight_are_forgotten_but_their_automations_stay() {
        // Pause while progress is high or a flag is set.
        let flag = SignalName::new("flag").expect("valid");
        let either = Condition::any(vec![
            Condition::compare(signal(), Comparison::AtLeast, number(90.0)).expect("valid"),
            Condition::compare(flag.clone(), Comparison::Equals, Value::Flag(true)).expect("valid"),
        ])
        .expect("valid");
        let rule = Rule::new(either, pause(), Duration::ZERO, Restore::OnRelease).expect("valid");
        let base = plan("A", 90.0);
        let a = Arc::new(AppPlan {
            labels: Labels {
                values: Vec::new(),
                ..base.labels.clone()
            },
            runs: Runs {
                window: base.runs.window.clone(),
                values: vec![value(signal()), value(flag)],
                rules: vec![RulePlan {
                    automation: "Pause A".to_owned(),
                    rule,
                }],
                clicks: Arc::default(),
            },
        });
        // Where each value is among the values the reader reads.
        let (progress, flag) = (0, 1);
        let mut model = Model::default();
        let now = Instant::now();
        model.replan(plans(&[("a", &a)]));
        model.apply(started("a", 7));
        assert_eq!(
            model.sample(SessionId(0), ReaderNumber(0), flag, Value::Flag(false), now),
            []
        );
        let fired = model.sample(SessionId(0), ReaderNumber(0), progress, number(95.0), now);
        assert_eq!(kinds(&fired), ["run 0 Pause A"]);
        model.forget(SessionId(9), ReaderNumber(0));
        let other = model.sample(SessionId(0), ReaderNumber(0), progress, number(10.0), now);
        assert_eq!(
            kinds(&other),
            ["undo 0 Pause A"],
            "another session's forget"
        );
        let again = model.sample(SessionId(0), ReaderNumber(0), progress, number(95.0), now);
        assert_eq!(kinds(&again), ["run 0 Pause A"]);
        model.forget(SessionId(0), ReaderNumber(0));
        let low = model.sample(SessionId(0), ReaderNumber(0), progress, number(10.0), now);
        assert_eq!(
            low,
            [],
            "the forgotten flag might be set, so the pause is not undone yet"
        );
        let released = model.sample(SessionId(0), ReaderNumber(0), flag, Value::Flag(false), now);
        assert_eq!(kinds(&released), ["undo 0 Pause A"]);
    }

    #[test]
    fn an_automation_held_back_by_its_cooldown_runs_once_it_passes() {
        let condition =
            Condition::compare(signal(), Comparison::AtLeast, number(90.0)).expect("valid");
        let rule =
            Rule::new(condition, pause(), Duration::from_secs(10), Restore::Never).expect("valid");
        let base = plan("A", 90.0);
        let a = Arc::new(AppPlan {
            labels: base.labels.clone(),
            runs: Runs {
                window: base.runs.window.clone(),
                values: vec![value(signal())],
                rules: vec![RulePlan {
                    automation: "Pause A".to_owned(),
                    rule,
                }],
                clicks: Arc::default(),
            },
        });
        let mut model = Model::default();
        let now = Instant::now();
        let later = |seconds| {
            now.checked_add(Duration::from_secs(seconds))
                .expect("a test time fits in an Instant")
        };
        model.replan(plans(&[("a", &a), ("b", &plan("B", 90.0))]));
        model.apply(started("a", 7));
        model.apply(started("b", 8));
        assert_eq!(model.next_due(), None);
        model.sample(SessionId(0), ReaderNumber(0), 0, number(95.0), now);
        model.sample(SessionId(0), ReaderNumber(0), 0, number(10.0), later(1));
        assert_eq!(
            model.sample(SessionId(0), ReaderNumber(0), 0, number(95.0), later(2)),
            [Order::Held((SessionId(0), RuleIndex(0)), Some(later(2)))],
            "held, for the trace"
        );
        assert_eq!(model.next_due(), Some(later(10)));
        assert_eq!(model.recheck(later(9)), []);
        assert_eq!(kinds(&model.recheck(later(10))), ["run 0 Pause A"]);
        assert_eq!(model.next_due(), None);
    }

    #[test]
    fn an_app_without_automations_is_attached_but_not_read() {
        let mut model = Model::default();
        assert_eq!(model.apply(started("a", 7)), []);
        assert_eq!(model.state(), State::Attached);
    }

    #[test]
    fn the_run_order_carries_the_rules_actions() {
        let mut model = Model::default();
        let now = Instant::now();
        model.replan(plans(&[("a", &plan("A", 90.0))]));
        model.apply(started("a", 7));
        let orders = model.sample(SessionId(0), ReaderNumber(0), 0, number(95.0), now);
        assert_eq!(
            orders,
            [Order::Run {
                owner: (SessionId(0), RuleIndex(0)),
                automation: "Pause A".to_owned(),
                watched: Watched {
                    process: ProcessId::new(7),
                    executable: "Example.exe".to_owned(),
                },
                actions: pause(),
                clicks: Arc::default(),
                restore: Restore::OnRelease,
                cause: Some(now),
            }]
        );
    }

    #[test]
    fn a_rule_its_cooldown_holds_back_is_an_order_for_the_trace_only() {
        let mut model = Model::default();
        let now = Instant::now();
        model.replan(plans(&[(
            "a",
            &plan_with_cooldown("A", Duration::from_secs(60)),
        )]));
        model.apply(started("a", 7));
        assert_eq!(
            kinds(&model.sample(SessionId(0), ReaderNumber(0), 0, number(95.0), now)),
            ["run 0 Pause A"]
        );
        model.sample(SessionId(0), ReaderNumber(0), 0, number(10.0), now);
        let later = now + Duration::from_secs(1);
        assert_eq!(
            model.sample(SessionId(0), ReaderNumber(0), 0, number(95.0), later),
            [Order::Held((SessionId(0), RuleIndex(0)), Some(later))]
        );
    }

    #[test]
    fn turning_automations_off_undoes_them_and_on_reads_again() {
        let mut model = Model::default();
        let now = Instant::now();
        model.replan(plans(&[("a", &plan("A", 90.0))]));
        model.apply(started("a", 7));
        model.sample(SessionId(0), ReaderNumber(0), 0, number(95.0), now);
        assert_eq!(
            kinds(&model.set_enabled(false)),
            ["stop 0 A", "undo 0 Pause A"]
        );
        assert!(!model.enabled());
        assert_eq!(model.active_automations(), Vec::<&str>::new());
        model.apply(exited("a", 7));
        assert_eq!(model.apply(started("a", 8)), [], "nothing runs while off");
        assert_eq!(kinds(&model.set_enabled(true)), ["read 1 A"]);
        assert_eq!(model.set_enabled(true), [], "already on");
    }

    #[test]
    fn a_new_plan_restarts_only_the_apps_whose_part_changed() {
        let mut model = Model::default();
        let now = Instant::now();
        let a = plan("A", 90.0);
        model.replan(plans(&[("a", &a), ("b", &plan("B", 90.0))]));
        model.apply(started("a", 7));
        model.apply(started("b", 8));
        model.sample(SessionId(1), ReaderNumber(0), 0, number(95.0), now);
        let b = plan("B", 50.0);
        let orders = model.replan(plans(&[("a", &plan("A", 90.0)), ("b", &b)]));
        assert_eq!(kinds(&orders), ["stop 1 B", "undo 1 Pause B", "read 2 B"]);
        let orders = model.replan(plans(&[("b", &b)]));
        assert_eq!(
            kinds(&orders),
            ["stop 0 A"],
            "a has no automations any more"
        );
        assert_eq!(model.active_automations(), ["Pause B"]);
        let orders = model.replan(plans(&[("a", &a), ("b", &b)]));
        assert_eq!(kinds(&orders), ["read 3 A"], "a is still attached");
        assert_eq!(
            model.active_automations(),
            ["Pause A", "Pause B"],
            "listed app by app, whichever started first"
        );
    }

    #[test]
    fn a_new_read_interval_restarts_each_reader_in_its_session() {
        let mut model = Model::default();
        let now = Instant::now();
        model.replan(plans(&[("a", &plan("A", 90.0)), ("b", &plan("B", 90.0))]));
        model.apply(started("a", 7));
        model.sample(SessionId(0), ReaderNumber(0), 0, number(95.0), now);
        model.set_interval(ReadInterval::DEFAULT);
        assert_eq!(
            model.restart_readers(),
            [],
            "the same interval changes nothing"
        );
        let slower = ReadInterval::new(Duration::from_millis(100), Duration::from_secs(1))
            .expect("in order");
        model.set_interval(slower);
        let orders = model.restart_readers();
        assert_eq!(kinds(&orders), ["stop 0 A", "read 0 A"], "b is not running");
        let Some(Order::Read { interval, .. }) = orders.last() else {
            panic!("a read order");
        };
        assert_eq!(*interval, slower);
        assert_eq!(
            model.active_automations(),
            ["Pause A"],
            "the automation stays run"
        );
        assert_eq!(model.restart_readers(), [], "a moves to it once");
        let Some(Order::Read { interval, .. }) = model.apply(started("b", 8)).first().cloned()
        else {
            panic!("b is read");
        };
        assert_eq!(interval, slower, "a new reader reads at the new interval");
    }

    #[test]
    fn a_new_read_interval_forgets_the_values_read_before() {
        let mut model = Model::default();
        let now = Instant::now();
        model.replan(plans(&[(
            "a",
            &plan_with_cooldown("A", Duration::from_secs(60)),
        )]));
        model.apply(started("a", 7));
        model.sample(SessionId(0), ReaderNumber(0), 0, number(95.0), now);
        model.sample(SessionId(0), ReaderNumber(0), 0, number(10.0), now);
        model.sample(SessionId(0), ReaderNumber(0), 0, number(95.0), now);
        assert!(model.next_due().is_some(), "held back by its cooldown");
        let slower = ReadInterval::new(Duration::from_millis(100), Duration::from_secs(1))
            .expect("in order");
        model.set_interval(slower);
        model.restart_readers();
        assert_eq!(model.next_due(), None, "nothing holds on unseen values");
        assert_eq!(model.recheck(now + Duration::from_secs(61)), []);
    }

    #[test]
    fn a_reload_that_changes_the_plan_and_the_interval_restarts_a_reader_once() {
        let mut model = Model::default();
        model.replan(plans(&[("a", &plan("A", 90.0)), ("b", &plan("B", 90.0))]));
        model.apply(started("a", 7));
        model.apply(started("b", 8));
        let slower = ReadInterval::new(Duration::from_millis(100), Duration::from_secs(1))
            .expect("in order");
        // As the owner does on a reload: the interval, then the plan, then
        // the readers the plan left running.
        model.set_interval(slower);
        let mut orders = model.replan(plans(&[("a", &plan("A", 80.0)), ("b", &plan("B", 90.0))]));
        orders.extend(model.restart_readers());
        assert_eq!(
            kinds(&orders),
            ["stop 0 A", "read 2 A", "stop 1 B", "read 1 B"],
            "a starts again for its plan, b for the interval"
        );
        assert!(
            orders.iter().all(|order| match order {
                Order::Read { interval, .. } => *interval == slower,
                _ => true,
            }),
            "both read at the new interval"
        );
    }

    /// `now` and `millis` more.
    fn after(now: Instant, millis: u64) -> Instant {
        now.checked_add(Duration::from_millis(millis))
            .expect("a test time fits in an Instant")
    }

    /// The reader each read order starts, and the interval it reads at.
    fn readers(orders: &[Order]) -> Vec<(SessionId, ReaderNumber, ReadInterval)> {
        orders
            .iter()
            .filter_map(|order| match order {
                Order::Read {
                    session,
                    reader,
                    interval,
                    ..
                } => Some((*session, *reader, *interval)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_reader_that_ended_starts_again_in_its_session_after_a_wait() {
        let mut model = Model::default();
        let now = Instant::now();
        model.replan(plans(&[(
            "a",
            &plan_with_cooldown("A", Duration::from_millis(500)),
        )]));
        let read = model.apply(started("a", 7));
        assert_eq!(
            readers(&read),
            [(SessionId(0), ReaderNumber(0), ReadInterval::DEFAULT)]
        );
        model.sample(SessionId(0), ReaderNumber(0), 0, number(95.0), now);
        model.sample(SessionId(0), ReaderNumber(0), 0, number(10.0), now);
        model.sample(SessionId(0), ReaderNumber(0), 0, number(95.0), now);
        assert_eq!(model.next_due(), Some(after(now, 500)), "held back");

        let ended = model.reader_ended(SessionId(0), ReaderNumber(0), now);
        assert_eq!(
            kinds(&ended),
            ["stop 0 A"],
            "the reader that ended is joined"
        );
        assert_eq!(model.active_automations(), ["Pause A"], "the session runs");
        assert_eq!(model.next_due(), Some(after(now, 1000)));
        assert_eq!(
            model.recheck(after(now, 999)),
            [],
            "the wait has not passed, and nothing fires on values nothing reads"
        );
        let again = model.recheck(after(now, 1000));
        assert_eq!(kinds(&again), ["read 0 A"]);
        assert_eq!(
            readers(&again),
            [(SessionId(0), ReaderNumber(1), ReadInterval::DEFAULT)],
            "the same session's next reader"
        );
        assert_eq!(model.next_due(), None);
        assert_eq!(model.recheck(after(now, 5000)), [], "it starts once");
    }

    #[test]
    fn what_a_reader_that_ended_or_was_replaced_read_is_dropped() {
        let mut model = Model::default();
        let now = Instant::now();
        model.replan(plans(&[(
            "a",
            &plan_with_cooldown("A", Duration::from_millis(500)),
        )]));
        model.apply(started("a", 7));
        let (session, first, second) = (SessionId(0), ReaderNumber(0), ReaderNumber(1));
        model.sample(session, first, 0, number(95.0), now);
        model.sample(session, first, 0, number(10.0), now);
        model.reader_ended(session, first, now);
        // The capture of the reader that ended can still send what it read.
        assert_eq!(model.sample(session, first, 0, number(95.0), now), []);
        assert_eq!(
            model.next_due(),
            Some(after(now, 1000)),
            "only the next reader is due: no value was recorded to hold a rule back on"
        );
        assert!(!model.forget(session, first), "nor does it forget");
        assert_eq!(kinds(&model.recheck(after(now, 1000))), ["read 0 A"]);
        assert_eq!(
            model.sample(session, first, 0, number(95.0), now),
            [],
            "replaced by the session's next reader"
        );
        assert_eq!(model.next_due(), None);
        assert!(!model.forget(session, first));
        assert_eq!(
            kinds(&model.sample(session, second, 0, number(95.0), after(now, 1000))),
            ["run 0 Pause A"],
            "the reader that reads is heard"
        );
        assert!(model.forget(session, second));
    }

    #[test]
    fn a_reader_that_ends_after_it_ran_long_waits_as_the_first_did() {
        let mut model = Model::default();
        let now = Instant::now();
        model.replan(plans(&[("a", &plan("A", 90.0))]));
        model.apply(started("a", 7));
        model.reader_ended(SessionId(0), ReaderNumber(0), now);
        model.recheck(after(now, 1000));
        // It ran for a millisecond less than the longest wait.
        model.reader_ended(SessionId(0), ReaderNumber(1), after(now, 30_999));
        assert_eq!(model.next_due(), Some(after(now, 32_999)), "twice as long");
        model.recheck(after(now, 32_999));
        model.reader_ended(SessionId(0), ReaderNumber(2), after(now, 62_999));
        assert_eq!(
            model.next_due(),
            Some(after(now, 63_999)),
            "it ran for the longest wait, so the wait starts over"
        );
    }

    #[test]
    fn each_reader_of_a_session_that_ends_waits_twice_as_long_up_to_a_limit() {
        let mut model = Model::default();
        let now = Instant::now();
        model.replan(plans(&[("a", &plan("A", 90.0))]));
        model.apply(started("a", 7));
        let waits: Vec<Option<Instant>> = (0..7)
            .map(|reader| {
                model.reader_ended(SessionId(0), ReaderNumber(reader), now);
                let due = model.next_due();
                model.recheck(after(now, 30_000));
                due
            })
            .collect();
        let expected = [1000, 2000, 4000, 8000, 16_000, 30_000, 30_000];
        assert_eq!(waits, expected.map(|millis| Some(after(now, millis))));
    }

    #[test]
    fn a_reader_that_says_it_ended_is_dropped_unless_it_is_its_sessions_newest() {
        let mut model = Model::default();
        let now = Instant::now();
        model.replan(plans(&[("a", &plan("A", 90.0))]));
        model.apply(started("a", 7));
        let slower = ReadInterval::new(Duration::from_millis(100), Duration::from_secs(1))
            .expect("in order");
        model.set_interval(slower);
        assert_eq!(
            readers(&model.restart_readers()),
            [(SessionId(0), ReaderNumber(1), slower)]
        );
        assert_eq!(
            model.reader_ended(SessionId(0), ReaderNumber(0), now),
            [],
            "the reader the session stopped"
        );
        assert_eq!(
            model.reader_ended(SessionId(9), ReaderNumber(1), now),
            [],
            "no such session"
        );
        assert_eq!(model.next_due(), None, "no reader starts for either");
        assert_eq!(
            kinds(&model.reader_ended(SessionId(0), ReaderNumber(1), now)),
            ["stop 0 A"],
            "the reader that reads"
        );
        model.apply(exited("a", 7));
        assert_eq!(
            model.reader_ended(SessionId(0), ReaderNumber(1), now),
            [],
            "a session that ended"
        );
    }

    #[test]
    fn a_new_read_interval_while_a_reader_waits_to_start_again_starts_none_early() {
        let mut model = Model::default();
        let now = Instant::now();
        model.replan(plans(&[("a", &plan("A", 90.0))]));
        model.apply(started("a", 7));
        model.reader_ended(SessionId(0), ReaderNumber(0), now);
        let slower = ReadInterval::new(Duration::from_millis(100), Duration::from_secs(1))
            .expect("in order");
        model.set_interval(slower);
        assert_eq!(model.restart_readers(), [], "no reader runs to start over");
        assert_eq!(
            readers(&model.recheck(after(now, 1000))),
            [(SessionId(0), ReaderNumber(1), slower)],
            "the reader that starts after the wait reads at the new interval"
        );
        assert_eq!(model.restart_readers(), []);
    }

    #[test]
    fn renaming_restarts_nothing_and_the_new_names_show_at_once() {
        let mut model = Model::default();
        let now = Instant::now();
        model.replan(plans(&[("a", &plan("A", 90.0))]));
        model.apply(started("a", 7));
        model.sample(SessionId(0), ReaderNumber(0), 0, number(95.0), now);
        let mut renamed = plan("A", 90.0);
        let parts = Arc::get_mut(&mut renamed).expect("unshared");
        parts.labels.name = "Renamed".to_owned();
        parts.labels.values = vec!["Renamed progress".to_owned()];
        parts.runs.rules[0].automation = "Pause renamed".to_owned();
        assert_eq!(
            model.replan(plans(&[("a", &renamed)])),
            [],
            "the automation that fired stays fired"
        );
        assert_eq!(model.active_automations(), ["Pause renamed"]);
        assert_eq!(model.app_name(SessionId(0)), Some("Renamed"));
        let sessions: Vec<_> = model
            .sessions()
            .map(|(id, plan)| (id, plan.labels.name.as_str()))
            .collect();
        assert_eq!(
            sessions,
            [(SessionId(0), "Renamed")],
            "the trace's names too"
        );
        assert_eq!(
            kinds(&model.sample(SessionId(0), ReaderNumber(0), 0, number(50.0), now)),
            ["undo 0 Pause renamed"]
        );
    }

    #[test]
    fn a_new_checked_build_restarts_nothing_but_is_what_the_log_reads() {
        let mut model = Model::default();
        model.replan(plans(&[("a", &plan("A", 90.0))]));
        assert_eq!(kinds(&model.apply(started("a", 7))), ["read 0 A"]);
        let mut checked = plan("A", 90.0);
        let build = BuildDef {
            version: Some("1.0.0.0".to_owned()),
            size: 1,
            modified: 2,
        };
        Arc::get_mut(&mut checked).expect("unshared").labels.checked = Some(build.clone());
        assert_eq!(model.replan(plans(&[("a", &checked)])), []);
        assert_eq!(
            model
                .plan("a")
                .and_then(|plan| plan.labels.checked.as_ref()),
            Some(&build)
        );
    }

    #[test]
    fn states_read_as_lowercase_words() {
        let names = [State::Idle, State::Attached, State::Degraded].map(|state| state.to_string());
        assert_eq!(names, ["idle", "attached", "degraded"]);
    }
}
