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

use std::time::{Duration, Instant};

/// The bounds on the time between two reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadInterval {
    min: Duration,
    max: Duration,
}

impl ReadInterval {
    /// The bounds used when a signal sets none: at most about 60 reads a
    /// second, and while the value holds still, a wait of at most half a
    /// second before the next frame is read.
    pub const DEFAULT: Self = Self {
        min: Duration::from_millis(16),
        max: Duration::from_millis(500),
    };

    /// Waits between reads of at least `min` and at most `max`, or `None`
    /// when `min` is longer than `max`.
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

/// The longest wait a [`ReadClock`] keeps; a longer one, such as
/// [`Duration::MAX`] to pause, is cut to this so the deadline stays
/// representable.
pub const LONGEST_WAIT: Duration = Duration::from_hours(24);

/// When a capture may read next: the deadline the last read's wait set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadClock {
    next: Instant,
}

impl ReadClock {
    /// A clock whose first read is due at once.
    #[must_use]
    pub const fn new(now: Instant) -> Self {
        Self { next: now }
    }

    /// Whether a read is due at `now`.
    #[must_use]
    pub fn due(self, now: Instant) -> bool {
        now >= self.next
    }

    /// How long from `now` until a read is due; zero once it is.
    #[must_use]
    pub fn remaining(self, now: Instant) -> Duration {
        self.next.saturating_duration_since(now)
    }

    /// Records a read that finished at `now` and asked to wait `wait`.
    pub fn after_read(&mut self, now: Instant, wait: Duration) {
        self.next = now.checked_add(wait.min(LONGEST_WAIT)).unwrap_or(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn millis(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    #[test]
    fn a_read_clock_waits_until_the_deadline() {
        let start = Instant::now();
        let mut clock = ReadClock::new(start);
        assert!(clock.due(start));
        assert_eq!(clock.remaining(start), Duration::ZERO);
        clock.after_read(start, millis(10));
        assert!(!clock.due(start));
        assert_eq!(clock.remaining(start), millis(10));
        let later = start.checked_add(millis(10)).unwrap();
        assert!(clock.due(later));
        assert_eq!(clock.remaining(later), Duration::ZERO);
    }

    #[test]
    fn a_read_clock_cuts_very_long_waits() {
        let start = Instant::now();
        let mut clock = ReadClock::new(start);
        clock.after_read(start, Duration::MAX);
        assert_eq!(clock.remaining(start), LONGEST_WAIT);
        assert!(!clock.due(start.checked_add(millis(1)).unwrap()));
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
