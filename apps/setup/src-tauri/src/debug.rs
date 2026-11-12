//! The setup app's debug mode, which **Show debug details** on the Settings
//! page turns on: what the Debug page shows, `setup.log`, and the debug
//! bundle.

pub(crate) mod bundle;
pub(crate) mod log;
pub(crate) mod trace_file;

use std::fs::File;
use std::io::{self, Read as _, Seek as _, SeekFrom};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::debug::trace_file::TraceSummary;

/// How much of the end of a log the Debug page shows.
const LOG_TAIL_BYTES: u64 = 64 * 1024;

/// What the Debug page's overview shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct DebugOverview {
    /// Whether the runtime is running, or `None` when that cannot be told.
    pub(crate) runtime: Option<bool>,
    /// The newest trace file, or `None` without one.
    pub(crate) trace: Option<TraceSummary>,
    /// Why the newest trace file cannot be read, if it cannot.
    pub(crate) trace_problem: Option<String>,
}

/// One of the two logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) enum LogName {
    /// The runtime's log, `runtime.log`.
    Runtime,
    /// The setup app's log, `setup.log`.
    Setup,
}

impl LogName {
    /// The log's file name.
    pub(crate) const fn file(self) -> &'static str {
        match self {
            Self::Runtime => "runtime.log",
            Self::Setup => "setup.log",
        }
    }
}

/// The end of the log file `path`, whole lines only, or nothing when there
/// is no file yet.
///
/// # Errors
///
/// Returns the error from reading the file.
pub(crate) fn log_tail(path: &Path) -> io::Result<String> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(String::new()),
        Err(error) => return Err(error),
    };
    let length = file.metadata()?.len();
    let start = length.saturating_sub(LOG_TAIL_BYTES);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes);
    // A tail that starts inside a line drops that line's end.
    let whole = if start == 0 {
        &*text
    } else {
        text.split_once('\n').map_or("", |(_cut, rest)| rest)
    };
    Ok(whole.to_owned())
}

#[cfg(test)]
mod tests {
    use std::{env, fs, process};

    use super::*;

    #[test]
    fn a_log_tail_keeps_whole_lines_and_a_missing_log_is_empty() {
        let folder = env::temp_dir().join(format!("appytappy-log-tail-{}", process::id()));
        fs::create_dir_all(&folder).expect("folder");
        let path = folder.join("runtime.log");
        assert_eq!(log_tail(&path).expect("missing"), "");
        fs::write(&path, "first\nsecond\n").expect("write");
        assert_eq!(log_tail(&path).expect("short"), "first\nsecond\n");
        let long = format!("{}\nlast\n", "x".repeat(70 * 1024));
        fs::write(&path, long).expect("write");
        assert_eq!(log_tail(&path).expect("long"), "last\n");
        fs::remove_dir_all(&folder).expect("clean up");
    }

    #[test]
    fn each_log_has_its_file() {
        assert_eq!(LogName::Runtime.file(), "runtime.log");
        assert_eq!(LogName::Setup.file(), "setup.log");
    }
}
