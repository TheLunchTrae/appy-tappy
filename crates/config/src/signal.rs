//! Signals: the named values an app exposes, and the values sources read for
//! them.

use std::error::Error;
use std::fmt;
use std::sync::Arc;

/// The longest signal name, in bytes.
const MAX_NAME_LEN: usize = 64;

/// The name of a signal, such as `progress`.
///
/// A name starts with a lowercase ASCII letter, continues with lowercase ASCII
/// letters, digits, and underscores, and is at most 64 bytes long.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SignalName(Box<str>);

impl SignalName {
    /// Validates `name`.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidSignalName`] when `name` breaks the rules above.
    pub fn new(name: &str) -> Result<Self, InvalidSignalName> {
        let mut characters = name.chars();
        let starts_with_letter = characters
            .next()
            .is_some_and(|first| first.is_ascii_lowercase());
        let rest_allowed = characters.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        });
        if starts_with_letter && rest_allowed && name.len() <= MAX_NAME_LEN {
            Ok(Self(name.into()))
        } else {
            Err(InvalidSignalName(name.into()))
        }
    }
}

impl fmt::Display for SignalName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A signal name broke the naming rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidSignalName(Box<str>);

impl fmt::Display for InvalidSignalName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "signal name {:?} must start with a lowercase letter, use only lowercase letters, \
             digits, and underscores, and be at most {MAX_NAME_LEN} bytes",
            self.0
        )
    }
}

impl Error for InvalidSignalName {}

/// A finite number: never NaN or infinite.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Number(f64);

impl Number {
    /// Wraps `raw`, or returns `None` when it is NaN or infinite.
    #[must_use]
    pub fn new(raw: f64) -> Option<Self> {
        raw.is_finite().then_some(Self(raw))
    }

    /// The number, which is never NaN or infinite.
    #[must_use]
    pub const fn raw(self) -> f64 {
        self.0
    }
}

/// The longest state name, in characters.
pub const MAX_STATE_NAME_LEN: usize = 32;

/// What a state check reads when the region looks like none of its states.
pub const NO_STATE: &str = "none";

/// The name of a state a state check reads, such as `Paused`, or
/// [`NO_STATE`] when the region looks like none of them.
///
/// A name the user gives is not empty, has no space at either end and no
/// control character, is at most [`MAX_STATE_NAME_LEN`] characters, and is
/// not [`NO_STATE`] in any case. Cloning one shares the text, so reading the
/// same state again allocates nothing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StateName(Arc<str>);

impl StateName {
    /// Validates `name`, a state's name as the user gave it.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidStateName`] naming the rule `name` breaks.
    pub fn new(name: &str) -> Result<Self, InvalidStateName> {
        let problem = if name.trim().is_empty() {
            Some("is empty")
        } else if name.trim() != name {
            Some("starts or ends with a space")
        } else if name.chars().any(char::is_control) {
            Some("has a control character")
        } else if name.chars().count() > MAX_STATE_NAME_LEN {
            Some("is longer than 32 characters")
        } else if name.eq_ignore_ascii_case(NO_STATE) {
            Some("is \"none\", which stands for no state")
        } else {
            None
        };
        match problem {
            None => Ok(Self(name.into())),
            Some(problem) => Err(InvalidStateName {
                name: name.into(),
                problem,
            }),
        }
    }

    /// Validates `name`, the state an automation compares a value with:
    /// a state's name, or [`NO_STATE`] for none of them.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidStateName`] naming the rule `name` breaks, as
    /// [`Self::new`] does.
    pub fn target(name: &str) -> Result<Self, InvalidStateName> {
        if name == NO_STATE {
            Ok(Self::none())
        } else {
            Self::new(name)
        }
    }

    /// The name [`NO_STATE`], read when the region looks like none of the
    /// states.
    #[must_use]
    pub fn none() -> Self {
        Self(NO_STATE.into())
    }

    /// The name's text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for StateName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A state name broke the naming rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidStateName {
    name: Box<str>,
    problem: &'static str,
}

impl fmt::Display for InvalidStateName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "state name {:?} {}", self.name, self.problem)
    }
}

impl Error for InvalidStateName {}

/// A value of a signal, as a source read it or a rule names it.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A number, such as a score or how full a bar is.
    Number(Number),
    /// A yes-or-no value, such as whether a color is showing.
    Flag(bool),
    /// Which of a few named looks the region shows, such as `Paused`.
    State(StateName),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_names_follow_the_naming_rules() {
        let longest = "a".repeat(MAX_NAME_LEN);
        let too_long = "a".repeat(MAX_NAME_LEN.checked_add(1).expect("small"));
        for valid in ["progress", "a", "step2", "x_", longest.as_str()] {
            assert_eq!(
                SignalName::new(valid).map(|name| name.to_string()),
                Ok(valid.to_owned())
            );
        }
        for invalid in [
            "",
            "Level",
            "2step",
            "_step",
            "step-two",
            "step two",
            "läp",
            too_long.as_str(),
        ] {
            assert_eq!(
                SignalName::new(invalid),
                Err(InvalidSignalName(invalid.into())),
                "{invalid:?}"
            );
        }
        let error = SignalName::new("Level").expect_err("uppercase");
        assert!(error.to_string().contains("\"Level\""));
    }

    #[test]
    fn state_names_follow_the_naming_rules() {
        let longest = "é".repeat(MAX_STATE_NAME_LEN);
        let too_long = "a".repeat(MAX_STATE_NAME_LEN.checked_add(1).expect("small"));
        for valid in ["Paused", "a", "Game over", "Nonever", longest.as_str()] {
            assert_eq!(
                StateName::new(valid).map(|name| name.to_string()),
                Ok(valid.to_owned()),
                "{valid:?}"
            );
        }
        for (invalid, problem) in [
            ("", "is empty"),
            ("  ", "is empty"),
            (" Paused", "starts or ends"),
            ("Paused ", "starts or ends"),
            ("Pau\tsed", "control"),
            (too_long.as_str(), "longer than 32"),
            ("none", "stands for no state"),
            ("NONE", "stands for no state"),
        ] {
            let error = StateName::new(invalid).expect_err(invalid);
            assert!(error.to_string().contains(problem), "{invalid:?}: {error}");
            assert!(error.to_string().contains(&format!("{invalid:?}")));
        }
        assert_eq!(StateName::none().as_str(), NO_STATE);
        assert_eq!(StateName::none().to_string(), "none");
    }

    #[test]
    fn a_target_state_is_a_state_s_name_or_none() {
        assert_eq!(StateName::target(NO_STATE), Ok(StateName::none()));
        assert_eq!(
            StateName::target("Paused").map(|name| name.to_string()),
            Ok("Paused".to_owned())
        );
        for invalid in ["None", " "] {
            assert_eq!(
                StateName::target(invalid),
                StateName::new(invalid),
                "{invalid:?}"
            );
        }
    }

    #[test]
    fn numbers_are_finite() {
        assert_eq!(Number::new(90.5), Some(Number(90.5)));
        assert_eq!(Number::new(-2.0).map(Number::raw), Some(-2.0));
        for raw in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(Number::new(raw), None);
        }
    }
}
