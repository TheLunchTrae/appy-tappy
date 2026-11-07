//! The local wall-clock time, for the runtime's log.

use std::fmt;

use crate::host;

/// A wall-clock time in the user's time zone, to the second.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    /// The year, such as 2026.
    pub year: u16,
    /// The month, from 1.
    pub month: u16,
    /// The day of the month, from 1.
    pub day: u16,
    /// The hour, from 0 to 23.
    pub hour: u16,
    /// The minute.
    pub minute: u16,
    /// The second.
    pub second: u16,
}

impl LocalTime {
    /// The current local time, or `None` where AppyTappy does not run.
    // Off Windows this body already returns `None`, so the mutation testing
    // there cannot tell a mutant from it.
    #[mutants::skip]
    #[must_use]
    pub fn now() -> Option<Self> {
        host::local_time()
    }
}

impl fmt::Display for LocalTime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_time_reads_year_first_with_padded_fields() {
        let time = LocalTime {
            year: 2026,
            month: 11,
            day: 7,
            hour: 3,
            minute: 5,
            second: 9,
        };
        assert_eq!(time.to_string(), "2026-11-07 03:05:09");
    }
}
