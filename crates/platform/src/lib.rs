//! The runtime's Windows host services: the window events that drive game
//! detection, the single-instance guard, autostart at sign-in, and the local
//! clock.
//!
//! Each service turns Windows API calls into a typed value or an error and
//! makes no decisions of its own. The Windows adapter lives in `win`; other
//! platforms get services that fail with [`io::ErrorKind::Unsupported`], so
//! the portable code that uses them still builds and tests there.
//!
//! See `docs/architecture.md` for this crate's boundaries.

use std::any::Any;
use std::ffi::OsStr;
use std::fmt;
use std::io;
use std::sync::mpsc::Sender;

#[cfg(not(windows))]
mod unsupported;
#[cfg(windows)]
mod win;

#[cfg(not(windows))]
use unsupported as host;
#[cfg(windows)]
use win as host;

/// A change to the set of windows on the desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowEvent {
    /// A top-level window was created.
    Created,
    /// A window was destroyed. Windows cannot tell whether a destroyed
    /// window was top-level, so this covers child windows too.
    Destroyed,
}

/// A thread that reports [`WindowEvent`]s for every process on the desktop.
///
/// The hook it installs never opens a process, so it is safe for every
/// anti-cheat class. Its thread ends when [`WindowWatch::stop`] runs, or
/// when the next event arrives after the receiver was dropped.
#[derive(Debug)]
pub struct WindowWatch(host::WindowWatch);

impl WindowWatch {
    /// Installs the hook on a new thread and returns once it is in place.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error when the thread or the hook
    /// cannot start.
    pub fn start(events: Sender<WindowEvent>) -> io::Result<Self> {
        host::WindowWatch::start(events).map(Self)
    }

    /// Removes the hook, ends the thread, and waits for it.
    ///
    /// # Errors
    ///
    /// Returns the error that ended the thread's message loop, if any.
    // Off Windows no watch exists, so no test there can reach this body; the
    // Windows adapter's tests call it.
    #[mutants::skip]
    pub fn stop(self) -> io::Result<()> {
        self.0.stop()
    }
}

/// Proof that this is the only running copy of a program in the user's
/// session. The guard lasts until the value is dropped or the process ends.
#[derive(Debug)]
pub struct InstanceGuard {
    _held: host::InstanceGuard,
}

impl InstanceGuard {
    /// The name of the runtime's guard. The setup app can check for it to
    /// learn whether the runtime is running.
    pub const RUNTIME: &str = r"Local\AppyTappy.Runtime";

    /// Takes the guard called `name`, or returns `None` when another holder
    /// has it.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    pub fn acquire(name: &str) -> io::Result<Option<Self>> {
        host::InstanceGuard::acquire(name).map(|guard| guard.map(|held| Self { _held: held }))
    }
}

/// The value under the user's `Run` registry key that starts the runtime at
/// sign-in.
pub const AUTOSTART_VALUE: &str = "AppyTappy";

/// Makes Windows run `command` when the user signs in, through the `Run`
/// key value `value`. It replaces any command the value held. The command
/// is a whole command line, with the program's path quoted.
///
/// # Errors
///
/// Returns the operating system's error.
pub fn enable_autostart(value: &str, command: &OsStr) -> io::Result<()> {
    host::enable_autostart(value, command)
}

/// Removes the `Run` key value `value`. A value that is already absent is
/// not an error.
///
/// # Errors
///
/// Returns the operating system's error.
pub fn disable_autostart(value: &str) -> io::Result<()> {
    host::disable_autostart(value)
}

/// The message a panicking thread left, for the error that reports it.
#[must_use]
pub fn panic_reason(payload: &(dyn Any + Send)) -> &str {
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("an unknown reason")
}

/// A wall-clock time in the user's time zone, to the second.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    /// The year, such as 2026.
    pub year: u16,
    /// The month, from 1.
    pub month: u16,
    /// The day of the month, from 1.
    pub day: u16,
    /// The hour, from 0 to 23.
    pub hour: u16,
    /// The minute.
    pub minute: u16,
    /// The second.
    pub second: u16,
}

impl LocalTime {
    /// The current local time, or `None` where AppyTappy does not run.
    // Off Windows this body already returns `None`, so the mutation testing
    // there cannot tell a mutant from it.
    #[mutants::skip]
    #[must_use]
    pub fn now() -> Option<Self> {
        host::local_time()
    }
}

impl fmt::Display for LocalTime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_reason_is_its_message() {
        let fixed: Box<dyn Any + Send> = Box::new("fixed");
        let formatted: Box<dyn Any + Send> = Box::new(format!("formatted {}", 1));
        let other: Box<dyn Any + Send> = Box::new(1_u8);
        let reasons =
            [fixed, formatted, other].map(|payload| panic_reason(payload.as_ref()).to_owned());
        assert_eq!(reasons, ["fixed", "formatted 1", "an unknown reason"]);
    }

    #[test]
    fn local_time_reads_year_first_with_padded_fields() {
        let time = LocalTime {
            year: 2026,
            month: 11,
            day: 7,
            hour: 3,
            minute: 5,
            second: 9,
        };
        assert_eq!(time.to_string(), "2026-11-07 03:05:09");
    }
}
