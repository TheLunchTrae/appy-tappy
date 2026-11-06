//! The rule evaluator: a pure transition from signal and game events to
//! action requests.
//!
//! A [`Session`] holds one game session's rule state, from the game's start
//! to its exit. Each event goes in as a method call, and the [`Effect`]s that
//! come out say which rules fired and which were released. The evaluator does
//! no I/O and reads no clock: every event carries the time it happened.
//! `docs/rules.md` explains the model.
//!
//! See `docs/architecture.md` for this crate's boundaries.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use appytappy_config::rule::{Condition, GameEvent, Rearm, Restore, Rule, Trigger};
use appytappy_config::signal::{SignalName, Value};

/// The position of a rule in the list a [`Session`] started with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleIndex(pub usize);

/// What the runtime must do after an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// The rule fired: run its actions.
    Fire(RuleIndex),
    /// The rule was released: restore what its paired actions changed.
    Release(RuleIndex),
}

/// One rule's state within a session.
#[derive(Debug, Clone, Copy)]
struct RuleState {
    /// Whether a [`Trigger::Becomes`] rule may fire; other triggers ignore it.
    armed: bool,
    /// Whether the rule fired, restores on release, and was not released yet.
    active: bool,
    /// When the rule last fired.
    last_fired: Option<Instant>,
}

impl RuleState {
    const fn new() -> Self {
        Self {
            armed: true,
            active: false,
            last_fired: None,
        }
    }

    fn cooled(&self, rule: &Rule, at: Instant) -> bool {
        self.last_fired
            .is_none_or(|fired| at.saturating_duration_since(fired) >= rule.cooldown())
    }

    /// Records a fire at `at` and returns its effect.
    fn fire(&mut self, rule: &Rule, index: usize, at: Instant) -> Effect {
        self.armed = false;
        self.active = rule.restore() == Restore::OnRelease;
        self.last_fired = Some(at);
        Effect::Fire(RuleIndex(index))
    }

    /// Releases the rule if it is active.
    fn release(&mut self, index: usize, effects: &mut Vec<Effect>) {
        if self.active {
            self.active = false;
            effects.push(Effect::Release(RuleIndex(index)));
        }
    }
}

/// The rule state of one game session.
///
/// Every session starts with each rule armed and no signal values, so a rule
/// that fired in an earlier session can fire again.
#[derive(Debug)]
pub struct Session {
    rules: Arc<[Rule]>,
    states: Vec<RuleState>,
    values: BTreeMap<SignalName, Value>,
    focused: Option<bool>,
}

impl Session {
    /// Starts a session for a game that started at `at`, and fires its
    /// [`GameEvent::Started`] rules.
    #[must_use]
    pub fn start(rules: Arc<[Rule]>, at: Instant) -> (Self, Vec<Effect>) {
        let mut session = Self::resume(rules);
        let mut effects = Vec::new();
        session.fire_event_rules(GameEvent::Started, at, &mut effects);
        (session, effects)
    }

    /// Starts a session for a game that is already running, such as after a
    /// [`Session::stop`] for a config change. It fires nothing: the
    /// [`GameEvent::Started`] rules already ran when the game started.
    #[must_use]
    pub fn resume(rules: Arc<[Rule]>) -> Self {
        Self {
            states: vec![RuleState::new(); rules.len()],
            rules,
            values: BTreeMap::new(),
            focused: None,
        }
    }

