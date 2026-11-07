//! Rules: when to act on an app's signals and events.
//!
//! These are validated types. The constructors check every invariant, so the
//! engine never meets a malformed rule. `docs/rules.md` explains the model.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::time::Duration;

use crate::action::Action;
use crate::signal::{SignalName, Value};

/// How deep conditions may nest; a single comparison has depth 1.
const MAX_CONDITION_DEPTH: usize = 8;

/// How a comparison relates a signal's value to the rule's value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparison {
    /// The signal is less than the value.
    Below,
    /// The signal is less than or equal to the value.
    AtMost,
    /// The signal equals the value.
    Equals,
    /// The signal differs from the value.
    NotEquals,
    /// The signal is greater than or equal to the value.
    AtLeast,
    /// The signal is greater than the value.
    Above,
}

impl Comparison {
    /// Whether the comparison needs an order, which flags do not have.
    const fn is_ordered(self) -> bool {
        matches!(
            self,
            Self::Below | Self::AtMost | Self::AtLeast | Self::Above
        )
    }
}

/// A test of an app's current signal values.
///
/// A comparison on a signal that has no value yet, or whose value has a
/// different type than the comparison's value, does not hold. A signal with
/// no value is also unknown: [`Condition::truth`] tells the two apart.
#[derive(Debug, Clone, PartialEq)]
pub struct Condition(Test);

#[derive(Debug, Clone, PartialEq)]
enum Test {
    Compare {
        signal: SignalName,
        comparison: Comparison,
        value: Value,
    },
    All(Box<[Condition]>),
    Any(Box<[Condition]>),
}

impl Condition {
    /// Compares `signal` with `value`.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::OrderedFlag`] when an ordered comparison, such as
    /// [`Comparison::Above`], meets a flag.
    pub fn compare(
        signal: SignalName,
        comparison: Comparison,
        value: Value,
    ) -> Result<Self, RuleError> {
        if comparison.is_ordered() && matches!(value, Value::Flag(_)) {
            return Err(RuleError::OrderedFlag(signal));
        }
        Ok(Self(Test::Compare {
            signal,
            comparison,
            value,
        }))
    }

    /// Holds when every condition in `conditions` holds.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::EmptyGroup`] for an empty list and
    /// [`RuleError::TooDeep`] when nesting passes the limit.
    pub fn all(conditions: Vec<Self>) -> Result<Self, RuleError> {
        Self::group(conditions).map(|group| Self(Test::All(group)))
    }

    /// Holds when at least one condition in `conditions` holds.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::EmptyGroup`] for an empty list and
    /// [`RuleError::TooDeep`] when nesting passes the limit.
    pub fn any(conditions: Vec<Self>) -> Result<Self, RuleError> {
        Self::group(conditions).map(|group| Self(Test::Any(group)))
    }

    fn group(conditions: Vec<Self>) -> Result<Box<[Self]>, RuleError> {
        let deepest = conditions
            .iter()
            .map(Self::depth)
            .max()
            .ok_or(RuleError::EmptyGroup)?;
        if deepest >= MAX_CONDITION_DEPTH {
            return Err(RuleError::TooDeep);
        }
        Ok(conditions.into_boxed_slice())
    }

    fn depth(&self) -> usize {
        match &self.0 {
            Test::Compare { .. } => 1,
            Test::All(conditions) | Test::Any(conditions) => conditions
                .iter()
                .map(Self::depth)
                .max()
                .unwrap_or_default()
                .saturating_add(1),
        }
    }

    /// Whether the condition holds for the current `values`.
    #[must_use]
    pub fn holds(&self, values: &BTreeMap<SignalName, Value>) -> bool {
        self.truth(values) == Some(true)
    }

    /// Whether the condition holds, does not, or cannot be told because a
    /// signal it needs has no value (`None`). An all group is false once
    /// one of its conditions is, and an any group is true once one of its
    /// conditions is, whatever the unknown ones would say.
    #[must_use]
    pub fn truth(&self, values: &BTreeMap<SignalName, Value>) -> Option<bool> {
        match &self.0 {
            Test::Compare {
                signal,
                comparison,
                value,
            } => values
                .get(signal)
                .map(|current| compares(*current, *comparison, *value)),
            Test::All(conditions) => decide(conditions, values, false),
            Test::Any(conditions) => decide(conditions, values, true),
        }
    }
}

