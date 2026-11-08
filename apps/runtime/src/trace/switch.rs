//! Turning the trace on and off as the settings say.
//!
//! The owner thread holds the [`Switch`]. Each time it reads the settings,
//! the switch starts a trace, keeps the one that runs, or stops it, and says
//! when the threads need a new [`Trace`] handle. A trace with an end time
//! stops by itself once that time passes, as the owner wakes for it.

use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use appytappy_config::settings_file::TraceDef;

use super::Trace;
use super::writer::{Options, TraceReport, Writer};
use crate::log::Log;
use crate::note;
use crate::plan::AppPlan;
use crate::workers::reader::SessionId;

/// How many bytes a megabyte of the file size limit is.
const MEGABYTE: u64 = 1_048_576;

/// The trace that runs, if any, and where traces go.
#[derive(Debug)]
pub(crate) struct Switch {
    /// The folder each trace's files go in.
    folder: PathBuf,
    /// The trace that runs.
    running: Option<Running>,
    /// The number the next trace gets.
    next: u64,
}

/// A trace that runs.
#[derive(Debug)]
struct Running {
    /// Its number.
    id: u64,
    writer: Writer,
    /// How it writes, from the settings.
    options: Options,
    /// When it stops by itself.
    until: Option<Instant>,
}

impl Switch {
    /// A switch with no trace, whose traces go in `folder`.
    pub(crate) const fn new(folder: PathBuf) -> Self {
        Self {
            folder,
            running: None,
            next: 1,
        }
    }

    /// The number of the trace that runs.
    #[cfg(test)]
    fn running(&self) -> Option<u64> {
        self.running.as_ref().map(|running| running.id)
    }

    /// When the trace that runs stops by itself.
    pub(crate) fn until(&self) -> Option<Instant> {
        self.running.as_ref().and_then(|running| running.until)
    }

    /// Starts, keeps, or stops the trace as `saved` says, at `now`, which is
    /// `unix_ms` milliseconds since 1970. A trace whose options are the same
    /// keeps running, with the end time `saved` gives it. Returns the handle
    /// every thread records with from now on, when it changed. The writer
    /// sends why it stopped, should it fail, to `reports`.
    pub(crate) fn apply<E>(
        &mut self,
        saved: &TraceDef,
        (now, unix_ms): (Instant, u64),
        reports: &Sender<E>,
        log: &mut Log,
    ) -> Option<Trace>
    where
        E: From<TraceReport> + Send + 'static,
    {
        let until = match saved.on.then(|| end(saved.until, now, unix_ms)) {
            Some(End::Never) => None,
            Some(End::At(at)) => Some(at),
            None | Some(End::Passed) => return self.stop(log, "the trace stopped"),
        };
        let options = self.options(saved);
        if let Some(running) = self
            .running
            .as_mut()
            .filter(|running| running.options == options)
        {
            running.until = until;
            return None;
        }
        let stopped = self.stop(log, "the trace stopped, so a new one starts");
        let id = self.next;
        self.next = self.next.saturating_add(1);
        match Writer::start(id, options.clone(), reports.clone()) {
            Ok((writer, trace)) => {
                note(log, &format!("the trace records to {}", writer.file()));
                self.running = Some(Running {
                    id,
                    writer,
                    options,
                    until,
                });
                Some(trace)
            },
            Err(error) => {
                note(log, &format!("the trace cannot start: {error}"));
                stopped
            },
        }
    }

    /// Stops the trace when its end time has passed at `now`. Returns
    /// [`Trace::OFF`] when it stopped.
    pub(crate) fn expire(&mut self, now: Instant, log: &mut Log) -> Option<Trace> {
        if self.until().is_some_and(|until| until <= now) {
            self.stop(log, "the trace's time is up, so it stopped")
        } else {
            None
        }
    }

    /// Forgets trace `id`, whose writer stopped because it could not write
    /// and told the owner why. Returns [`Trace::OFF`] when that was the
    /// trace that runs; a report from an earlier trace changes nothing.
    pub(crate) fn failed(&mut self, id: u64, log: &mut Log) -> Option<Trace> {
        if self
            .running
            .as_ref()
            .is_some_and(|running| running.id == id)
        {
            self.stop(log, "the trace stopped")
        } else {
            None
        }
    }

    /// Hands the writer the plan that names `session`'s rows.
    pub(crate) fn name(&self, session: SessionId, plan: &Arc<AppPlan>) {
        if let Some(running) = &self.running {
            running.writer.name(session, Arc::clone(plan));
        }
    }

