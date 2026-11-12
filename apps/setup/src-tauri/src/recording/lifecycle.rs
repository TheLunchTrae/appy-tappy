//! When a recording records, whatever it keeps: from its start until it
//! stops, with why it skips frames or stopped by itself.

use std::time::{Duration, Instant};

/// When a recording records, and what is wrong with it, if anything.
///
/// It records from its start until it stops. Only a recording that can go
/// on resumes; the time it shows counts only recording.
#[derive(Debug)]
pub(crate) struct Lifecycle {
    /// When it started, moved later by each pause so the time it shows
    /// counts only recording.
    started: Instant,
    /// How long it recorded, once it stopped.
    stopped: Option<Duration>,
    /// Why the latest frame could not be recorded, until one can be, or why
    /// it stopped by itself.
    problem: Option<String>,
}

impl Lifecycle {
    /// Recording since `started`.
    pub(crate) const fn new(started: Instant) -> Self {
        Self {
            started,
            stopped: None,
            problem: None,
        }
    }

    /// Whether it still records.
    pub(crate) const fn is_recording(&self) -> bool {
        self.stopped.is_none()
    }

    /// How long it has recorded at `now`, or recorded in all once stopped.
    pub(crate) fn elapsed(&self, now: Instant) -> Duration {
        self.stopped
            .unwrap_or_else(|| now.saturating_duration_since(self.started))
    }

    /// Why the latest frame could not be recorded, or why it stopped by
    /// itself.
    pub(crate) fn problem(&self) -> Option<&str> {
        self.problem.as_deref()
    }

    /// Notes why the latest frame could not be recorded, or with `None`
    /// that it could.
    pub(crate) fn set_problem(&mut self, problem: Option<String>) {
        self.problem = problem;
    }

    /// Stops recording at `at`; later frames are ignored.
    pub(crate) fn stop(&mut self, at: Instant) {
        if self.stopped.is_none() {
            self.stopped = Some(at.saturating_duration_since(self.started));
        }
    }

    /// Stops recording at `at` by itself, as when frames stopped coming,
    /// with `problem` saying why. A stopped recording keeps its problem.
    pub(crate) fn fail(&mut self, at: Instant, problem: String) {
        if self.stopped.is_none() {
            self.stop(at);
            self.problem = Some(problem);
        }
    }

    /// Stops recording at `at` for good, as when the window closed, with
    /// `problem` saying why even if it had stopped already, since it can
    /// never go on.
    pub(crate) fn end(&mut self, at: Instant, problem: String) {
        self.stop(at);
        self.problem = Some(problem);
    }

    /// Goes on recording at `at` after a stop. The time it shows goes on
    /// from where it stopped.
    pub(crate) fn resume(&mut self, at: Instant) {
        // Why it last stopped no longer holds; the next frame tells.
        self.problem = None;
        if let Some(recorded) = self.stopped.take() {
            self.started = at.checked_sub(recorded).unwrap_or(at);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_time_counts_only_recording() {
        let start = Instant::now();
        let second = Duration::from_secs(1);
        let mut life = Lifecycle::new(start);
        assert!(life.is_recording());
        assert_eq!(life.elapsed(start + 3 * second), 3 * second);
        life.stop(start + 4 * second);
        life.stop(start + 5 * second);
        assert!(!life.is_recording());
        assert_eq!(life.elapsed(start + 9 * second), 4 * second);
        life.resume(start + 10 * second);
        life.resume(start + 11 * second);
        assert!(life.is_recording());
        assert_eq!(life.elapsed(start + 12 * second), 6 * second);
    }

    #[test]
    fn a_problem_lasts_until_a_frame_records_or_it_goes_on() {
        let start = Instant::now();
        let mut life = Lifecycle::new(start);
        assert_eq!(life.problem(), None);
        life.set_problem(Some("skipped".to_owned()));
        assert_eq!(life.problem(), Some("skipped"));
        assert!(life.is_recording(), "a skipped frame does not stop it");
        life.set_problem(None);
        assert_eq!(life.problem(), None);

        life.fail(start, "failed".to_owned());
        assert!(!life.is_recording());
        life.fail(start, "failed again".to_owned());
        assert_eq!(life.problem(), Some("failed"), "it had stopped already");
        life.end(start, "ended".to_owned());
        assert_eq!(life.problem(), Some("ended"), "an end says why even so");
        life.resume(start);
        assert_eq!(life.problem(), None);

        life.stop(start);
        life.fail(start, "failed".to_owned());
        assert_eq!(life.problem(), None, "the user stopped it first");
    }
}