/// A group's truth: `settles` as soon as one condition has it, otherwise
/// unknown if any condition is unknown, otherwise the opposite of `settles`.
fn decide(
    conditions: &[Condition],
    values: &BTreeMap<SignalName, Value>,
    settles: bool,
) -> Option<bool> {
    let mut unknown = false;
    for condition in conditions {
        match condition.truth(values) {
            Some(truth) if truth == settles => return Some(settles),
            Some(_) => {},
            None => unknown = true,
        }
    }
    (!unknown).then_some(!settles)
}

fn compares(current: Value, comparison: Comparison, expected: Value) -> bool {
    let order = match (current, expected) {
        (Value::Number(current), Value::Number(expected)) => current.partial_cmp(&expected),
        (Value::Flag(current), Value::Flag(expected)) => Some(current.cmp(&expected)),
        (Value::Number(_), Value::Flag(_)) | (Value::Flag(_), Value::Number(_)) => None,
    };
    order.is_some_and(|order| match comparison {
        Comparison::Below => order.is_lt(),
        Comparison::AtMost => order.is_le(),
        Comparison::Equals => order.is_eq(),
        Comparison::NotEquals => order.is_ne(),
        Comparison::AtLeast => order.is_ge(),
        Comparison::Above => order.is_gt(),
    })
}

/// Something that happens to an app as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppEvent {
    /// The app started; its first window appeared.
    Started,
    /// The app exited.
    Exited,
    /// An app window gained the keyboard focus.
    Focused,
    /// The app's windows lost the keyboard focus.
    Unfocused,
}

/// When a [`Trigger::Becomes`] rule may fire again after it fired.
///
/// A rule re-arms only while its own condition does not hold, so it never
/// fires twice for one stretch of holding. Re-arming also releases it.
#[derive(Debug, Clone, PartialEq)]
pub enum Rearm {
    /// Once its condition stops holding.
    WhenFalse,
    /// Once its condition stops holding and this other condition holds, such
    /// as a progress value falling back near zero. A value that wavers
    /// around the trigger's threshold then neither fires the rule again nor
    /// releases it.
    When(Condition),
    /// Not until the app starts again: once per app session. The rule is
    /// released when its condition stops holding.
    Never,
}

/// What makes a rule fire.
#[derive(Debug, Clone, PartialEq)]
pub enum Trigger {
    /// Fires when `condition` starts to hold, including on the first values
    /// that make it hold.
    Becomes {
        /// The condition to watch.
        condition: Condition,
        /// When the rule may fire again.
        rearm: Rearm,
    },
    /// Fires when a new value of `signal` differs from its last value. The
    /// first value an app session reads is not a change.
    Changes {
        /// The signal to watch.
        signal: SignalName,
        /// A condition that must also hold, if any.
        guard: Option<Condition>,
    },
    /// Fires when `event` happens.
    On {
        /// The event to watch.
        event: AppEvent,
        /// A condition that must also hold, if any. A
        /// [`AppEvent::Started`] trigger takes none, because no signal has
        /// a value yet.
        guard: Option<Condition>,
    },
}

/// Whether a rule's paired actions restore what they changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Restore {
    /// The changes stay.
    Never,
    /// The changes are undone when the rule is released: when a
    /// [`Trigger::Becomes`] rule re-arms (see [`Rearm`]), when the opposite
    /// focus event of a focus trigger happens, or when the session ends.
    OnRelease,
}

/// When to act and what to do: a trigger, the actions to run in order, a
/// cooldown, and whether to restore.
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    trigger: Trigger,
    actions: Box<[Action]>,
    cooldown: Duration,
    restore: Restore,
}