    /// Stops the trace that runs, if any, and logs `line`. Returns
    /// [`Trace::OFF`] when one ran.
    fn stop(&mut self, log: &mut Log, line: &str) -> Option<Trace> {
        let running = self.running.take()?;
        match running.writer.stop() {
            Ok(()) => note(log, line),
            Err(error) => note(log, &format!("the trace stopped badly: {error}")),
        }
        Some(Trace::OFF)
    }

    /// Stops the trace at the end of the runtime.
    ///
    /// # Errors
    ///
    /// Returns an error when the writer thread panicked.
    pub(crate) fn finish(self) -> io::Result<()> {
        self.running.map_or(Ok(()), |running| running.writer.stop())
    }

    /// How a trace with the `saved` settings writes.
    fn options(&self, saved: &TraceDef) -> Options {
        Options {
            folder: self.folder.clone(),
            categories: saved.record.iter().copied().collect(),
            file_limit: u64::from(saved.file_limit_mb).saturating_mul(MEGABYTE),
            keep_files: usize::try_from(saved.keep_files).unwrap_or(usize::MAX),
        }
    }
}

/// When a saved trace stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum End {
    /// It records until it is turned off.
    Never,
    /// It stops by itself then.
    At(Instant),
    /// Its end time passed, so it does not record.
    Passed,
}

