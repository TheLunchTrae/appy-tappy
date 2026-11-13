//! How often a source reads: as often as the minimum interval allows while
//! its value moves, backing off toward the maximum while it holds still.
//!
//! A screen capture reads only when the window has drawn a new frame, so a
//! window that does not change is not read at all, whatever the maximum.
//!
//! Reading less often saves CPU; reading more often cuts the time between a
//! change on screen and the rule that acts on it. The interval bounds, which
//! the user sets in the settings, choose that trade.

#![forbid(unsafe_code)]

use std::time::Duration;

use appytappy_config::settings_file::ReadInterval;

/// What the last read showed about its signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    /// The value differs from the previous read.
    Changed,
    /// The value is the same as the previous read.
    Steady,
}

/// The adaptive wait between reads within a [`ReadInterval`].
///
/// After a change, the wait drops to the minimum. Each steady read doubles
/// it, up to the maximum, so a value that holds still for a while is read
/// rarely and a moving one at full speed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadSchedule {
    interval: ReadInterval,
    wait: Duration,
}

/// The smallest wait a steady read backs off to, so doubling a zero
/// minimum still grows.
const FIRST_BACKOFF: Duration = Duration::from_millis(1);

impl ReadSchedule {
    /// A schedule that starts at the interval's minimum.
    #[must_use]
    pub const fn new(interval: ReadInterval) -> Self {
        Self {
            interval,
            wait: interval.min(),
        }
    }

    /// The wait until the next read, given what the read just taken showed.
    pub fn after_read(&mut self, activity: Activity) -> Duration {
        self.wait = match activity {
            Activity::Changed => self.interval.min(),
            Activity::Steady => self
                .wait
                .saturating_mul(2)
                .max(FIRST_BACKOFF)
                .clamp(self.interval.min(), self.interval.max()),
        };
        self.wait
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn millis(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    #[test]
    fn steady_reads_back_off_and_activity_resets_to_the_minimum() {
        let mut schedule = ReadSchedule::new(ReadInterval::new(millis(10), millis(50)).unwrap());
        let waits: Vec<_> = [
            Activity::Steady,
            Activity::Steady,
            Activity::Steady,
            Activity::Changed,
            Activity::Steady,
            Activity::Changed,
        ]
        .into_iter()
        .map(|activity| schedule.after_read(activity))
        .collect();
        assert_eq!(
            waits,
            [
                millis(20),
                millis(40),
                millis(50),
                millis(10),
                millis(20),
                millis(10)
            ]
        );
    }

    #[test]
    fn a_zero_minimum_still_backs_off() {
        let mut schedule = ReadSchedule::new(ReadInterval::new(Duration::ZERO, millis(3)).unwrap());
        assert_eq!(schedule.after_read(Activity::Changed), Duration::ZERO);
        assert_eq!(schedule.after_read(Activity::Steady), millis(1));
        assert_eq!(schedule.after_read(Activity::Steady), millis(2));
        assert_eq!(schedule.after_read(Activity::Steady), millis(3));
        let mut fixed =
            ReadSchedule::new(ReadInterval::new(Duration::ZERO, Duration::ZERO).unwrap());
        assert_eq!(fixed.after_read(Activity::Steady), Duration::ZERO);
    }
}
