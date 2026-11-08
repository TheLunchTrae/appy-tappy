//! The root model on the owner thread: which apps are attached, what
//! problem, if any, stops the runtime from watching, whether automations
//! are enabled, whether their actions can run, and the rule session of
//! each app whose automations run.
//!
//! The model does no I/O. Each change returns the [`Order`]s that carry it
//! out: start or stop a reader, and run or undo an automation's actions.
//! An app's automations run while it is attached, automations are enabled,
//! and the plan has automations for it.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::time::Instant;

use appytappy_actions::Watched;
use appytappy_config::anti_cheat::ProcessId;
use appytappy_config::signal::{SignalName, Value};
use appytappy_engine::{Effect, Session};
use appytappy_sources::schedule::ReadInterval;

use crate::lifecycle::Change;
use crate::plan::AppPlan;
use crate::trace::Trace;
use crate::workers::act::{Job, Owner};
use crate::workers::reader::{self, SessionId};

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
    Read(reader::Job),
    /// Stop the reader of a session, named by its app's name for the log.
    StopReading(SessionId, String),
    /// Run or undo an automation's actions.
    Act(Job),
    /// Nothing runs: an automation's cooldown held it back. The trace
    /// records it, with when the frame arrived whose reading it was.
    Held(Owner, Option<Instant>),
}

/// An attached app's process.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Attached {
    /// The process.
    process: ProcessId,
    /// Its executable, such as `Example.exe`.
    executable: String,
}

/// An app whose automations run.
#[derive(Debug)]
struct Running {
    id: SessionId,
    plan: Arc<AppPlan>,
    session: Session,
    /// The process the session follows.
    watched: Watched,
    /// How often its reader reads.
    interval: ReadInterval,
    /// The trace its reader records to.
    trace: Trace,
}

/// The owner thread's state.
#[derive(Debug)]
pub(crate) struct Model {
    /// The process each attached app follows, by app ID.
    attached: BTreeMap<String, Attached>,
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
    /// The apps whose automations run now, by app ID.
    running: BTreeMap<String, Running>,
    /// How often readers read, from the settings.
    interval: ReadInterval,
    /// The trace readers record to, from the settings.
    trace: Trace,
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
            trace: Trace::OFF,
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

    /// The names of the automations that run now, app by app.
    pub(crate) fn active_automations(&self) -> Vec<&str> {
        self.running
            .values()
            .flat_map(|running| running.plan.labels.automations.iter().map(String::as_str))
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
        self.running
            .values()
            .find(|running| running.id == id)
            .map(|running| &*running.plan)
    }

    /// Each running session with its application's newest plan, so the trace
    /// names what a reload renamed without starting it over.
    pub(crate) fn sessions(&self) -> impl Iterator<Item = (SessionId, &Arc<AppPlan>)> {
        self.running
            .values()
            .map(|running| (running.id, &running.plan))
    }

    /// Where `signal` is among the values that session `id` reads, for the
    /// trace.
    pub(crate) fn value_position(&self, id: SessionId, signal: &SignalName) -> Option<usize> {
        self.running
            .values()
            .find(|running| running.id == id)?
            .plan
            .runs
            .values
            .iter()
            .position(|value| value.signal == *signal)
    }