impl Rule {
    /// Builds a rule that runs `actions` in order when it fires. After it
    /// fires, it cannot fire again until `cooldown` has passed.
    ///
    /// # Errors
    ///
    /// - [`RuleError::NoActions`] when `actions` is empty.
    /// - [`RuleError::NothingToRestore`] when `restore` is
    ///   [`Restore::OnRelease`] and no action changes anything that can be put
    ///   back, such as a rule that only skips tracks.
    /// - [`RuleError::NothingToRelease`] when `restore` is
    ///   [`Restore::OnRelease`] for a trigger that is never released: a
    ///   [`Trigger::Changes`] trigger or an [`AppEvent::Exited`] trigger.
    /// - [`RuleError::GuardNeverHolds`] for a guard on a
    ///   [`AppEvent::Started`] trigger, because no signal has a value then.
    /// - [`RuleError::CooldownDropsFocus`] for a focus trigger that restores
    ///   and has a cooldown: a focus event during the cooldown is lost, so the
    ///   rule would stay released while the focus says it should be active.
    pub fn new(
        trigger: Trigger,
        actions: Box<[Action]>,
        cooldown: Duration,
        restore: Restore,
    ) -> Result<Self, RuleError> {
        if actions.is_empty() {
            return Err(RuleError::NoActions);
        }
        let restores = restore == Restore::OnRelease;
        if restores && !actions.iter().any(Action::restorable) {
            return Err(RuleError::NothingToRestore);
        }
        match &trigger {
            Trigger::Changes { .. }
            | Trigger::On {
                event: AppEvent::Exited,
                ..
            } if restores => return Err(RuleError::NothingToRelease),
            Trigger::On {
                event: AppEvent::Started,
                guard: Some(_),
            } => return Err(RuleError::GuardNeverHolds),
            Trigger::On {
                event: AppEvent::Focused | AppEvent::Unfocused,
                ..
            } if restores && !cooldown.is_zero() => return Err(RuleError::CooldownDropsFocus),
            Trigger::Becomes { .. } | Trigger::Changes { .. } | Trigger::On { .. } => {},
        }
        Ok(Self {
            trigger,
            actions,
            cooldown,
            restore,
        })
    }

    /// What makes the rule fire.
    #[must_use]
    pub const fn trigger(&self) -> &Trigger {
        &self.trigger
    }

    /// The actions to run, in order, when the rule fires.
    #[must_use]
    pub const fn actions(&self) -> &[Action] {
        &self.actions
    }

    /// How long after firing the rule cannot fire again.
    #[must_use]
    pub const fn cooldown(&self) -> Duration {
        self.cooldown
    }

    /// Whether the rule's paired actions restore what they changed.
    #[must_use]
    pub const fn restore(&self) -> Restore {
        self.restore
    }
}

/// A rule or condition broke an invariant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleError {
    /// An ordered comparison, such as "above", named a flag value for this
    /// signal.
    OrderedFlag(SignalName),
    /// An "all" or "any" group had no conditions.
    EmptyGroup,
    /// Conditions nested more deeply than the limit.
    TooDeep,
    /// The rule had no actions.
    NoActions,
    /// The rule restores on release, but none of its actions can be undone.
    NothingToRestore,
    /// The rule restores on release, but its trigger is never released.
    NothingToRelease,
    /// A start trigger had a guard, which can never hold.
    GuardNeverHolds,
    /// A focus trigger that restores had a cooldown, which would lose focus
    /// events.
    CooldownDropsFocus,
}

impl fmt::Display for RuleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OrderedFlag(signal) => write!(
                formatter,
                "a comparison on {signal} orders a yes-or-no value; use equals or not equals"
            ),
            Self::EmptyGroup => formatter.write_str("an all or any group needs a condition"),
            Self::TooDeep => write!(
                formatter,
                "conditions nest more than {MAX_CONDITION_DEPTH} levels deep"
            ),
            Self::NoActions => formatter.write_str("a rule needs an action"),
            Self::NothingToRestore => formatter.write_str(
                "none of this rule's actions can be undone, so it cannot restore what it changed",
            ),
            Self::NothingToRelease => formatter.write_str(
                "this trigger is never released, so its rule cannot restore what it changed",
            ),
            Self::GuardNeverHolds => formatter
                .write_str("a start trigger cannot have a guard, because no signal has a value yet"),
            Self::CooldownDropsFocus => formatter.write_str(
                "a focus rule that restores cannot have a cooldown, because it would lose focus events",
            ),
        }
    }
}

impl Error for RuleError {}

#[cfg(test)]
mod tests {
    use crate::signal::Number;

    use super::*;
    use crate::action::{MediaCommand, MediaTarget};

    fn pause() -> Box<[Action]> {
        media(MediaCommand::Pause)
    }

    fn media(command: MediaCommand) -> Box<[Action]> {
        Box::new([Action::Media {
            target: MediaTarget::Current,
            command,
        }])
    }

