//! `setup.log`, beside the runtime's log: each command the page calls, how
//! long it took and why it failed. The setup app writes it only while it
//! shows debug details, and it rotates like the runtime's log. The owner
//! thread holds it and writes each line as a command's work ends there.

use std::path::PathBuf;
use std::time::Duration;

use appytappy_platform::{LOG_LIMIT, LocalTime, Log};

/// The commands the page calls again and again on a timer. A success of one
/// of them that took less than [`SLOW`] is left out, so the log keeps what
/// the user did rather than the page's polling.
pub(crate) const REPEATED: &[&str] = &[
    "debug_overview",
    "explain_reading",
    "explain_watched",
    "found_picture",
    "found_state",
    "list_apps",
    "list_automations",
    "list_windows",
    "live_values",
    "look_recording_state",
    "match_windows",
    "overlay_state",
    "preview",
    "read_log",
    "read_value",
    "recorded_reading",
    "recording_state",
    "screenshot",
    "sync_overlays",
    "trace_files",
    "trace_metrics",
    "trace_rows",
    "watched_reading",
    "window_size",
];

/// How long a repeated command takes before its success is logged.
const SLOW: Duration = Duration::from_millis(50);

/// The setup app's log, open only while it shows debug details.
#[derive(Debug)]
pub(crate) struct SetupLog {
    /// The folder the log goes in, or `None` when it is unknown.
    folder: Option<PathBuf>,
    /// The open file, while debug details show.
    open: Option<Log>,
}

impl SetupLog {
    /// A log in `folder`, which writes nothing until [`SetupLog::set`] turns
    /// it on.
    pub(crate) const fn new(folder: Option<PathBuf>) -> Self {
        Self { folder, open: None }
    }

    /// Starts writing, or stops and closes the file. A log that cannot open
    /// stays off; the setup app works the same without it.
    pub(crate) fn set(&mut self, on: bool) {
        if !on {
            self.open = None;
        } else if self.open.is_none()
            && let Some(folder) = &self.folder
        {
            self.open = Log::open(folder, "setup", LOG_LIMIT).ok();
        }
    }

    /// Records that `command` took `took`, and why it failed, if it did.
    pub(crate) fn record(&mut self, command: &str, took: Duration, failure: Option<&String>) {
        let (Some(log), Some(line)) = (self.open.as_mut(), line(command, took, failure)) else {
            return;
        };
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a line the log cannot take is lost; the command's answer goes to the page either way"
        )]
        let _ = log.write(LocalTime::now(), &line);
    }
}

/// The line for `command`, or `None` for a quick success of a repeated one.
fn line(command: &str, took: Duration, failure: Option<&String>) -> Option<String> {
    let ms = took.as_secs_f64() * 1000.0;
    match failure {
        Some(reason) => Some(format!("{command} failed after {ms:.1} ms: {reason}")),
        None if took < SLOW && REPEATED.contains(&command) => None,
        None => Some(format!("{command} took {ms:.1} ms")),
    }
}

#[cfg(test)]
mod tests {
    use std::{env, fs, process};

    use super::*;

    #[test]
    fn a_line_says_how_long_a_command_took_and_why_it_failed() {
        let quick = Duration::from_micros(1_250);
        assert_eq!(
            line("save_value", quick, None),
            Some("save_value took 1.2 ms".to_owned())
        );
        assert_eq!(
            line("save_value", quick, Some(&"Name it first.".to_owned())),
            Some("save_value failed after 1.2 ms: Name it first.".to_owned())
        );
    }

    #[test]
    fn a_repeated_command_is_logged_only_when_slow_or_failed() {
        assert_eq!(line("preview", Duration::from_millis(49), None), None);
        assert_eq!(
            line("preview", SLOW, None),
            Some("preview took 50.0 ms".to_owned())
        );
        assert!(line("preview", Duration::ZERO, Some(&"Gone.".to_owned())).is_some());
    }

    #[test]
    fn the_log_writes_only_while_on() {
        let folder = env::temp_dir().join(format!("appytappy-setup-log-{}", process::id()));
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a folder left by an earlier run may or may not exist"
        )]
        let _ = fs::remove_dir_all(&folder);
        let mut log = SetupLog::new(Some(folder.clone()));
        let failed = "No window.".to_owned();
        log.record("pick_window", Duration::ZERO, Some(&failed));
        assert!(!folder.join("setup.log").exists(), "off writes nothing");
        log.set(true);
        log.record("pick_window", Duration::ZERO, Some(&failed));
        log.set(false);
        log.record("save_value", Duration::ZERO, None);
        let text = fs::read_to_string(folder.join("setup.log")).expect("read");
        assert!(
            text.trim_end()
                .ends_with("pick_window failed after 0.0 ms: No window."),
            "{text}"
        );
        assert_eq!(text.lines().count(), 1, "{text}");
        fs::remove_dir_all(&folder).expect("clean up");
    }

    #[test]
    fn a_log_with_no_folder_stays_off() {
        let mut log = SetupLog::new(None);
        log.set(true);
        assert!(log.open.is_none(), "no folder, no file");
    }
}