    /// Applies what detection found at `at`.
    pub(crate) fn apply(&mut self, change: Change, at: Instant) -> Vec<Order> {
        match change {
            Change::Started {
                app,
                process,
                executable,
            } => {
                self.attached.insert(
                    app.clone(),
                    Attached {
                        process,
                        executable,
                    },
                );
                self.start(&app, Start::Fresh(at))
            },
            Change::Exited { app, .. } => {
                self.attached.remove(&app);
                self.stop(&app, Stop::Exited(at))
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
        let apps: Vec<String> = if enabled {
            self.attached.keys().cloned().collect()
        } else {
            self.running.keys().cloned().collect()
        };
        apps.iter()
            .flat_map(|app| {
                if enabled {
                    self.start(app, Start::Resume)
                } else {
                    self.stop(app, Stop::Paused)
                }
            })
            .collect()
    }

    /// Switches to a new plan. An app whose part runs differently starts
    /// over with it, undoing what its automations changed. An app that runs
    /// the same keeps running untouched and only takes the new plan's labels,
    /// so renaming it or its parts, or marking its values as checked,
    /// restarts nothing.
    pub(crate) fn replan(&mut self, plan: BTreeMap<String, Arc<AppPlan>>) -> Vec<Order> {
        let mut changed = Vec::new();
        for (app, running) in &mut self.running {
            match plan.get(app) {
                Some(new) if new.runs == running.plan.runs => {
                    running.plan = Arc::clone(new);
                },
                _ => changed.push(app.clone()),
            }
        }
        let mut orders: Vec<Order> = changed
            .iter()
            .flat_map(|app| self.stop(app, Stop::Paused))
            .collect();
        self.plan = plan;
        let attached: Vec<String> = self.attached.keys().cloned().collect();
        for app in attached {
            orders.extend(self.start(&app, Start::Resume));
        }
        orders
    }

    /// How often readers read.
    pub(crate) const fn interval(&self) -> ReadInterval {
        self.interval
    }

    /// Sets how often readers read and the trace they record to. Readers
    /// that start from now on use them; [`Model::restart_readers`] moves the
    /// running ones to them.
    pub(crate) fn set_reading(&mut self, interval: ReadInterval, trace: Trace) {
        self.interval = interval;
        self.trace = trace;
    }

    /// The trace readers record to.
    pub(crate) const fn trace(&self) -> &Trace {
        &self.trace
    }

    /// Starts over each running app's reader that reads at another interval
    /// or records to another trace than the model's, in the same session, so
    /// its automations keep their state and nothing is undone. Its values are
    /// forgotten first, as when a reader can no longer see them, because the
    /// new reader sends each value as it first reads it. A reading the old
    /// reader sent just before it stopped can still arrive after that, a few
    /// milliseconds old.
    pub(crate) fn restart_readers(&mut self) -> Vec<Order> {
        let interval = self.interval;
        let trace = &self.trace;
        self.running
            .values_mut()
            .filter(|running| running.interval != interval || running.trace != *trace)
            .flat_map(|running| {
                running.interval = interval;
                running.trace = trace.clone();
                running.session.forget_values();
                [
                    Order::StopReading(running.id, running.plan.labels.name.clone()),
                    Order::Read(reader::Job {
                        session: running.id,
                        plan: Arc::clone(&running.plan),
                        process: running.watched.process,
                        executable: running.watched.executable.clone(),
                        interval,
                        trace: trace.clone(),
                    }),
                ]
            })
            .collect()
    }

    /// Records a value a session's reader read at `at`. A reading from a
    /// session that ended is dropped.
    pub(crate) fn sample(
        &mut self,
        id: SessionId,
        signal: SignalName,
        value: Value,
        at: Instant,
    ) -> Vec<Order> {
        let Some(running) = self.running.values_mut().find(|running| running.id == id) else {
            return Vec::new();
        };
        let effects = running.session.sample(signal, value, at);
        acts(id, &running.plan, &running.watched, effects, Some(at))
    }

    /// When a running automation that its cooldown held back is next due,
    /// across every app. See [`Model::recheck`].
    pub(crate) fn next_due(&self) -> Option<Instant> {
        self.running
            .values()
            .filter_map(|running| running.session.next_due())
            .min()
    }

    /// Fires, at `at`, the automations whose cooldown held them back while
    /// their condition held, and whose cooldown has passed.
    pub(crate) fn recheck(&mut self, at: Instant) -> Vec<Order> {
        self.running
            .values_mut()
            .flat_map(|running| {
                let effects = running.session.recheck(at);
                acts(running.id, &running.plan, &running.watched, effects, None)
            })
            .collect()
    }

    /// Forgets the values a session's reader can no longer see. Each rule
    /// keeps its state, so an automation that ran stays run, and nothing
    /// fires or is undone until the values read next decide its condition.
    pub(crate) fn forget(&mut self, id: SessionId) {
        if let Some(running) = self.running.values_mut().find(|running| running.id == id) {
            running.session.forget_values();
        }
    }

    /// Starts running `app`'s automations, if it is attached, automations
    /// are enabled, and the plan has automations for it.
    fn start(&mut self, app: &str, start: Start) -> Vec<Order> {
        let (Some(attached), Some(plan)) = (self.attached.get(app), self.plan.get(app)) else {
            return Vec::new();
        };
        if !self.enabled || self.running.contains_key(app) {
            return Vec::new();
        }
        let id = SessionId(self.next);
        self.next = self.next.wrapping_add(1);
        let rules = Arc::clone(&plan.runs.rules);
        let (session, effects) = match start {
            Start::Fresh(at) => Session::start(rules, at),
            Start::Resume => (Session::resume(rules), Vec::new()),
        };
        let mut orders = vec![Order::Read(reader::Job {
            session: id,
            plan: Arc::clone(plan),
            process: attached.process,
            executable: attached.executable.clone(),
            interval: self.interval,
            trace: self.trace.clone(),
        })];
        let watched = Watched {
            process: attached.process,
            executable: attached.executable.clone(),
        };
        orders.extend(acts(id, plan, &watched, effects, None));
        let plan = Arc::clone(plan);
        self.running.insert(
            app.to_owned(),
            Running {
                id,
                plan,
                session,
                watched,
                interval: self.interval,
                trace: self.trace.clone(),
            },
        );
        orders
    }

    /// Stops running `app`'s automations, undoing what they changed.
    fn stop(&mut self, app: &str, stop: Stop) -> Vec<Order> {
        let Some(running) = self.running.remove(app) else {
            return Vec::new();
        };
        let effects = match stop {
            Stop::Exited(at) => running.session.end(at),
            Stop::Paused => running.session.stop(),
        };
        let name = running.plan.labels.name.clone();
        let mut orders = vec![Order::StopReading(running.id, name)];
        orders.extend(acts(
            running.id,
            &running.plan,
            &running.watched,
            effects,
            None,
        ));
        orders
    }
}

/// How a session starts.
#[derive(Debug, Clone, Copy)]
enum Start {
    /// The app started at this time.
    Fresh(Instant),
    /// The app already ran: automations were turned back on, or its plan
    /// changed.
    Resume,
}

/// Why a session stops.
#[derive(Debug, Clone, Copy)]
enum Stop {
    /// The app exited at this time.
    Exited(Instant),
    /// The app keeps running: automations were turned off, or its plan
    /// changed.
    Paused,
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
            let (Effect::Fire(rule) | Effect::Release(rule) | Effect::Held(rule)) = effect;
            let owner = (id, rule);
            let job = match effect {
                Effect::Fire(_) => {
                    let fired = plan.runs.rules.get(rule.0)?;
                    Job::Run {
                        owner,
                        automation: plan.labels.automations.get(rule.0)?.clone(),
                        watched: watched.clone(),
                        actions: fired.actions().into(),
                        clicks: Arc::clone(&plan.runs.clicks),
                        restore: fired.restore(),
                        cause,
                    }
                },
                Effect::Release(_) => Job::Undo {
                    owner,
                    automation: plan.labels.automations.get(rule.0)?.clone(),
                    cause,
                },
                Effect::Held(_) => return Some(Order::Held(owner, cause)),
            };
            Some(Order::Act(job))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use appytappy_config::anti_cheat::ProcessId;
    use appytappy_config::app_file::BuildDef;
    use appytappy_config::rule::{Comparison, Condition, Rearm, Restore, Rule, Trigger};
    use appytappy_engine::RuleIndex;

    use super::*;
    use appytappy_config::settings_file::TraceCategory;

    use crate::plan::examples::{number, pause, plan, plan_with_cooldown, signal};
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
                Order::Read(job) => format!("read {} {}", job.session.0, job.plan.labels.name),
                Order::StopReading(id, name) => format!("stop {} {name}", id.0),
                Order::Act(Job::Run {
                    owner, automation, ..
                }) => format!("run {} {automation}", owner.0.0),
                Order::Act(Job::Undo {
                    owner, automation, ..
                }) => format!("undo {} {automation}", owner.0.0),
                Order::Act(Job::Trace(_)) => "trace".to_owned(),
                Order::Held((id, rule), _) => format!("held {} {}", id.0, rule.0),
            })
            .collect()
    }

    #[test]
    fn the_runtime_is_attached_while_any_app_runs() {
        let mut model = Model::default();
        let now = Instant::now();
        assert_eq!(model.state(), State::Idle);
        model.apply(started("a", 1), now);
        assert_eq!(model.state(), State::Attached);
        model.apply(started("b", 2), now);
        model.apply(exited("a", 1), now);
        assert_eq!(model.state(), State::Attached);
        model.apply(exited("b", 2), now);
        assert_eq!(model.state(), State::Idle);
    }

    #[test]
    fn a_failing_scan_degrades_the_runtime_until_it_recovers() {
        let mut model = Model::default();
        let now = Instant::now();
        model.apply(started("a", 1), now);
        model.apply(Change::ScanFailed("denied".to_owned()), now);
        assert_eq!(model.state(), State::Degraded);
        model.apply(exited("a", 1), now);
        assert_eq!(model.state(), State::Degraded, "degraded overrides idle");
        model.apply(Change::ScanRestored, now);
        assert_eq!(model.state(), State::Idle);
    }

    #[test]
    fn an_attached_app_with_automations_is_read_and_its_automations_fire() {
        let mut model = Model::default();
        let now = Instant::now();
        let a = plan("A", 90.0);
        assert_eq!(model.replan(plans(&[("a", &a)])), []);
        let read = model.apply(started("a", 7), now);
        assert_eq!(kinds(&read), ["read 0 A"]);
        let Some(Order::Read(job)) = read.first() else {
            panic!("a read order");
        };
        assert_eq!(job.executable, "Example.exe");
        assert_eq!(model.active_automations(), ["Pause A"]);
        assert_eq!(model.app_name(SessionId(0)), Some("A"));
        assert_eq!(model.app_name(SessionId(9)), None);
        let fired = model.sample(SessionId(0), signal(), number(95.0), now);
        assert_eq!(kinds(&fired), ["run 0 Pause A"]);
        let again = model.sample(SessionId(0), signal(), number(96.0), now);
        assert_eq!(again, [], "it fires once until it rearms");
        let released = model.sample(SessionId(0), signal(), number(10.0), now);
        assert_eq!(kinds(&released), ["undo 0 Pause A"]);
        let other = model.sample(SessionId(5), signal(), number(95.0), now);
        assert_eq!(other, [], "a reading of another session is dropped");
        model.sample(SessionId(0), signal(), number(95.0), now);
        let ended = model.apply(exited("a", 7), now);
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
        let rule = Rule::new(
            Trigger::Becomes {
                condition: either,
                rearm: Rearm::WhenFalse,
            },
            pause(),
            Duration::ZERO,
            Restore::OnRelease,
        )
        .expect("valid");
        let base = plan("A", 90.0);
        let a = Arc::new(AppPlan {
            labels: Labels {
                values: Vec::new(),
                ..base.labels.clone()
            },
            runs: Runs {
                window: base.runs.window.clone(),
                values: Vec::new(),
                rules: Arc::from(vec![rule]),
                clicks: Arc::default(),
            },
        });
        let mut model = Model::default();
        let now = Instant::now();
        model.replan(plans(&[("a", &a)]));
        model.apply(started("a", 7), now);
        assert_eq!(
            model.sample(SessionId(0), flag.clone(), Value::Flag(false), now),
            []
        );
        let fired = model.sample(SessionId(0), signal(), number(95.0), now);
        assert_eq!(kinds(&fired), ["run 0 Pause A"]);
        model.forget(SessionId(9));
        let other = model.sample(SessionId(0), signal(), number(10.0), now);
        assert_eq!(
            kinds(&other),
            ["undo 0 Pause A"],
            "another session's forget"
        );
        let again = model.sample(SessionId(0), signal(), number(95.0), now);
        assert_eq!(kinds(&again), ["run 0 Pause A"]);
        model.forget(SessionId(0));
        let low = model.sample(SessionId(0), signal(), number(10.0), now);
        assert_eq!(
            low,
            [],
            "the forgotten flag might be set, so the pause is not undone yet"
        );
        let released = model.sample(SessionId(0), flag, Value::Flag(false), now);
        assert_eq!(kinds(&released), ["undo 0 Pause A"]);
    }

    #[test]
    fn an_automation_held_back_by_its_cooldown_runs_once_it_passes() {
        let condition =
            Condition::compare(signal(), Comparison::AtLeast, number(90.0)).expect("valid");
        let rule = Rule::new(
            Trigger::Becomes {
                condition,
                rearm: Rearm::WhenFalse,
            },
            pause(),
            Duration::from_secs(10),
            Restore::Never,
        )
        .expect("valid");
        let base = plan("A", 90.0);
        let a = Arc::new(AppPlan {
            labels: Labels {
                values: Vec::new(),
                ..base.labels.clone()
            },
            runs: Runs {
                window: base.runs.window.clone(),
                values: Vec::new(),
                rules: Arc::from(vec![rule]),
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
        model.apply(started("a", 7), now);
        model.apply(started("b", 8), now);
        assert_eq!(model.next_due(), None);
        model.sample(SessionId(0), signal(), number(95.0), now);
        model.sample(SessionId(0), signal(), number(10.0), later(1));
        assert_eq!(
            model.sample(SessionId(0), signal(), number(95.0), later(2)),
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
        assert_eq!(model.apply(started("a", 7), Instant::now()), []);
        assert_eq!(model.state(), State::Attached);
    }

    #[test]
    fn the_run_order_carries_the_rules_actions() {
        let mut model = Model::default();
        let now = Instant::now();
        model.replan(plans(&[("a", &plan("A", 90.0))]));
        model.apply(started("a", 7), now);
        let orders = model.sample(SessionId(0), signal(), number(95.0), now);
        assert_eq!(
            orders,
            [Order::Act(Job::Run {
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
            })]
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
        model.apply(started("a", 7), now);
        assert_eq!(
            kinds(&model.sample(SessionId(0), signal(), number(95.0), now)),
            ["run 0 Pause A"]
        );
        model.sample(SessionId(0), signal(), number(10.0), now);
        let later = now + Duration::from_secs(1);
        assert_eq!(
            model.sample(SessionId(0), signal(), number(95.0), later),
            [Order::Held((SessionId(0), RuleIndex(0)), Some(later))]
        );
        assert_eq!(model.value_position(SessionId(0), &signal()), Some(0));
        assert_eq!(model.value_position(SessionId(9), &signal()), None);
    }

    #[test]
    fn turning_automations_off_undoes_them_and_on_reads_again() {
        let mut model = Model::default();
        let now = Instant::now();
        model.replan(plans(&[("a", &plan("A", 90.0))]));
        model.apply(started("a", 7), now);
        model.sample(SessionId(0), signal(), number(95.0), now);
        assert_eq!(
            kinds(&model.set_enabled(false)),
            ["stop 0 A", "undo 0 Pause A"]
        );
        assert!(!model.enabled());
        assert_eq!(model.active_automations(), Vec::<&str>::new());
        model.apply(exited("a", 7), now);
        assert_eq!(
            model.apply(started("a", 8), now),
            [],
            "nothing runs while off"
        );
        assert_eq!(kinds(&model.set_enabled(true)), ["read 1 A"]);
        assert_eq!(model.set_enabled(true), [], "already on");
    }

    #[test]
    fn a_new_plan_restarts_only_the_apps_whose_part_changed() {
        let mut model = Model::default();
        let now = Instant::now();
        let a = plan("A", 90.0);
        model.replan(plans(&[("a", &a), ("b", &plan("B", 90.0))]));
        model.apply(started("a", 7), now);
        model.apply(started("b", 8), now);
        model.sample(SessionId(1), signal(), number(95.0), now);
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
    }

    #[test]
    fn a_new_read_interval_restarts_each_reader_in_its_session() {
        let mut model = Model::default();
        let now = Instant::now();
        model.replan(plans(&[("a", &plan("A", 90.0)), ("b", &plan("B", 90.0))]));
        model.apply(started("a", 7), now);
        model.sample(SessionId(0), signal(), number(95.0), now);
        model.set_reading(ReadInterval::DEFAULT, Trace::OFF);
        assert_eq!(
            model.restart_readers(),
            [],
            "the same interval changes nothing"
        );
        let slower = ReadInterval::new(Duration::from_millis(100), Duration::from_secs(1))
            .expect("in order");
        model.set_reading(slower, Trace::OFF);
        let orders = model.restart_readers();
        assert_eq!(kinds(&orders), ["stop 0 A", "read 0 A"], "b is not running");
        let Some(Order::Read(job)) = orders.last() else {
            panic!("a read order");
        };
        assert_eq!(job.interval, slower);
        assert_eq!(
            model.active_automations(),
            ["Pause A"],
            "the automation stays run"
        );
        assert_eq!(model.restart_readers(), [], "a moves to it once");
        let Some(Order::Read(job)) = model.apply(started("b", 8), now).first().cloned() else {
            panic!("b is read");
        };
        assert_eq!(
            job.interval, slower,
            "a new reader reads at the new interval"
        );
    }

    #[test]
    fn a_new_read_interval_forgets_the_values_read_before() {
        let mut model = Model::default();
        let now = Instant::now();
        model.replan(plans(&[(
            "a",
            &plan_with_cooldown("A", Duration::from_secs(60)),
        )]));
        model.apply(started("a", 7), now);
        model.sample(SessionId(0), signal(), number(95.0), now);
        model.sample(SessionId(0), signal(), number(10.0), now);
        model.sample(SessionId(0), signal(), number(95.0), now);
        assert!(model.next_due().is_some(), "held back by its cooldown");
        let slower = ReadInterval::new(Duration::from_millis(100), Duration::from_secs(1))
            .expect("in order");
        model.set_reading(slower, Trace::OFF);
        model.restart_readers();
        assert_eq!(model.next_due(), None, "nothing holds on unseen values");
        assert_eq!(model.recheck(now + Duration::from_secs(61)), []);
    }

    #[test]
    fn a_reload_that_changes_the_plan_and_the_interval_restarts_a_reader_once() {
        let mut model = Model::default();
        let now = Instant::now();
        model.replan(plans(&[("a", &plan("A", 90.0)), ("b", &plan("B", 90.0))]));
        model.apply(started("a", 7), now);
        model.apply(started("b", 8), now);
        let slower = ReadInterval::new(Duration::from_millis(100), Duration::from_secs(1))
            .expect("in order");
        // As the owner does on a reload: the interval, then the plan, then
        // the readers the plan left running.
        model.set_reading(slower, Trace::OFF);
        let mut orders = model.replan(plans(&[("a", &plan("A", 80.0)), ("b", &plan("B", 90.0))]));
        orders.extend(model.restart_readers());
        assert_eq!(
            kinds(&orders),
            ["stop 0 A", "read 2 A", "stop 1 B", "read 1 B"],
            "a starts again for its plan, b for the interval"
        );
        assert!(
            orders.iter().all(|order| match order {
                Order::Read(job) => job.interval == slower,
                _ => true,
            }),
            "both read at the new interval"
        );
    }

    #[test]
    fn a_new_trace_restarts_each_reader_in_its_session() {
        let mut model = Model::default();
        let now = Instant::now();
        model.replan(plans(&[("a", &plan("A", 90.0))]));
        model.apply(started("a", 7), now);
        let (trace, _entries) = Trace::for_test(1, &[TraceCategory::Readings]);
        model.set_reading(ReadInterval::DEFAULT, trace.clone());
        let orders = model.restart_readers();
        assert_eq!(kinds(&orders), ["stop 0 A", "read 0 A"]);
        let Some(Order::Read(job)) = orders.last() else {
            panic!("a read order");
        };
        assert_eq!(job.trace, trace);
        model.set_reading(ReadInterval::DEFAULT, trace);
        assert_eq!(
            model.restart_readers(),
            [],
            "the same trace changes nothing"
        );
        model.set_reading(ReadInterval::DEFAULT, Trace::OFF);
        assert_eq!(kinds(&model.restart_readers()), ["stop 0 A", "read 0 A"]);
    }

    #[test]
    fn renaming_restarts_nothing_and_the_new_names_show_at_once() {
        let mut model = Model::default();
        let now = Instant::now();
        model.replan(plans(&[("a", &plan("A", 90.0))]));
        model.apply(started("a", 7), now);
        model.sample(SessionId(0), signal(), number(95.0), now);
        let mut renamed = plan("A", 90.0);
        let labels = &mut Arc::get_mut(&mut renamed).expect("unshared").labels;
        labels.name = "Renamed".to_owned();
        labels.automations = vec!["Pause renamed".to_owned()];
        labels.values = vec!["Renamed progress".to_owned()];
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
            kinds(&model.sample(SessionId(0), signal(), number(50.0), now)),
            ["undo 0 Pause renamed"]
        );
    }

    #[test]
    fn a_new_checked_build_restarts_nothing_but_is_what_the_log_reads() {
        let mut model = Model::default();
        let now = Instant::now();
        model.replan(plans(&[("a", &plan("A", 90.0))]));
        assert_eq!(kinds(&model.apply(started("a", 7), now)), ["read 0 A"]);
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
