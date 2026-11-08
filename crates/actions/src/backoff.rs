//! How long to wait before trying again something that keeps failing.

use std::time::{Duration, Instant};

/// The wait after the first failure; each later failure doubles it.
const FIRST_WAIT: Duration = Duration::from_secs(1);

/// The longest wait between two tries.
const LONGEST_WAIT: Duration = Duration::from_secs(60);

/// When to try again something that failed: a second after the first
/// failure, and twice as long after each later one, up to a minute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
    /// The earliest time to try again.
    retry_at: Instant,
    /// How long the next failure waits.
    wait: Duration,
}

impl Backoff {
    /// Starts waiting after the first failure, at `now`.
    #[must_use]
    pub fn new(now: Instant) -> Self {
        Self {
            retry_at: now.checked_add(FIRST_WAIT).unwrap_or(now),
            wait: FIRST_WAIT.saturating_mul(2).min(LONGEST_WAIT),
        }
    }

    /// Whether the wait is over at `now`.
    #[must_use]
    pub fn is_over(&self, now: Instant) -> bool {
        now >= self.retry_at
    }

    /// The earliest time to try again.
    #[must_use]
    pub const fn retry_at(&self) -> Instant {
        self.retry_at
    }

    /// Waits longer after another failure, at `now`.
    pub fn failed_again(&mut self, now: Instant) {
        self.retry_at = now.checked_add(self.wait).unwrap_or(now);
        self.wait = self.wait.saturating_mul(2).min(LONGEST_WAIT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wait_doubles_after_each_failure_up_to_a_minute() {
        let start = Instant::now();
        let at = |seconds: u64| start + Duration::from_secs(seconds);
        let mut backoff = Backoff::new(start);
        assert!(!backoff.is_over(start));
        assert!(backoff.is_over(at(1)), "the first wait is a second");
        let mut now = at(1);
        for wait in [2, 4, 8, 16, 32, 60, 60] {
            backoff.failed_again(now);
            assert!(!backoff.is_over(now + Duration::from_secs(wait - 1)));
            now += Duration::from_secs(wait);
            assert!(backoff.is_over(now), "after {wait}s");
        }
    }
}
