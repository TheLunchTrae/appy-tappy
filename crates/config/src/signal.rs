//! Signals: the named values a game exposes, and the values sources read for
//! them.

use std::error::Error;
use std::fmt;

/// The longest signal name, in bytes.
const MAX_NAME_LEN: usize = 64;

/// The name of a signal, such as `level_percent`.
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

/// A value of a signal, as a source read it or a rule names it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    /// A number, such as a level percentage.
    Number(Number),
    /// A yes-or-no value, such as whether the player is in a level.
    Flag(bool),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_names_follow_the_naming_rules() {
        let longest = "a".repeat(MAX_NAME_LEN);
        let too_long = "a".repeat(MAX_NAME_LEN.checked_add(1).expect("small"));
        for valid in ["level_percent", "a", "lap2", "x_", longest.as_str()] {
            assert_eq!(
                SignalName::new(valid).map(|name| name.to_string()),
                Ok(valid.to_owned())
            );
        }
        for invalid in [
            "",
            "Level",
            "2lap",
            "_lap",
            "lap-two",
            "lap two",
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
    fn numbers_are_finite() {
        assert_eq!(Number::new(90.5), Some(Number(90.5)));
        assert_eq!(Number::new(-2.0).map(Number::raw), Some(-2.0));
        for raw in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(Number::new(raw), None);
        }
    }
}