    fn name(raw: &str) -> SignalName {
        SignalName::new(raw).expect("valid name")
    }

    fn number(raw: f64) -> Value {
        Value::Number(Number::new(raw).expect("finite"))
    }

    fn values(entries: &[(&str, Value)]) -> BTreeMap<SignalName, Value> {
        entries
            .iter()
            .map(|(signal, value)| (name(signal), *value))
            .collect()
    }

    fn progress(comparison: Comparison, threshold: f64) -> Condition {
        Condition::compare(name("progress"), comparison, number(threshold)).expect("valid")
    }

    #[test]
    fn comparisons_relate_the_signal_to_the_value() {
        use Comparison::{Above, AtLeast, AtMost, Below, Equals, NotEquals};
        let expectations = [
            (Below, [true, false, false]),
            (AtMost, [true, true, false]),
            (Equals, [false, true, false]),
            (NotEquals, [true, false, true]),
            (AtLeast, [false, true, true]),
            (Above, [false, false, true]),
        ];
        for (comparison, expected) in expectations {
            let condition = progress(comparison, 90.0);
            let held = [89.0, 90.0, 91.0]
                .map(|current| condition.holds(&values(&[("progress", number(current))])));
            assert_eq!(held, expected, "{comparison:?}");
        }
    }

    #[test]
    fn flags_compare_by_equality() {
        let icon_visible =
            Condition::compare(name("icon_visible"), Comparison::Equals, Value::Flag(true))
                .expect("valid");
        assert!(icon_visible.holds(&values(&[("icon_visible", Value::Flag(true))])));
        assert!(!icon_visible.holds(&values(&[("icon_visible", Value::Flag(false))])));
        assert_eq!(
            Condition::compare(name("icon_visible"), Comparison::Above, Value::Flag(false)),
            Err(RuleError::OrderedFlag(name("icon_visible")))
        );
        assert!(
            RuleError::OrderedFlag(name("icon_visible"))
                .to_string()
                .contains("icon_visible")
        );
    }

    #[test]
    fn missing_and_mistyped_values_do_not_hold() {
        let condition = progress(Comparison::NotEquals, 90.0);
        assert!(!condition.holds(&BTreeMap::new()));
        assert!(!condition.holds(&values(&[("progress", Value::Flag(true))])));
        assert!(!condition.holds(&values(&[("other", number(1.0))])));
    }

    #[test]
    fn a_missing_value_is_unknown_unless_a_group_is_settled_without_it() {
        let low = progress(Comparison::Below, 10.0);
        let shown = Condition::compare(
            SignalName::new("icon").expect("valid"),
            Comparison::Equals,
            Value::Flag(true),
        )
        .expect("valid");
        let all = Condition::all(vec![low.clone(), shown.clone()]).expect("valid");
        let any = Condition::any(vec![low.clone(), shown]).expect("valid");
        let at = |current| values(&[("progress", number(current))]);
        assert_eq!(low.truth(&BTreeMap::new()), None);
        assert_eq!(
            low.truth(&values(&[("progress", Value::Flag(true))])),
            Some(false),
            "a value of the other type is known not to hold"
        );
        assert_eq!(
            [5.0, 50.0].map(|current| all.truth(&at(current))),
            [None, Some(false)]
        );
        assert_eq!(
            [5.0, 50.0].map(|current| any.truth(&at(current))),
            [Some(true), None]
        );
        let mut both = at(50.0);
        both.insert(SignalName::new("icon").expect("valid"), Value::Flag(false));
        assert_eq!(
            (all.truth(&both), any.truth(&both)),
            (Some(false), Some(false))
        );
        assert!(!any.holds(&at(50.0)), "unknown does not hold");
    }

    #[test]
    fn groups_combine_conditions() {
        let low = progress(Comparison::Below, 10.0);
        let high = progress(Comparison::Above, 90.0);
        let all = Condition::all(vec![low.clone(), high.clone()]).expect("valid");
        let any = Condition::any(vec![low, high]).expect("valid");
        let at = |current| values(&[("progress", number(current))]);
        assert_eq!(
            [5.0, 50.0, 95.0].map(|current| any.holds(&at(current))),
            [true, false, true]
        );
        assert_eq!(
            [5.0, 50.0, 95.0].map(|current| all.holds(&at(current))),
            [false; 3]
        );
        let both = Condition::all(vec![
            progress(Comparison::Above, 10.0),
            progress(Comparison::Below, 90.0),
        ])
        .expect("valid");
        assert_eq!(
            [5.0, 50.0, 95.0].map(|current| both.holds(&at(current))),
            [false, true, false]
        );
    }

