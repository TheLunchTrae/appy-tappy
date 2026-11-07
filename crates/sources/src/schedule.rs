//! How often a source reads: as often as the minimum interval allows while
//! its value moves, backing off toward the maximum while it holds still.
//!
//! Reading less often saves CPU; reading more often cuts the time between a
//! change on screen and the rule that acts on it. The interval bounds let the
//! user choose that trade per signal.

#![forbid(unsafe_code)]

use std::time::Duration;

/// The bounds on the time between two reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadInterval {
    min: Duration,
    max: Duration,
}

impl ReadInterval {
    /// The bounds used when a signal sets none: at most about 60 reads a
    /// second, and at least two a second.
    pub const DEFAULT: Self = Self {
        min: Duration::from_millis(16),
        max: Duration::from_millis(500),
    };

    /// Reads at most every `min` and at least every `max`, or `None` when
    /// `min` is longer than `max`.
    #[must_use]
    pub fn new(min: Duration, max: Duration) -> Option<Self> {
        (min <= max).then_some(Self { min, max })
    }

    /// The shortest time between two reads.
    #[must_use]
    pub const fn min(self) -> Duration {
        self.min
    }

    /// The longest time between two reads.
    #[must_use]
    pub const fn max(self) -> Duration {
        self.max
    }
}

impl Default for ReadInterval {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// What the last read showed about its signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    /// The value differs from the previous read.
    Changed,
    /// The value is close to where a rule would fire, so the next change
    /// matters.
    NearThreshold,
    /// The value is the same as the previous read and far from any rule.
    Steady,
}

/// The adaptive wait between reads within a [`ReadInterval`].
///
/// After a change, or near a rule's threshold, the wait drops to the
/// minimum. Each steady read doubles it, up to the maximum, so a value that
/// holds still for a while is read rarely and a moving one at full speed.
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
            wait: interval.min,
        }
    }

    /// The wait until the next read, given what the read just taken showed.
    pub fn after_read(&mut self, activity: Activity) -> Duration {
        self.wait = match activity {
            Activity::Changed | Activity::NearThreshold => self.interval.min,
            Activity::Steady => self
                .wait
                .saturating_mul(2)
                .max(FIRST_BACKOFF)
                .clamp(self.interval.min, self.interval.max),
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
    fn the_minimum_cannot_exceed_the_maximum() {
        assert_eq!(ReadInterval::new(millis(2), millis(1)), None);
        let interval = ReadInterval::new(millis(1), millis(1)).unwrap();
        assert_eq!((interval.min(), interval.max()), (millis(1), millis(1)));
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
            Activity::NearThreshold,
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