    /// Records a new `value` of `signal`, read at `at`.
    ///
    /// Releases come before fires, so a restore never undoes a change that
    /// another rule makes on the same value.
    pub fn sample(&mut self, signal: SignalName, value: Value, at: Instant) -> Vec<Effect> {
        let previous = self.values.insert(signal.clone(), value);
        let changed = previous.is_some_and(|previous| previous != value);
        let mut effects = Vec::new();
        for (index, (rule, state)) in self.rules.iter().zip(&mut self.states).enumerate() {
            if let Trigger::Becomes { condition, rearm } = rule.trigger()
                && !state.armed
                && !condition.holds(&self.values)
            {
                state.armed = match rearm {
                    Rearm::WhenFalse => true,
                    Rearm::When(rearm) => rearm.holds(&self.values),
                    Rearm::Never => false,
                };
                if state.armed || matches!(rearm, Rearm::Never) {
                    state.release(index, &mut effects);
                }
            }
        }
        for (index, (rule, state)) in self.rules.iter().zip(&mut self.states).enumerate() {
            let fires = match rule.trigger() {
                Trigger::Becomes { condition, .. } => state.armed && condition.holds(&self.values),
                Trigger::Changes {
                    signal: watched,
                    guard,
                } => changed && *watched == signal && guard_holds(guard.as_ref(), &self.values),
                Trigger::On { .. } => false,
            };
            if fires && state.cooled(rule, at) {
                effects.push(state.fire(rule, index, at));
            }
        }
        effects
    }

    /// Records that the game gained (`true`) or lost (`false`) the keyboard
    /// focus at `at`. A report that repeats the current focus does nothing.
    pub fn focus(&mut self, focused: bool, at: Instant) -> Vec<Effect> {
        let mut effects = Vec::new();
        if self.focused == Some(focused) {
            return effects;
        }
        self.focused = Some(focused);
        let (event, opposite) = if focused {
            (GameEvent::Focused, GameEvent::Unfocused)
        } else {
            (GameEvent::Unfocused, GameEvent::Focused)
        };
        for (index, (rule, state)) in self.rules.iter().zip(&mut self.states).enumerate() {
            if matches!(rule.trigger(), Trigger::On { event, .. } if *event == opposite) {
                state.release(index, &mut effects);
            }
        }
        self.fire_event_rules(event, at, &mut effects);
        effects
    }

    /// Ends the session because the game exited: releases every active rule,
    /// then fires the [`GameEvent::Exited`] rules.
    #[must_use]
    pub fn end(mut self, at: Instant) -> Vec<Effect> {
        let mut effects = self.release_all();
        self.fire_event_rules(GameEvent::Exited, at, &mut effects);
        effects
    }

    /// Ends the session while the game keeps running, as a pause or a config
    /// change does: releases every active rule and fires nothing.
    #[must_use]
    pub fn stop(mut self) -> Vec<Effect> {
        self.release_all()
    }

    fn release_all(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        for (index, state) in self.states.iter_mut().enumerate() {
            state.release(index, &mut effects);
        }
        effects
    }

    fn fire_event_rules(&mut self, happened: GameEvent, at: Instant, effects: &mut Vec<Effect>) {
        for (index, (rule, state)) in self.rules.iter().zip(&mut self.states).enumerate() {
            if let Trigger::On { event, guard } = rule.trigger()
                && *event == happened
                && guard_holds(guard.as_ref(), &self.values)
                && state.cooled(rule, at)
            {
                effects.push(state.fire(rule, index, at));
            }
        }
    }
}

