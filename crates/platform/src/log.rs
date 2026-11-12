//! A program's log: one file that rotates to one old file, so it never takes
//! more than twice its limit on disk. The runtime writes `runtime.log`, and
//! the setup app writes `setup.log` while it shows debug details.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::LocalTime;

/// The size at which a log rotates.
pub const LOG_LIMIT: u64 = 524_288;

/// An open log file.
#[derive(Debug)]
pub struct Log {
    path: PathBuf,
    old: PathBuf,
    file: File,
    size: u64,
    limit: u64,
    /// Makes opening the new file fail, for a test.
    #[cfg(test)]
    fail_open: bool,
}

fn open_append(path: &Path) -> io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

impl Log {
    /// Opens `{name}.log` in `folder`, creating both if needed. A line that
    /// would take the file past `limit` bytes first moves it to
    /// `{name}.old.log`, replacing the old one.
    ///
    /// # Errors
    ///
    /// Returns the error from creating the folder or opening the file.
    pub fn open(folder: &Path, name: &str, limit: u64) -> io::Result<Self> {
        fs::create_dir_all(folder)?;
        let path = folder.join(format!("{name}.log"));
        let file = open_append(&path)?;
        let size = file.metadata()?.len();
        Ok(Self {
            old: folder.join(format!("{name}.old.log")),
            path,
            file,
            size,
            limit,
            #[cfg(test)]
            fail_open: false,
        })
    }

    /// Makes the log rotate at `limit` bytes from now on. Only the copy that
    /// holds the single-instance guard rotates, so another copy that only
    /// notes why it exits never moves the file under it.
    pub const fn rotate_at(&mut self, limit: u64) {
        self.limit = limit;
    }

    /// Appends `message` as one line, after `time` when there is one.
    ///
    /// # Errors
    ///
    /// Returns the error from writing the line.
    pub fn write(&mut self, time: Option<LocalTime>, message: &str) -> io::Result<()> {
        let line = match time {
            Some(time) => format!("{time} {message}\n"),
            None => format!("{message}\n"),
        };
        let length = u64::try_from(line.len()).unwrap_or(u64::MAX);
        if self.size != 0 && self.size.saturating_add(length) > self.limit {
            self.rotate();
        }
        self.file.write_all(line.as_bytes())?;
        self.size = self.size.saturating_add(length);
        Ok(())
    }

    /// Moves the file to the old name and starts a new one. A step that
    /// fails, such as a move while another program holds the file, leaves
    /// the log writing where it did, past its limit, and the next line tries
    /// again, because the log is the runtime's only output. A move that finds
    /// no file, because it was deleted or already moved by a try whose open
    /// failed, goes on to the open, so the old file is never moved twice.
    fn rotate(&mut self) {
        if let Err(error) = fs::rename(&self.path, &self.old)
            && error.kind() != io::ErrorKind::NotFound
        {
            return;
        }
        if let Ok(file) = self.open_new() {
            self.size = file.metadata().map_or(0, |metadata| metadata.len());
            self.file = file;
        }
    }

    /// Opens the new file at the log's path.
    fn open_new(&self) -> io::Result<File> {
        #[cfg(test)]
        if self.fail_open {
            return Err(io::Error::other("injected"));
        }
        open_append(&self.path)
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
        Log::open(&folder.0, "runtime", LOG_LIMIT)
            .expect("open")
            .write(Some(time), "started")
            .expect("write");
        Log::open(&folder.0, "runtime", LOG_LIMIT)
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
        let mut log = Log::open(&folder.0, "runtime", 10).expect("open");
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
    fn a_log_that_cannot_move_keeps_every_line() {
        let folder = Folder::new("stuck");
        let mut log = Log::open(&folder.0, "runtime", 10).expect("open");
        log.write(None, "first").expect("write");
        // A folder that is not empty in the old file's place stops the move.
        let blocker = folder.0.join("runtime.old.log");
        fs::create_dir(&blocker).expect("block the move");
        fs::write(blocker.join("held"), "").expect("fill the folder");
        log.write(None, "second").expect("still written");
        log.write(None, "third").expect("still written");
        assert_eq!(
            read(&folder.0.join("runtime.log")),
            "first\nsecond\nthird\n"
        );
        fs::remove_dir_all(&blocker).expect("unblock the move");
        // Short enough to fit under the limit if a failed move had reset the
        // size, so only a log that still counts every line moves it here.
        log.write(None, "4th").expect("write");
        assert_eq!(
            read(&blocker),
            "first\nsecond\nthird\n",
            "the next line moves it"
        );
        assert_eq!(read(&folder.0.join("runtime.log")), "4th\n");
    }

    #[test]
    fn a_new_file_that_cannot_open_keeps_the_moved_one_until_it_can() {
        let folder = Folder::new("reopen-fails");
        let mut log = Log::open(&folder.0, "runtime", 10).expect("open");
        log.write(None, "first").expect("write");
        log.fail_open = true;
        log.write(None, "second")
            .expect("written to the moved file");
        log.write(None, "third").expect("written to the moved file");
        assert_eq!(
            read(&folder.0.join("runtime.old.log")),
            "first\nsecond\nthird\n"
        );
        assert!(!folder.0.join("runtime.log").exists());
        log.fail_open = false;
        log.write(None, "fourth").expect("write");
        assert_eq!(
            read(&folder.0.join("runtime.old.log")),
            "first\nsecond\nthird\n",
            "the moved file is not moved again"
        );
        assert_eq!(read(&folder.0.join("runtime.log")), "fourth\n");
    }

    #[test]
    fn a_log_deleted_while_open_starts_a_new_file() {
        let folder = Folder::new("deleted");
        let mut log = Log::open(&folder.0, "runtime", 10).expect("open");
        log.write(None, "first").expect("write");
        fs::remove_file(folder.0.join("runtime.log")).expect("delete the log");
        log.write(None, "second").expect("write");
        assert_eq!(read(&folder.0.join("runtime.log")), "second\n");
        assert!(!folder.0.join("runtime.old.log").exists());
    }

    #[test]
    fn a_log_rotates_only_once_given_a_limit() {
        let folder = Folder::new("unlimited");
        let mut log = Log::open(&folder.0, "runtime", u64::MAX).expect("open");
        log.write(None, "first").expect("write");
        log.write(None, "second").expect("write");
        log.rotate_at(10);
        log.write(None, "third").expect("write");
        assert_eq!(read(&folder.0.join("runtime.old.log")), "first\nsecond\n");
        assert_eq!(read(&folder.0.join("runtime.log")), "third\n");
    }

    #[test]
    fn a_reopened_log_counts_what_the_file_already_holds() {
        let folder = Folder::new("reopen");
        Log::open(&folder.0, "runtime", 10)
            .expect("open")
            .write(None, "first")
            .expect("write");
        Log::open(&folder.0, "runtime", 10)
            .expect("reopen")
            .write(None, "second")
            .expect("write");
        assert_eq!(read(&folder.0.join("runtime.old.log")), "first\n");
        assert_eq!(read(&folder.0.join("runtime.log")), "second\n");
    }
}
