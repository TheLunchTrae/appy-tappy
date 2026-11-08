//! The root model on the owner thread: which apps are attached, what
//! problem, if any, stops the runtime from watching, whether automations
//! are enabled, and the rule session of each app whose automations run.
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

use crate::lifecycle::Change;
use crate::plan::AppPlan;
use crate::workers::act::Job;
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
    /// What each app with automations runs, by app ID.
    plan: BTreeMap<String, Arc<AppPlan>>,
    /// The apps whose automations run now, by app ID.
    running: BTreeMap<String, Running>,
    /// The next session's ID.
    next: u64,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            attached: BTreeMap::new(),
            problem: None,
            enabled: true,
            plan: BTreeMap::new(),
            running: BTreeMap::new(),
            next: 0,
        }
    }
}

impl Model {
    /// Whether rules may fire.
    pub(crate) const fn enabled(&self) -> bool {
        self.enabled
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
            .flat_map(|running| running.plan.automations.iter().map(String::as_str))
            .collect()
    }

    /// The name of the app a session reads, for the log.
    pub(crate) fn app_name(&self, id: SessionId) -> Option<&str> {
        self.running
            .values()
            .find(|running| running.id == id)
            .map(|running| running.plan.name.as_str())
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

    /// Switches to a new plan. An app whose part changed starts over with
    /// it, undoing what its automations changed; an app whose part did not
    /// change keeps running untouched.
    pub(crate) fn replan(&mut self, plan: BTreeMap<String, Arc<AppPlan>>) -> Vec<Order> {
        let changed: Vec<String> = self
            .running
            .iter()
            .filter(|(app, running)| plan.get(*app).is_none_or(|new| **new != *running.plan))
            .map(|(app, _)| app.clone())
            .collect();
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
        acts(id, &running.plan, &running.watched, effects)
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
        let rules = Arc::clone(&plan.rules);
        let (session, effects) = match start {
            Start::Fresh(at) => Session::start(rules, at),
            Start::Resume => (Session::resume(rules), Vec::new()),
        };
        let mut orders = vec![Order::Read(reader::Job {
            session: id,
            plan: Arc::clone(plan),
            executable: attached.executable.clone(),
        })];
        let watched = Watched {
            process: attached.process,
            executable: attached.executable.clone(),
        };
        orders.extend(acts(id, plan, &watched, effects));
        let plan = Arc::clone(plan);
        self.running.insert(
            app.to_owned(),
            Running {
                id,
                plan,
                session,
                watched,
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
        let name = running.plan.name.clone();
        let mut orders = vec![Order::StopReading(running.id, name)];
        orders.extend(acts(running.id, &running.plan, &running.watched, effects));
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

/// The action jobs for a session's `effects`.
fn acts(id: SessionId, plan: &AppPlan, watched: &Watched, effects: Vec<Effect>) -> Vec<Order> {
    effects
        .into_iter()
        .filter_map(|effect| {
            let (Effect::Fire(rule) | Effect::Release(rule)) = effect;
            let automation = plan.automations.get(rule.0)?.clone();
            let owner = (id, rule);
            let job = match effect {
                Effect::Fire(_) => {
                    let fired = plan.rules.get(rule.0)?;
                    Job::Run {
                        owner,
                        automation,
                        watched: watched.clone(),
                        actions: fired.actions().into(),
                        restore: fired.restore(),
                    }
                },
                Effect::Release(_) => Job::Undo { owner, automation },
            };
            Some(Order::Act(job))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use appytappy_config::action::{Action, MediaCommand, MediaTarget};
    use appytappy_config::anti_cheat::ProcessId;
    use appytappy_config::rule::{Comparison, Condition, Rearm, Restore, Rule, Trigger};
    use appytappy_config::signal::Number;
    use appytappy_config::window_match::{MustMatch, Pattern, WindowMatch};
    use appytappy_engine::RuleIndex;
    use appytappy_sources::screen::check::{BarFill, Check};
    use appytappy_sources::screen::saved::SavedCheck;
    use appytappy_sources::screen::{Region, Size};
    use appytappy_sources::system::Rgb;

    use super::*;
    use crate::plan::ValuePlan;

    fn signal() -> SignalName {
        SignalName::new("progress").expect("valid")
    }

    fn number(raw: f64) -> Value {
        Value::Number(Number::new(raw).expect("finite"))
    }

    fn pause() -> Box<[Action]> {
        Box::new([Action::Media {
            target: MediaTarget::Current,
            command: MediaCommand::Pause,
        }])
    }

    /// An app whose one automation pauses media while progress is at least
    /// `threshold`, and undoes it after.
    fn plan(name: &str, threshold: f64) -> Arc<AppPlan> {
        let condition =
            Condition::compare(signal(), Comparison::AtLeast, number(threshold)).expect("valid");
        let rule = Rule::new(
            Trigger::Becomes {
                condition,
                rearm: Rearm::WhenFalse,
            },
            pause(),
            Duration::ZERO,
            Restore::OnRelease,
        )
        .expect("valid");
        let black = Rgb {
            red: 0,
            green: 0,
            blue: 0,
        };
        let white = Rgb {
            red: 255,
            green: 255,
            blue: 255,
        };
        Arc::new(AppPlan {
            name: name.to_owned(),
            window: WindowMatch {
                title: Pattern::new("*"),
                class: Pattern::new("*"),
                executable: Pattern::new("Example.exe"),
                must_match: MustMatch {
                    title: false,
                    class: false,
                },
                fallback: false,
            },
            values: vec![ValuePlan {
                signal: signal(),
                name: "Progress".to_owned(),
                check: SavedCheck {
                    window_size: Size {
                        width: 800,
                        height: 600,
                    },
                    region: Region::new(0, 0, 10, 2).expect("not empty"),
                    check: Check::BarFill(BarFill::new(white, black).expect("different")),
                },
            }],
            rules: Arc::from(vec![rule]),
            automations: vec![format!("Pause {name}")],
        })
    }

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
                Order::Read(job) => format!("read {} {}", job.session.0, job.plan.name),
                Order::StopReading(id, name) => format!("stop {} {name}", id.0),
                Order::Act(Job::Run {
                    owner, automation, ..
                }) => format!("run {} {automation}", owner.0.0),
                Order::Act(Job::Undo { owner, automation }) => {
                    format!("undo {} {automation}", owner.0.0)
                },
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
            name: base.name.clone(),
            window: base.window.clone(),
            values: Vec::new(),
            rules: Arc::from(vec![rule]),
            automations: base.automations.clone(),
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
                restore: Restore::OnRelease,
            })]
        );
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
    fn states_read_as_lowercase_words() {
        let names = [State::Idle, State::Attached, State::Degraded].map(|state| state.to_string());
        assert_eq!(names, ["idle", "attached", "degraded"]);
    }
}
