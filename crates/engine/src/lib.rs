//! The rule evaluator: a pure transition from signal values to action
//! requests.
//!
//! A [`Session`] holds one app session's rule state, from when its
//! automations start running until they stop. Each new value goes in as a
//! method call, and the [`Effect`]s that come out say which rules fired, which
//! were held back by their cooldown, and which were released. The evaluator
//! does no I/O and reads no clock: every value carries the time it was read.
//! `wiki/developer/Rules.md` explains the model.
//!
//! See `wiki/developer/Architecture.md` for this crate's boundaries.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use appytappy_config::rule::{Restore, Rule};
use appytappy_config::signal::{SignalName, Value};

/// The position of a rule in the list a [`Session`] started with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleIndex(pub usize);

/// What the runtime must do after a new value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// The rule fired: run its actions.
    Fire(RuleIndex),
    /// The rule was released: restore what its paired actions changed.
    Release(RuleIndex),
    /// The rule would have fired, but its cooldown held it back. Nothing
    /// runs; the runtime's trace records it.
    Held(RuleIndex),
}

/// One rule's state within a session.
#[derive(Debug, Clone, Copy)]
struct RuleState {
    /// Whether the rule may fire: it has not fired, or its condition stopped
    /// holding since.
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

    /// Whether the rule may fire now that its condition holds on `values`,
    /// apart from its cooldown.
    fn due(&self, rule: &Rule, values: &BTreeMap<SignalName, Value>) -> bool {
        self.armed && rule.condition().holds(values)
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

/// The rule state of one app session.
///
/// Every session starts with each rule armed and no signal values, so a rule
/// that fired in an earlier session can fire again.
#[derive(Debug)]
pub struct Session {
    rules: Arc<[Rule]>,
    states: Vec<RuleState>,
    values: BTreeMap<SignalName, Value>,
}

impl Session {
    /// Starts a session with every rule armed and no signal values.
    #[must_use]
    pub fn new(rules: Arc<[Rule]>) -> Self {
        Self {
            states: vec![RuleState::new(); rules.len()],
            rules,
            values: BTreeMap::new(),
        }
    }

    /// Records a new `value` of `signal`, read at `at`.
    ///
    /// Releases come before fires, so a restore never undoes a change that
    /// another rule makes on the same value.
    pub fn sample(&mut self, signal: SignalName, value: Value, at: Instant) -> Vec<Effect> {
        self.values.insert(signal, value);
        let mut effects = Vec::new();
        for (index, (rule, state)) in self.rules.iter().zip(&mut self.states).enumerate() {
            // An unknown condition leaves a fired rule as it is, until the
            // values it needs are read again.
            if !state.armed && rule.condition().truth(&self.values) == Some(false) {
                state.armed = true;
                state.release(index, &mut effects);
            }
        }
        for (index, (rule, state)) in self.rules.iter().zip(&mut self.states).enumerate() {
            if state.due(rule, &self.values) {
                effects.push(if state.cooled(rule, at) {
                    state.fire(rule, index, at)
                } else {
                    Effect::Held(RuleIndex(index))
                });
            }
        }
        effects
    }

    /// When a rule that its cooldown held back is next due: its condition
    /// holds on the values recorded, and it fires at that time unless a new
    /// value changes that first. The runtime calls [`Session::recheck`] then,
    /// because a value that holds still sends no sample.
    #[must_use]
    pub fn next_due(&self) -> Option<Instant> {
        self.rules
            .iter()
            .zip(&self.states)
            .filter(|(rule, state)| state.due(rule, &self.values))
            .filter_map(|(rule, state)| state.last_fired?.checked_add(rule.cooldown()))
            .min()
    }

    /// Checks at `at`, with no new value, whether a rule that its cooldown
    /// held back fires now that the cooldown passed.
    pub fn recheck(&mut self, at: Instant) -> Vec<Effect> {
        let mut effects = Vec::new();
        for (index, (rule, state)) in self.rules.iter().zip(&mut self.states).enumerate() {
            if state.due(rule, &self.values) && state.cooled(rule, at) {
                effects.push(state.fire(rule, index, at));
            }
        }
        effects
    }

    /// Forgets every signal's value, as when the values can no longer be
    /// seen. Each rule keeps its state: a rule that fired stays fired, and
    /// keeps its cooldown, until the values read next decide its condition.
    pub fn forget_values(&mut self) {
        self.values.clear();
    }

    /// Ends the session, because the app exited or its automations stop
    /// running: releases every active rule.
    #[must_use]
    pub fn stop(mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        for (index, state) in self.states.iter_mut().enumerate() {
            state.release(index, &mut effects);
        }
        effects
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use appytappy_config::action::{Action, MediaCommand, MediaTarget};
    use appytappy_config::rule::{Comparison, Condition};
    use appytappy_config::signal::{Number, StateName};

    use super::*;

    fn pause() -> Box<[Action]> {
        Box::new([Action::Media {
            target: MediaTarget::Current,
            command: MediaCommand::Pause,
        }])
    }

    fn name(raw: &str) -> SignalName {
        SignalName::new(raw).expect("valid name")
    }

    fn number(raw: f64) -> Value {
        Value::Number(Number::new(raw).expect("finite"))
    }

    fn progress(comparison: Comparison, threshold: f64) -> Condition {
        Condition::compare(name("progress"), comparison, number(threshold)).expect("valid")
    }

    fn at_90(cooldown: Duration, restore: Restore) -> Rule {
        Rule::new(
            progress(Comparison::AtLeast, 90.0),
            pause(),
            cooldown,
            restore,
        )
        .expect("valid")
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

    fn start(rules: Vec<Rule>) -> Session {
        Session::new(rules.into())
    }

    /// Feeds `progress` values one millisecond apart from `from` and collects the effects.
    fn feed(session: &mut Session, clock: &Clock, from: u64, values: &[f64]) -> Vec<Effect> {
        let mut effects = Vec::new();
        for (offset, value) in (from..).zip(values) {
            effects.extend(session.sample(name("progress"), number(*value), clock.at(offset)));
        }
        effects
    }

    const FIRST: RuleIndex = RuleIndex(0);
    const SECOND: RuleIndex = RuleIndex(1);

    #[test]
    fn a_threshold_rule_fires_once_per_crossing_and_releases_when_the_value_resets() {
        let clock = Clock::new();
        let mut session = start(vec![at_90(Duration::ZERO, Restore::OnRelease)]);
        assert_eq!(feed(&mut session, &clock, 1, &[10.0, 50.0, 89.9]), []);
        assert_eq!(
            feed(&mut session, &clock, 4, &[90.0, 95.0, 100.0]),
            [Effect::Fire(FIRST)]
        );
        // The value resets to zero, which releases and re-arms the rule.
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
    fn forgotten_values_keep_each_rule_as_it_was_until_they_are_read_again() {
        let clock = Clock::new();
        let lobby = Condition::compare(name("lobby"), Comparison::Equals, Value::Flag(true))
            .expect("valid");
        let both = Condition::all(vec![progress(Comparison::AtLeast, 90.0), lobby]).expect("valid");
        let rule =
            Rule::new(both, pause(), Duration::from_secs(60), Restore::OnRelease).expect("valid");
        let mut session = start(vec![rule]);
        session.sample(name("lobby"), Value::Flag(true), clock.at(1));
        assert_eq!(
            feed(&mut session, &clock, 2, &[95.0]),
            [Effect::Fire(FIRST)]
        );
        session.forget_values();
        // One value back is not enough to tell, so nothing is released.
        assert_eq!(feed(&mut session, &clock, 3, &[95.0]), []);
        // Both back and still true: it neither releases nor fires again.
        assert_eq!(
            session.sample(name("lobby"), Value::Flag(true), clock.at(4)),
            []
        );
        session.forget_values();
        // A value back that settles the group as false releases it.
        assert_eq!(
            feed(&mut session, &clock, 5, &[10.0]),
            [Effect::Release(FIRST)]
        );
    }

    #[test]
    fn a_forgotten_value_cannot_release_an_any_group() {
        let clock = Clock::new();
        let lobby = Condition::compare(name("lobby"), Comparison::Equals, Value::Flag(true))
            .expect("valid");
        let either =
            Condition::any(vec![progress(Comparison::AtLeast, 90.0), lobby]).expect("valid");
        let rule = Rule::new(either, pause(), Duration::ZERO, Restore::OnRelease).expect("valid");
        let mut session = start(vec![rule]);
        session.sample(name("lobby"), Value::Flag(false), clock.at(1));
        assert_eq!(
            feed(&mut session, &clock, 2, &[95.0]),
            [Effect::Fire(FIRST)]
        );
        session.forget_values();
        assert_eq!(
            feed(&mut session, &clock, 3, &[10.0]),
            [],
            "the lobby might be true now"
        );
        assert_eq!(
            session.sample(name("lobby"), Value::Flag(false), clock.at(4)),
            [Effect::Release(FIRST)]
        );
    }

    #[test]
    fn a_condition_that_holds_on_the_first_value_fires() {
        let clock = Clock::new();
        let mut session = start(vec![at_90(Duration::ZERO, Restore::Never)]);
        assert_eq!(
            feed(&mut session, &clock, 1, &[95.0]),
            [Effect::Fire(FIRST)]
        );
        // Without restore, nothing is released.
        assert_eq!(feed(&mut session, &clock, 2, &[0.0]), []);
        assert_eq!(session.stop(), []);
    }

    #[test]
    fn a_cooldown_holds_back_a_flapping_value() {
        let clock = Clock::new();
        let cooldown = Duration::from_millis(10);
        let mut session = start(vec![at_90(cooldown, Restore::Never)]);
        assert_eq!(
            feed(&mut session, &clock, 1, &[90.1, 89.9, 90.1, 89.9, 90.1]),
            [
                Effect::Fire(FIRST),
                Effect::Held(FIRST),
                Effect::Held(FIRST)
            ]
        );
        // The rule stays armed and fires on the first holding value once the cooldown has passed.
        assert_eq!(
            session.sample(name("progress"), number(90.1), clock.at(10)),
            [Effect::Held(FIRST)]
        );
        assert_eq!(
            session.sample(name("progress"), number(90.1), clock.at(11)),
            [Effect::Fire(FIRST)]
        );
    }

    #[test]
    fn a_rule_that_became_true_during_its_cooldown_fires_once_it_passes() {
        let clock = Clock::new();
        let cooldown = Duration::from_millis(10);
        let mut session = start(vec![at_90(cooldown, Restore::Never)]);
        assert_eq!(session.next_due(), None, "nothing fired yet");
        assert_eq!(
            feed(&mut session, &clock, 1, &[95.0, 10.0, 95.0]),
            [Effect::Fire(FIRST), Effect::Held(FIRST)]
        );
        // The value then holds still, so no sample comes.
        assert_eq!(session.next_due(), Some(clock.at(11)));
        assert_eq!(session.recheck(clock.at(10)), [], "still cooling down");
        assert_eq!(session.recheck(clock.at(11)), [Effect::Fire(FIRST)]);
        assert_eq!(session.next_due(), None, "it fired, so nothing is due");
        assert_eq!(session.recheck(clock.at(30)), []);
    }

    #[test]
    fn a_rule_whose_condition_stopped_holding_is_not_due() {
        let clock = Clock::new();
        let cooldown = Duration::from_millis(10);
        let mut session = start(vec![at_90(cooldown, Restore::Never)]);
        feed(&mut session, &clock, 1, &[95.0, 10.0, 95.0, 10.0]);
        assert_eq!(session.next_due(), None);
        assert_eq!(session.recheck(clock.at(20)), []);
        // Held again, then the value can no longer be seen: unknown is not
        // holding either.
        feed(&mut session, &clock, 21, &[95.0, 10.0, 95.0]);
        assert_eq!(session.next_due(), Some(clock.at(31)));
        session.forget_values();
        assert_eq!(session.next_due(), None);
        assert_eq!(session.recheck(clock.at(40)), []);
    }

    #[test]
    fn releases_come_before_fires_across_rules() {
        let clock = Clock::new();
        let rule = |comparison| {
            let condition = progress(comparison, 1.0);
            Rule::new(condition, pause(), Duration::ZERO, Restore::OnRelease).expect("valid")
        };
        let mut session = start(vec![rule(Comparison::AtLeast), rule(Comparison::Below)]);
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
    fn a_state_rule_fires_on_the_state_and_releases_on_another() {
        let clock = Clock::new();
        let state = |name: &str| Value::State(StateName::new(name).expect("valid"));
        let paused =
            Condition::compare(name("status"), Comparison::Equals, state("Paused")).expect("valid");
        let rule = Rule::new(paused, pause(), Duration::ZERO, Restore::OnRelease).expect("valid");
        let mut session = start(vec![rule]);
        let mut status = |value, at| session.sample(name("status"), value, clock.at(at));
        assert_eq!(status(state("Live"), 1), []);
        assert_eq!(status(state("Paused"), 2), [Effect::Fire(FIRST)]);
        assert_eq!(status(state("Paused"), 3), []);
        assert_eq!(
            status(Value::State(StateName::none()), 4),
            [Effect::Release(FIRST)]
        );
    }

    #[test]
    fn stopping_releases_only_the_active_rules_that_restore() {
        let clock = Clock::new();
        let mut session = start(vec![
            at_90(Duration::ZERO, Restore::OnRelease),
            at_90(Duration::ZERO, Restore::Never),
        ]);
        assert_eq!(
            feed(&mut session, &clock, 1, &[95.0]),
            [Effect::Fire(FIRST), Effect::Fire(SECOND)]
        );
        assert_eq!(session.stop(), [Effect::Release(FIRST)]);
    }

    #[test]
    fn released_rules_are_not_released_again_when_the_session_stops() {
        let clock = Clock::new();
        let mut session = start(vec![at_90(Duration::ZERO, Restore::OnRelease)]);
        assert_eq!(
            feed(&mut session, &clock, 1, &[95.0, 0.0]),
            [Effect::Fire(FIRST), Effect::Release(FIRST)]
        );
        assert_eq!(session.stop(), []);
    }
}
