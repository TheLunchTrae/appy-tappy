//! The runtime's log: one file that rotates to one old file, so it never
//! takes more than twice its limit on disk.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use appytappy_platform::LocalTime;

/// The size at which the log rotates.
pub(crate) const LIMIT: u64 = 524_288;

/// An open log file.
#[derive(Debug)]
pub(crate) struct Log {
    path: PathBuf,
    old: PathBuf,
    file: File,
    size: u64,
    limit: u64,
}

fn open_append(path: &Path) -> io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

impl Log {
    /// Opens `runtime.log` in `folder`, creating both if needed. A line that
    /// would take the file past `limit` bytes first moves it to
    /// `runtime.old.log`, replacing the old one.
    pub(crate) fn open(folder: &Path, limit: u64) -> io::Result<Self> {
        fs::create_dir_all(folder)?;
        let path = folder.join("runtime.log");
        let file = open_append(&path)?;
        let size = file.metadata()?.len();
        Ok(Self {
            old: folder.join("runtime.old.log"),
            path,
            file,
            size,
            limit,
        })
    }

    /// Appends `message` as one line, after `time` when there is one.
    pub(crate) fn write(&mut self, time: Option<LocalTime>, message: &str) -> io::Result<()> {
        let line = match time {
            Some(time) => format!("{time} {message}\n"),
            None => format!("{message}\n"),
        };
        let length = u64::try_from(line.len()).unwrap_or(u64::MAX);
        if self.size != 0 && self.size.saturating_add(length) > self.limit {
            fs::rename(&self.path, &self.old)?;
            self.file = open_append(&self.path)?;
            self.size = 0;
        }
        self.file.write_all(line.as_bytes())?;
        self.size = self.size.saturating_add(length);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::process;

    use super::*;

    /// A fresh folder for one test, removed when the value drops.
    struct Folder(PathBuf);

    impl Folder {
        fn new(test: &str) -> Self {
            let path = env::temp_dir().join(format!("appytappy-log-{test}-{}", process::id()));
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a folder left by an earlier run may or may not exist"
            )]
            let _ = fs::remove_dir_all(&path);
            Self(path)
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "cleanup is best-effort, and Drop has no caller to report to"
            )]
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn read(path: &Path) -> String {
        fs::read_to_string(path).expect("read the log")
    }

    #[test]
    fn lines_carry_the_time_and_append_across_opens() {
        let folder = Folder::new("append");
        let time = LocalTime {
            year: 2026,
            month: 11,
            day: 7,
            hour: 3,
            minute: 5,
            second: 9,
        };
        Log::open(&folder.0, LIMIT)
            .expect("open")
            .write(Some(time), "started")
            .expect("write");
        Log::open(&folder.0, LIMIT)
            .expect("reopen")
            .write(None, "stopped")
            .expect("write");
        assert_eq!(
            read(&folder.0.join("runtime.log")),
            "2026-11-07 03:05:09 started\nstopped\n"
        );
    }

    #[test]
    fn a_line_past_the_limit_rotates_the_file() {
        let folder = Folder::new("rotate");
        let mut log = Log::open(&folder.0, 10).expect("open");
        log.write(None, "first").expect("write");
        log.write(None, "fou").expect("write");
        assert_eq!(
            read(&folder.0.join("runtime.log")),
            "first\nfou\n",
            "exactly at the limit"
        );
        log.write(None, "second").expect("write");
        log.write(None, "third").expect("write");
        assert_eq!(read(&folder.0.join("runtime.old.log")), "second\n");
        assert_eq!(read(&folder.0.join("runtime.log")), "third\n");
    }

    #[test]
    fn a_reopened_log_counts_what_the_file_already_holds() {
        let folder = Folder::new("reopen");
        Log::open(&folder.0, 10)
            .expect("open")
            .write(None, "first")
            .expect("write");
        Log::open(&folder.0, 10)
            .expect("reopen")
            .write(None, "second")
            .expect("write");
        assert_eq!(read(&folder.0.join("runtime.old.log")), "first\n");
        assert_eq!(read(&folder.0.join("runtime.log")), "second\n");
    }
}
