//! Rules: when to act on a game's signals and events.
//!
//! These are validated types. The constructors check every invariant, so the
//! engine never meets a malformed rule. `docs/rules.md` explains the model.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::time::Duration;

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

/// A test of a game's current signal values.
///
/// A comparison on a signal that has no value yet, or whose value has a
/// different type than the comparison's value, does not hold.
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
        match &self.0 {
            Test::Compare {
                signal,
                comparison,
                value,
            } => values
                .get(signal)
                .is_some_and(|current| compares(*current, *comparison, *value)),
            Test::All(conditions) => conditions.iter().all(|condition| condition.holds(values)),
            Test::Any(conditions) => conditions.iter().any(|condition| condition.holds(values)),
        }
    }
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

/// Something that happens to a game as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameEvent {
    /// The game started; its first window appeared.
    Started,
    /// The game exited.
    Exited,
    /// A game window gained the keyboard focus.
    Focused,
    /// The game's windows lost the keyboard focus.
    Unfocused,
}

/// When a [`Trigger::Becomes`] rule may fire again after it fired.
#[derive(Debug, Clone, PartialEq)]
pub enum Rearm {
    /// Once its condition stops holding.
    WhenFalse,
    /// Once this other condition holds, such as a level percentage falling
    /// back near zero. It keeps a value that wavers around the trigger's
    /// threshold from firing the rule again.
    When(Condition),
    /// Not until the game starts again: once per game session.
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
    /// first value a game session reads is not a change.
    Changes {
        /// The signal to watch.
        signal: SignalName,
        /// A condition that must also hold, if any.
        guard: Option<Condition>,
    },
    /// Fires when `event` happens.
    On {
        /// The event to watch.
        event: GameEvent,
        /// A condition that must also hold, if any. At
        /// [`GameEvent::Started`] no signal has a value yet, so a guard
        /// there never holds.
        guard: Option<Condition>,
    },
}

/// Whether a rule's paired actions restore what they changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Restore {
    /// The changes stay.
    Never,
    /// The changes are undone when the rule is released: when a
    /// [`Trigger::Becomes`] condition stops holding, when the opposite focus
    /// event of a focus trigger happens, or when the game exits.
    OnRelease,
}

/// When to act: a trigger, a cooldown, and whether to restore.
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    trigger: Trigger,
    cooldown: Duration,
    restore: Restore,
}

impl Rule {
    /// Builds a rule. After it fires, it cannot fire again until `cooldown`
    /// has passed.
    ///
    /// # Errors
    ///
    /// Returns [`RuleError::NothingToRelease`] when `restore` is
    /// [`Restore::OnRelease`] for a trigger that is never released: a
    /// [`Trigger::Changes`] trigger or a [`GameEvent::Exited`] trigger.
    pub fn new(trigger: Trigger, cooldown: Duration, restore: Restore) -> Result<Self, RuleError> {
        let released = match &trigger {
            Trigger::Becomes { .. } => true,
            Trigger::Changes { .. } => false,
            Trigger::On { event, .. } => *event != GameEvent::Exited,
        };
        if restore == Restore::OnRelease && !released {
            return Err(RuleError::NothingToRelease);
        }
        Ok(Self {
            trigger,
            cooldown,
            restore,
        })
    }

    /// What makes the rule fire.
    #[must_use]
    pub const fn trigger(&self) -> &Trigger {
        &self.trigger
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
    /// The rule restores on release, but its trigger is never released.
    NothingToRelease,
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
            Self::NothingToRelease => formatter.write_str(
                "this trigger is never released, so its rule cannot restore what it changed",
            ),
        }
    }
}

impl Error for RuleError {}

#[cfg(test)]
mod tests {
    use crate::signal::Number;

    use super::*;

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

    fn percent(comparison: Comparison, threshold: f64) -> Condition {
        Condition::compare(name("percent"), comparison, number(threshold)).expect("valid")
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
            let condition = percent(comparison, 90.0);
            let held = [89.0, 90.0, 91.0]
                .map(|current| condition.holds(&values(&[("percent", number(current))])));
            assert_eq!(held, expected, "{comparison:?}");
        }
    }

    #[test]
    fn flags_compare_by_equality() {
        let in_level = Condition::compare(name("in_level"), Comparison::Equals, Value::Flag(true))
            .expect("valid");
        assert!(in_level.holds(&values(&[("in_level", Value::Flag(true))])));
        assert!(!in_level.holds(&values(&[("in_level", Value::Flag(false))])));
        assert_eq!(
            Condition::compare(name("in_level"), Comparison::Above, Value::Flag(false)),
            Err(RuleError::OrderedFlag(name("in_level")))
        );
        assert!(
            RuleError::OrderedFlag(name("in_level"))
                .to_string()
                .contains("in_level")
        );
    }

    #[test]
    fn missing_and_mistyped_values_do_not_hold() {
        let condition = percent(Comparison::NotEquals, 90.0);
        assert!(!condition.holds(&BTreeMap::new()));
        assert!(!condition.holds(&values(&[("percent", Value::Flag(true))])));
        assert!(!condition.holds(&values(&[("other", number(1.0))])));
    }

    #[test]
    fn groups_combine_conditions() {
        let low = percent(Comparison::Below, 10.0);
        let high = percent(Comparison::Above, 90.0);
        let all = Condition::all(vec![low.clone(), high.clone()]).expect("valid");
        let any = Condition::any(vec![low, high]).expect("valid");
        let at = |current| values(&[("percent", number(current))]);
        assert_eq!(
            [5.0, 50.0, 95.0].map(|current| any.holds(&at(current))),
            [true, false, true]
        );
        assert_eq!(
            [5.0, 50.0, 95.0].map(|current| all.holds(&at(current))),
            [false; 3]
        );
        let both = Condition::all(vec![
            percent(Comparison::Above, 10.0),
            percent(Comparison::Below, 90.0),
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
        let mut condition = percent(Comparison::Above, 1.0);
        for _ in 1..MAX_CONDITION_DEPTH {
            condition = Condition::any(vec![condition]).expect("within the limit");
        }
        assert_eq!(condition.depth(), MAX_CONDITION_DEPTH);
        assert_eq!(
            Condition::all(vec![condition.clone()]),
            Err(RuleError::TooDeep)
        );
        assert_eq!(
            Condition::any(vec![percent(Comparison::Above, 1.0), condition]),
            Err(RuleError::TooDeep)
        );
    }

    #[test]
    fn only_released_triggers_restore() {
        let becomes = Trigger::Becomes {
            condition: percent(Comparison::AtLeast, 90.0),
            rearm: Rearm::WhenFalse,
        };
        let changes = Trigger::Changes {
            signal: name("percent"),
            guard: None,
        };
        let on = |event| Trigger::On { event, guard: None };
        for trigger in [
            becomes,
            on(GameEvent::Started),
            on(GameEvent::Focused),
            on(GameEvent::Unfocused),
        ] {
            assert!(Rule::new(trigger, Duration::ZERO, Restore::OnRelease).is_ok());
        }
        for trigger in [changes, on(GameEvent::Exited)] {
            assert_eq!(
                Rule::new(trigger.clone(), Duration::ZERO, Restore::OnRelease),
                Err(RuleError::NothingToRelease)
            );
            assert!(Rule::new(trigger, Duration::ZERO, Restore::Never).is_ok());
        }
    }
}