    #[test]
    fn groups_need_conditions_and_limited_depth() {
        assert_eq!(Condition::all(Vec::new()), Err(RuleError::EmptyGroup));
        assert_eq!(Condition::any(Vec::new()), Err(RuleError::EmptyGroup));
        let mut condition = progress(Comparison::Above, 1.0);
        for _ in 1..MAX_CONDITION_DEPTH {
            condition = Condition::any(vec![condition]).expect("within the limit");
        }
        assert_eq!(condition.depth(), MAX_CONDITION_DEPTH);
        assert_eq!(
            Condition::all(vec![condition.clone()]),
            Err(RuleError::TooDeep)
        );
        assert_eq!(
            Condition::any(vec![progress(Comparison::Above, 1.0), condition]),
            Err(RuleError::TooDeep)
        );
    }

    #[test]
    fn only_released_triggers_restore() {
        let becomes = Trigger::Becomes {
            condition: progress(Comparison::AtLeast, 90.0),
            rearm: Rearm::WhenFalse,
        };
        let changes = Trigger::Changes {
            signal: name("progress"),
            guard: None,
        };
        let on = |event| Trigger::On { event, guard: None };
        for trigger in [
            becomes,
            on(AppEvent::Started),
            on(AppEvent::Focused),
            on(AppEvent::Unfocused),
        ] {
            assert!(Rule::new(trigger, pause(), Duration::ZERO, Restore::OnRelease).is_ok());
        }
        let guard = Some(progress(Comparison::AtLeast, 0.0));
        assert_eq!(
            Rule::new(
                Trigger::On {
                    event: AppEvent::Started,
                    guard: guard.clone()
                },
                pause(),
                Duration::ZERO,
                Restore::Never
            ),
            Err(RuleError::GuardNeverHolds)
        );
        assert!(
            Rule::new(
                Trigger::On {
                    event: AppEvent::Exited,
                    guard
                },
                pause(),
                Duration::ZERO,
                Restore::Never
            )
            .is_ok()
        );
        let second = Duration::from_secs(1);
        for event in [AppEvent::Focused, AppEvent::Unfocused] {
            assert_eq!(
                Rule::new(on(event), pause(), second, Restore::OnRelease),
                Err(RuleError::CooldownDropsFocus)
            );
            assert!(Rule::new(on(event), pause(), second, Restore::Never).is_ok());
        }
        assert!(Rule::new(on(AppEvent::Started), pause(), second, Restore::OnRelease).is_ok());
        for trigger in [changes, on(AppEvent::Exited)] {
            assert_eq!(
                Rule::new(trigger.clone(), pause(), Duration::ZERO, Restore::OnRelease),
                Err(RuleError::NothingToRelease)
            );
            assert!(Rule::new(trigger, pause(), Duration::ZERO, Restore::Never).is_ok());
        }
    }

    #[test]
    fn a_rule_needs_actions_and_something_to_restore() {
        let trigger = || Trigger::On {
            event: AppEvent::Focused,
            guard: None,
        };
        assert_eq!(
            Rule::new(trigger(), Box::new([]), Duration::ZERO, Restore::Never),
            Err(RuleError::NoActions)
        );
        for command in [MediaCommand::Next, MediaCommand::Previous] {
            assert_eq!(
                Rule::new(
                    trigger(),
                    media(command),
                    Duration::ZERO,
                    Restore::OnRelease
                ),
                Err(RuleError::NothingToRestore)
            );
            assert!(Rule::new(trigger(), media(command), Duration::ZERO, Restore::Never).is_ok());
        }
        let mut actions = media(MediaCommand::Next).into_vec();
        actions.extend(pause());
        let rule = Rule::new(
            trigger(),
            actions.clone().into(),
            Duration::ZERO,
            Restore::OnRelease,
        )
        .expect("valid");
        assert_eq!(rule.actions(), actions.as_slice());
        assert!(RuleError::NoActions.to_string().contains("needs an action"));
        assert!(RuleError::NothingToRestore.to_string().contains("undone"));
    }
}
