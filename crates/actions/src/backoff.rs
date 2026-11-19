//! How long to wait before trying again something that keeps failing, and
//! a thing made again after that wait until it can be made.

use std::io;
use std::time::{Duration, Instant};

/// The wait after the first failure; each later failure doubles it.
const FIRST_WAIT: Duration = Duration::from_secs(1);

/// The longest wait between two tries.
const LONGEST_WAIT: Duration = Duration::from_secs(60);

/// When to try again something that failed: a second after the first
/// failure, and twice as long after each later one, up to a minute or the
/// longest wait [`Backoff::up_to`] gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
    /// The earliest time to try again.
    retry_at: Instant,
    /// How long the next failure waits.
    wait: Duration,
    /// The longest wait between two tries.
    longest: Duration,
}

impl Backoff {
    /// Starts waiting after the first failure, at `now`.
    #[must_use]
    pub fn new(now: Instant) -> Self {
        Self::up_to(now, LONGEST_WAIT)
    }

    /// Starts waiting after the first failure, at `now`, waiting at most
    /// `longest` between two tries.
    #[must_use]
    pub fn up_to(now: Instant, longest: Duration) -> Self {
        let first = FIRST_WAIT.min(longest);
        Self {
            retry_at: now.checked_add(first).unwrap_or(now),
            wait: first.saturating_mul(2).min(longest),
            longest,
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
        self.wait = self.wait.saturating_mul(2).min(self.longest);
    }
}

/// Something made with a call that can fail, such as an operating system
/// service that is not ready yet.
///
/// It is either made, or the last error and when to make it again, after a
/// wait that grows with each failure.
#[derive(Debug)]
pub struct Retrying<T> {
    made: Result<T, Missing>,
}

/// Why a [`Retrying`] is not made yet, and when to make it again.
#[derive(Debug)]
struct Missing {
    /// The last error making it.
    error: String,
    /// When to make it again.
    backoff: Backoff,
}

impl<T> Retrying<T> {
    /// What the first call to make it, at `now`, gave.
    #[must_use]
    pub fn new(made: io::Result<T>, now: Instant) -> Self {
        Self {
            made: made.map_err(|error| Missing {
                error: error.to_string(),
                backoff: Backoff::new(now),
            }),
        }
    }

    /// Makes it again with `make` if it is missing and the wait is over at
    /// `now`. Answers whether this call made it.
    pub fn make_again(&mut self, now: Instant, make: impl FnOnce() -> io::Result<T>) -> bool {
        let Err(missing) = &mut self.made else {
            return false;
        };
        if !missing.backoff.is_over(now) {
            return false;
        }
        match make() {
            Ok(thing) => {
                self.made = Ok(thing);
                true
            },
            Err(error) => {
                missing.error = error.to_string();
                missing.backoff.failed_again(now);
                false
            },
        }
    }

    /// The thing, or the last error making it.
    ///
    /// # Errors
    ///
    /// Returns the last error making it, while it is missing.
    pub fn get(&self) -> Result<&T, &str> {
        self.made.as_ref().map_err(|missing| missing.error.as_str())
    }

    /// The thing, or the last error making it.
    ///
    /// # Errors
    ///
    /// Returns the last error making it, while it is missing.
    pub fn get_mut(&mut self) -> Result<&mut T, &str> {
        self.made.as_mut().map_err(|missing| missing.error.as_str())
    }

    /// When to make it again, while it is missing.
    #[must_use]
    pub fn retry_at(&self) -> Option<Instant> {
        self.made
            .as_ref()
            .err()
            .map(|missing| missing.backoff.retry_at())
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

    #[test]
    fn a_shorter_longest_wait_caps_the_doubling() {
        let start = Instant::now();
        let mut backoff = Backoff::up_to(start, Duration::from_secs(3));
        assert_eq!(backoff.retry_at(), start + Duration::from_secs(1));
        let mut now = start;
        for wait in [2, 3, 3] {
            now += Duration::from_secs(1);
            backoff.failed_again(now);
            assert_eq!(backoff.retry_at(), now + Duration::from_secs(wait));
        }
        let short = Backoff::up_to(start, Duration::from_millis(500));
        assert_eq!(short.retry_at(), start + Duration::from_millis(500));
    }

    #[test]
    fn a_missing_thing_is_made_again_only_once_its_wait_is_over() {
        let start = Instant::now();
        let at = |seconds: u64| start + Duration::from_secs(seconds);
        let mut made: Retrying<u8> = Retrying::new(Err(io::Error::other("not ready")), start);
        assert_eq!(made.get(), Err("not ready"));
        assert_eq!(made.retry_at(), Some(at(1)));
        let mut tries = 0;
        let mut fail = || {
            tries += 1;
            Err(io::Error::other("still not ready"))
        };
        assert!(!made.make_again(start, &mut fail), "the wait is not over");
        assert!(!made.make_again(at(1), &mut fail));
        assert_eq!(tries, 1);
        assert_eq!(made.get(), Err("still not ready"));
        assert_eq!(made.retry_at(), Some(at(3)), "the wait doubled");
        assert!(made.make_again(at(3), || Ok(7)));
        assert_eq!(made.get(), Ok(&7));
        assert_eq!(made.retry_at(), None);
        assert!(!made.make_again(at(9), || Ok(8)), "a made thing stays");
        assert_eq!(made.get_mut().map(|made| *made), Ok(7));
    }
}