/// When a trace saved with the end time `until`, in milliseconds since 1970,
/// stops, as seen at `now`, which is `unix_ms`.
fn end(until: Option<u64>, now: Instant, unix_ms: u64) -> End {
    let Some(until) = until else {
        return End::Never;
    };
    match until.checked_sub(unix_ms).filter(|left| *left > 0) {
        None => End::Passed,
        // An end too far off to count from now has no end that matters.
        Some(left) => now
            .checked_add(Duration::from_millis(left))
            .map_or(End::Never, End::At),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::mpsc;
    use std::{env, fs, process};

    use appytappy_config::settings_file::TraceCategory;

    use super::*;
    use crate::log;

    struct Scratch {
        folder: PathBuf,
        log: Log,
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let folder = env::temp_dir().join(format!("appytappy-switch-{name}-{}", process::id()));
            drop(fs::remove_dir_all(&folder));
            fs::create_dir_all(&folder).expect("make the folder");
            let log = Log::open(&folder.join("logs"), log::LIMIT).expect("open the log");
            Self { folder, log }
        }

        fn traces(&self) -> PathBuf {
            self.folder.join("traces")
        }

        fn lines(&self) -> Vec<String> {
            fs::read_to_string(self.folder.join("logs").join("runtime.log"))
                .expect("read the log")
                .lines()
                .map(str::to_owned)
                .collect()
        }

        fn files(&self) -> usize {
            fs::read_dir(self.traces()).map_or(0, Iterator::count)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            drop(fs::remove_dir_all(&self.folder));
        }
    }

    fn on() -> TraceDef {
        TraceDef {
            on: true,
            ..TraceDef::default()
        }
    }

    const NOW_MS: u64 = 1_762_600_000_000;

    #[test]
    fn a_saved_end_time_counts_from_now() {
        let now = Instant::now();
        assert_eq!(end(None, now, NOW_MS), End::Never, "no end");
        assert_eq!(
            end(Some(NOW_MS + 600_000), now, NOW_MS),
            End::At(
                now.checked_add(Duration::from_secs(600))
                    .expect("ten minutes on")
            )
        );
        assert_eq!(end(Some(NOW_MS), now, NOW_MS), End::Passed, "ends now");
        assert_eq!(end(Some(NOW_MS - 1), now, NOW_MS), End::Passed, "ended");
    }

    #[test]
    fn the_settings_start_keep_and_stop_the_trace() {
        let mut scratch = Scratch::new("apply");
        let (reports, _received) = mpsc::channel::<TraceReport>();
        let mut switch = Switch::new(scratch.traces());
        let now = Instant::now();
        let at = (now, NOW_MS);
        assert_eq!(
            switch.apply(&TraceDef::default(), at, &reports, &mut scratch.log),
            None,
            "off stays off"
        );
        assert_eq!(switch.running(), None);
        let first = switch
            .apply(&on(), at, &reports, &mut scratch.log)
            .expect("starts");
        assert_ne!(first, Trace::OFF);
        assert_eq!(switch.running(), Some(1));
        assert_eq!(switch.until(), None, "no end time");
        let later = TraceDef {
            until: Some(NOW_MS + 60_000),
            ..on()
        };
        assert_eq!(
            switch.apply(&later, at, &reports, &mut scratch.log),
            None,
            "a new end time keeps the trace"
        );
        assert_eq!(switch.until(), now.checked_add(Duration::from_secs(60)));
        let more = TraceDef {
            record: BTreeSet::from([TraceCategory::Readings]),
            ..on()
        };
        let second = switch
            .apply(&more, at, &reports, &mut scratch.log)
            .expect("new categories start a new trace");
        assert_ne!(second, first);
        assert!(second.wants(TraceCategory::Readings));
        assert_eq!(
            switch.apply(&TraceDef::default(), at, &reports, &mut scratch.log),
            Some(Trace::OFF)
        );
        assert_eq!(switch.running(), None);
        let lines = scratch.lines();
        assert_eq!(lines.len(), 4, "{lines:?}");
        assert!(
            lines[0].contains("the trace records to trace-"),
            "{lines:?}"
        );
        assert!(
            lines[1].ends_with("the trace stopped, so a new one starts"),
            "{lines:?}"
        );
        assert!(
            lines[2].contains("the trace records to trace-"),
            "{lines:?}"
        );
        assert!(lines[3].ends_with("the trace stopped"), "{lines:?}");
        assert_eq!(scratch.files(), 2, "each trace has its file");
    }

    #[test]
    fn a_trace_whose_end_time_passed_does_not_start() {
        let mut scratch = Scratch::new("ended");
        let (reports, _received) = mpsc::channel::<TraceReport>();
        let mut switch = Switch::new(scratch.traces());
        let ended = TraceDef {
            until: Some(NOW_MS - 1),
            ..on()
        };
        assert_eq!(
            switch.apply(&ended, (Instant::now(), NOW_MS), &reports, &mut scratch.log),
            None
        );
        assert_eq!(scratch.files(), 0);
    }

    #[test]
    fn a_trace_stops_when_its_time_is_up() {
        let mut scratch = Scratch::new("expire");
        let (reports, _received) = mpsc::channel::<TraceReport>();
        let mut switch = Switch::new(scratch.traces());
        let now = Instant::now();
        let saved = TraceDef {
            until: Some(NOW_MS + 1_000),
            ..on()
        };
        switch
            .apply(&saved, (now, NOW_MS), &reports, &mut scratch.log)
            .expect("starts");
        let until = switch.until().expect("an end time");
        assert_eq!(
            switch.expire(
                until
                    .checked_sub(Duration::from_millis(1))
                    .expect("a moment before"),
                &mut scratch.log
            ),
            None
        );
        assert_eq!(switch.expire(until, &mut scratch.log), Some(Trace::OFF));
        assert_eq!(switch.until(), None);
        assert!(
            scratch
                .lines()
                .last()
                .is_some_and(|line| line.ends_with("the trace's time is up, so it stopped"))
        );
    }

    #[test]
    fn only_the_trace_that_runs_stops_for_its_failure() {
        let mut scratch = Scratch::new("failed");
        let (reports, _received) = mpsc::channel::<TraceReport>();
        let mut switch = Switch::new(scratch.traces());
        let at = (Instant::now(), NOW_MS);
        switch
            .apply(&on(), at, &reports, &mut scratch.log)
            .expect("trace 1 starts");
        let more = TraceDef {
            record: BTreeSet::from([TraceCategory::Readings]),
            ..on()
        };
        switch
            .apply(&more, at, &reports, &mut scratch.log)
            .expect("trace 2 starts");
        assert_eq!(switch.failed(1, &mut scratch.log), None, "trace 1 is over");
        assert_eq!(switch.running(), Some(2));
        assert_eq!(switch.failed(2, &mut scratch.log), Some(Trace::OFF));
        assert_eq!(switch.running(), None);
        switch.finish().expect("nothing to stop");
    }

    #[test]
    fn a_folder_that_cannot_be_made_starts_no_trace() {
        let mut scratch = Scratch::new("cannot");
        let (reports, _received) = mpsc::channel::<TraceReport>();
        // A file where the folder should be.
        fs::write(scratch.traces(), b"").expect("write");
        let mut switch = Switch::new(scratch.traces());
        assert_eq!(
            switch.apply(&on(), (Instant::now(), NOW_MS), &reports, &mut scratch.log),
            None
        );
        assert_eq!(switch.running(), None);
        let lines = scratch.lines();
        assert!(
            lines
                .last()
                .is_some_and(|line| line.contains("the trace cannot start: ")),
            "{lines:?}"
        );
    }

    #[test]
    fn the_file_size_limit_is_in_megabytes() {
        let switch = Switch::new(PathBuf::from("traces"));
        let options = switch.options(&TraceDef {
            file_limit_mb: 2,
            keep_files: 3,
            ..TraceDef::default()
        });
        assert_eq!(options.file_limit, 2 * 1_048_576);
        assert_eq!(options.keep_files, 3);
        assert_eq!(options.folder, PathBuf::from("traces"));
        assert!(!options.categories.contains(TraceCategory::Readings));
        assert!(options.categories.contains(TraceCategory::Rules));
    }
}