fn guard_holds(guard: Option<&Condition>, values: &BTreeMap<SignalName, Value>) -> bool {
    guard.is_none_or(|guard| guard.holds(values))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use appytappy_config::rule::Comparison;
    use appytappy_config::signal::Number;

    use super::*;

    fn name(raw: &str) -> SignalName {
        SignalName::new(raw).expect("valid name")
    }

    fn number(raw: f64) -> Value {
        Value::Number(Number::new(raw).expect("finite"))
    }

    fn percent(comparison: Comparison, threshold: f64) -> Condition {
        Condition::compare(name("percent"), comparison, number(threshold)).expect("valid")
    }

    fn becomes(rearm: Rearm, cooldown: Duration, restore: Restore) -> Rule {
        let condition = percent(Comparison::AtLeast, 90.0);
        Rule::new(Trigger::Becomes { condition, rearm }, cooldown, restore).expect("valid")
    }

    fn on(event: GameEvent, guard: Option<Condition>, restore: Restore) -> Rule {
        Rule::new(Trigger::On { event, guard }, Duration::ZERO, restore).expect("valid")
    }

    /// A clock for one test: `clock.at(ms)` is `ms` milliseconds after start.
    struct Clock(Instant);

    impl Clock {
        fn new() -> Self {
            Self(Instant::now())
        }

        fn at(&self, milliseconds: u64) -> Instant {
            self.0
                .checked_add(Duration::from_millis(milliseconds))
                .expect("a test time fits in an Instant")
        }
    }

    fn start(rules: Vec<Rule>, clock: &Clock) -> Session {
        let (session, effects) = Session::start(rules.into(), clock.at(0));
        assert_eq!(effects, []);
        session
    }

    /// Feeds `percent` values one millisecond apart from `from` and collects the effects.
    fn feed(session: &mut Session, clock: &Clock, from: u64, values: &[f64]) -> Vec<Effect> {
        let mut effects = Vec::new();
        for (offset, value) in (from..).zip(values) {
            effects.extend(session.sample(name("percent"), number(*value), clock.at(offset)));
        }
        effects
    }

    const FIRST: RuleIndex = RuleIndex(0);
    const SECOND: RuleIndex = RuleIndex(1);

    #[test]
    fn a_threshold_rule_fires_once_per_attempt_and_releases_on_death() {
        let clock = Clock::new();
        let mut session = start(
            vec![becomes(
                Rearm::WhenFalse,
                Duration::ZERO,
                Restore::OnRelease,
            )],
            &clock,
        );
        assert_eq!(feed(&mut session, &clock, 1, &[10.0, 50.0, 89.9]), []);
        assert_eq!(
            feed(&mut session, &clock, 4, &[90.0, 95.0, 100.0]),
            [Effect::Fire(FIRST)]
        );
        // The player dies: the percentage drops to zero, which releases and re-arms the rule.
        assert_eq!(
            feed(&mut session, &clock, 7, &[0.0, 0.0]),
            [Effect::Release(FIRST)]
        );
        assert_eq!(
            feed(&mut session, &clock, 9, &[45.0, 91.0]),
            [Effect::Fire(FIRST)]
        );
    }

    #[test]
    fn a_condition_that_holds_on_the_first_value_fires() {
        let clock = Clock::new();
        let mut session = start(
            vec![becomes(Rearm::WhenFalse, Duration::ZERO, Restore::Never)],
            &clock,
        );
        assert_eq!(
            feed(&mut session, &clock, 1, &[95.0]),
            [Effect::Fire(FIRST)]
        );
        // Without restore, nothing is released.
        assert_eq!(feed(&mut session, &clock, 2, &[0.0]), []);
        assert_eq!(session.end(clock.at(3)), []);
    }

    #[test]
    fn a_cooldown_holds_back_a_flapping_value() {
        let clock = Clock::new();
        let cooldown = Duration::from_millis(10);
        let mut session = start(
            vec![becomes(Rearm::WhenFalse, cooldown, Restore::Never)],
            &clock,
        );
        assert_eq!(
            feed(&mut session, &clock, 1, &[90.1, 89.9, 90.1, 89.9, 90.1]),
            [Effect::Fire(FIRST)]
        );
        // The rule stays armed and fires on the first holding value once the cooldown has passed.
        assert_eq!(
            session.sample(name("percent"), number(90.1), clock.at(10)),
            []
        );
        assert_eq!(
            session.sample(name("percent"), number(90.1), clock.at(11)),
            [Effect::Fire(FIRST)]
        );
    }

    #[test]
    fn a_rearm_condition_ignores_dips_above_it() {
        let clock = Clock::new();
        let rearm = Rearm::When(percent(Comparison::Below, 5.0));
        let mut session = start(
            vec![becomes(rearm, Duration::ZERO, Restore::OnRelease)],
            &clock,
        );
        // Dips above the re-arm threshold neither fire the rule again nor release it.
        assert_eq!(
            feed(&mut session, &clock, 1, &[90.0, 89.0, 90.0, 50.0, 95.0]),
            [Effect::Fire(FIRST)]
        );
        assert_eq!(
            feed(&mut session, &clock, 6, &[4.0, 92.0]),
            [Effect::Release(FIRST), Effect::Fire(FIRST)]
        );
    }

    #[test]
    fn a_rearm_condition_that_overlaps_the_trigger_waits_for_it_to_stop_holding() {
        let clock = Clock::new();
        let rearm = Rearm::When(percent(Comparison::Below, 95.0));
        let mut session = start(
            vec![becomes(rearm, Duration::ZERO, Restore::OnRelease)],
            &clock,
        );
        assert_eq!(
            feed(&mut session, &clock, 1, &[90.0, 92.0, 93.0]),
            [Effect::Fire(FIRST)]
        );
        assert_eq!(
            feed(&mut session, &clock, 4, &[80.0, 91.0]),
            [Effect::Release(FIRST), Effect::Fire(FIRST)]
        );
    }

    #[test]
    fn releases_come_before_fires_across_rules() {
        let clock = Clock::new();
        let rule = |comparison| {
            let condition = percent(comparison, 1.0);
            let trigger = Trigger::Becomes {
                condition,
                rearm: Rearm::WhenFalse,
            };
            Rule::new(trigger, Duration::ZERO, Restore::OnRelease).expect("valid")
        };
        let mut session = start(
            vec![rule(Comparison::AtLeast), rule(Comparison::Below)],
            &clock,
        );
        assert_eq!(
            feed(&mut session, &clock, 1, &[0.0]),
            [Effect::Fire(SECOND)]
        );
        // The second rule undoes its change before the first one makes its own.
        assert_eq!(
            feed(&mut session, &clock, 2, &[2.0]),
            [Effect::Release(SECOND), Effect::Fire(FIRST)]
        );
    }

    #[test]
    fn a_rule_that_never_rearms_fires_once_per_session() {
        let clock = Clock::new();
        let rules: Arc<[Rule]> = vec![becomes(Rearm::Never, Duration::ZERO, Restore::Never)].into();
        let (mut session, _) = Session::start(Arc::clone(&rules), clock.at(0));
        assert_eq!(
            feed(&mut session, &clock, 1, &[95.0, 0.0, 95.0]),
            [Effect::Fire(FIRST)]
        );
        assert_eq!(session.end(clock.at(4)), []);
        // A new session for the game's next start fires again.
        let (mut session, _) = Session::start(rules, clock.at(5));
        assert_eq!(
            feed(&mut session, &clock, 6, &[95.0]),
            [Effect::Fire(FIRST)]
        );
    }

    #[test]
    fn a_rule_that_never_rearms_is_released_when_its_condition_stops_holding() {
        let clock = Clock::new();
        let mut session = start(
            vec![becomes(Rearm::Never, Duration::ZERO, Restore::OnRelease)],
            &clock,
        );
        assert_eq!(
            feed(&mut session, &clock, 1, &[95.0, 0.0, 0.0, 95.0]),
            [Effect::Fire(FIRST), Effect::Release(FIRST)]
        );
    }

    #[test]
    fn a_change_rule_fires_on_new_values_of_its_signal() {
        let clock = Clock::new();
        let in_race = Condition::compare(name("in_race"), Comparison::Equals, Value::Flag(true))
            .expect("valid");
        let changes = |signal: &str, guard| {
            Rule::new(
                Trigger::Changes {
                    signal: name(signal),
                    guard,
                },
                Duration::ZERO,
                Restore::Never,
            )
            .expect("valid")
        };
        let mut session = start(
            vec![changes("lap", Some(in_race)), changes("percent", None)],
            &clock,
        );
        let mut lap = |value, at| session.sample(name("lap"), number(value), clock.at(at));
        // The first value is not a change, and the guard does not hold yet.
        assert_eq!(lap(1.0, 1), []);
        assert_eq!(lap(2.0, 2), []);
        assert_eq!(
            session.sample(name("in_race"), Value::Flag(true), clock.at(3)),
            []
        );
        let mut lap = |value, at| session.sample(name("lap"), number(value), clock.at(at));
        assert_eq!(lap(2.0, 4), []);
        assert_eq!(lap(3.0, 5), [Effect::Fire(FIRST)]);
        assert_eq!(
            feed(&mut session, &clock, 6, &[1.0, 1.0, 2.0]),
            [Effect::Fire(SECOND)]
        );
    }

    #[test]
    fn focus_rules_fire_on_focus_and_release_on_the_opposite_event() {
        let clock = Clock::new();
        let mut session = start(
            vec![
                on(GameEvent::Focused, None, Restore::OnRelease),
                on(GameEvent::Unfocused, None, Restore::Never),
            ],
            &clock,
        );
        assert_eq!(session.focus(true, clock.at(1)), [Effect::Fire(FIRST)]);
        assert_eq!(session.focus(true, clock.at(2)), []);
        assert_eq!(
            session.focus(false, clock.at(3)),
            [Effect::Release(FIRST), Effect::Fire(SECOND)]
        );
        assert_eq!(session.focus(false, clock.at(4)), []);
        assert_eq!(session.focus(true, clock.at(5)), [Effect::Fire(FIRST)]);
    }

    #[test]
    fn a_cooldown_applies_to_event_rules() {
        let clock = Clock::new();
        let rule = Rule::new(
            Trigger::On {
                event: GameEvent::Focused,
                guard: None,
            },
            Duration::from_millis(10),
            Restore::Never,
        )
        .expect("valid");
        let mut session = start(vec![rule], &clock);
        assert_eq!(session.focus(true, clock.at(1)), [Effect::Fire(FIRST)]);
        assert_eq!(session.focus(false, clock.at(2)), []);
        assert_eq!(session.focus(true, clock.at(3)), []);
        assert_eq!(session.focus(false, clock.at(4)), []);
        assert_eq!(session.focus(true, clock.at(11)), [Effect::Fire(FIRST)]);
    }

    #[test]
    fn start_and_exit_fire_their_rules_and_exit_releases_first() {
        let clock = Clock::new();
        let rules = vec![
            on(GameEvent::Started, None, Restore::OnRelease),
            becomes(Rearm::WhenFalse, Duration::ZERO, Restore::OnRelease),
            on(
                GameEvent::Exited,
                Some(percent(Comparison::AtLeast, 99.0)),
                Restore::Never,
            ),
            on(GameEvent::Exited, None, Restore::Never),
        ];
        let (mut session, effects) = Session::start(rules.into(), clock.at(0));
        assert_eq!(effects, [Effect::Fire(FIRST)]);
        assert_eq!(
            feed(&mut session, &clock, 1, &[95.0]),
            [Effect::Fire(SECOND)]
        );
        // The game exits mid-rule: both active rules are released before the exit rules run,
        // and the guarded exit rule sees the last value.
        assert_eq!(
            session.end(clock.at(2)),
            [
                Effect::Release(FIRST),
                Effect::Release(SECOND),
                Effect::Fire(RuleIndex(3)),
            ]
        );
    }

    #[test]
    fn stopping_releases_active_rules_without_firing_exit_rules() {
        let clock = Clock::new();
        let rules = vec![
            on(GameEvent::Started, None, Restore::OnRelease),
            on(GameEvent::Started, None, Restore::Never),
            on(GameEvent::Exited, None, Restore::Never),
        ];
        let rules: Arc<[Rule]> = rules.into();
        let (session, effects) = Session::start(Arc::clone(&rules), clock.at(0));
        assert_eq!(effects, [Effect::Fire(FIRST), Effect::Fire(SECOND)]);
        assert_eq!(session.stop(), [Effect::Release(FIRST)]);
        // Resuming for the running game does not run the start rules again.
        let session = Session::resume(rules);
        assert_eq!(session.end(clock.at(1)), [Effect::Fire(RuleIndex(2))]);
    }

    #[test]
    fn released_rules_are_not_released_again_at_exit() {
        let clock = Clock::new();
        let mut session = start(
            vec![becomes(
                Rearm::WhenFalse,
                Duration::ZERO,
                Restore::OnRelease,
            )],
            &clock,
        );
        assert_eq!(
            feed(&mut session, &clock, 1, &[95.0, 0.0]),
            [Effect::Fire(FIRST), Effect::Release(FIRST)]
        );
        assert_eq!(session.end(clock.at(3)), []);
    }
}
