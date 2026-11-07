//! The operating system services the guided scan needs.
//!
//! They are process and window discovery that never opens a process, the
//! user's answer before memory access, opening a process under a permit, and
//! screen pixels.

#![forbid(unsafe_code)]

use std::convert::Infallible;
use std::io;

use appytappy_config::anti_cheat::{ProcessAccessPermit, ProcessId};

use crate::memory::ProcessMemory;

/// A running process, as a system-wide snapshot reports it without opening
/// the process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessEntry {
    /// The process ID.
    pub id: ProcessId,
    /// The ID of the process that started it.
    pub parent: ProcessId,
    /// The executable's file name, such as `Example.exe`.
    pub executable: String,
    /// How many threads it runs.
    pub threads: u32,
}

/// A top-level window, as window enumeration reports it without opening the
/// owning process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowEntry {
    /// The window handle as a number.
    pub handle: u64,
    /// The process that owns the window.
    pub process: ProcessId,
    /// The window's title.
    pub title: String,
    /// The window's class name.
    pub class: String,
    /// Whether the window is visible.
    pub visible: bool,
    /// The window's bounds in screen pixels.
    pub bounds: Bounds,
}

/// A rectangle in screen pixels; `right` and `bottom` are exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    /// The left edge.
    pub left: i32,
    /// The top edge.
    pub top: i32,
    /// The right edge.
    pub right: i32,
    /// The bottom edge.
    pub bottom: i32,
}

/// A pixel's color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    /// Red.
    pub red: u8,
    /// Green.
    pub green: u8,
    /// Blue.
    pub blue: u8,
}

/// The user's answer to a request to read a process's memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessAnswer {
    /// The user confirmed that the app has no anti-cheat.
    Allowed,
    /// The user refused, or closed the prompt.
    Refused,
}

/// The operating system services the guided scan uses.
pub trait System {
    /// An opened process.
    type Process: ProcessMemory;

    /// Lists running processes without opening any of them.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn processes(&self) -> io::Result<Vec<ProcessEntry>>;

    /// Lists top-level windows without opening their processes.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn windows(&self) -> io::Result<Vec<WindowEntry>>;

    /// Asks the user, outside the agent's reach, whether `process` may be
    /// read. `anti_cheat` names anti-cheat products that are running.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error when the prompt cannot be shown.
    fn ask_memory_access(
        &self,
        process: &ProcessEntry,
        anti_cheat: &[&str],
    ) -> io::Result<AccessAnswer>;

    /// Opens the permitted process for reading only.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn open(&self, permit: &ProcessAccessPermit) -> io::Result<Self::Process>;

    /// Reads one pixel of the screen.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn screen_pixel(&self, x: i32, y: i32) -> io::Result<Rgb>;
}

/// The system of a platform AppyTappy does not support: every call fails
/// with [`io::ErrorKind::Unsupported`].
#[derive(Debug, Default)]
pub struct Unsupported;

/// The process type of [`Unsupported`], which cannot be constructed.
#[derive(Debug)]
pub struct NoProcess(Infallible);

fn unsupported<T>() -> io::Result<T> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "AppyTappy reads application state on Windows only",
    ))
}

// `NoProcess` holds `Infallible`, so no value exists and these bodies never
// run. No test can call them, so mutating them cannot be caught.
impl ProcessMemory for NoProcess {
    #[mutants::skip]
    fn regions(&self) -> io::Result<Vec<crate::memory::Region>> {
        match self.0 {}
    }

    #[mutants::skip]
    fn read(&self, _address: u64, _buffer: &mut [u8]) -> io::Result<()> {
        match self.0 {}
    }

    #[mutants::skip]
    fn modules(&self) -> io::Result<Vec<crate::memory::Module>> {
        match self.0 {}
    }

    #[mutants::skip]
    fn pointer_width(&self) -> crate::memory::PointerWidth {
        match self.0 {}
    }

    #[mutants::skip]
    fn image_name(&self) -> io::Result<String> {
        match self.0 {}
    }
}

impl System for Unsupported {
    type Process = NoProcess;

    fn processes(&self) -> io::Result<Vec<ProcessEntry>> {
        unsupported()
    }

    fn windows(&self) -> io::Result<Vec<WindowEntry>> {
        unsupported()
    }

    fn ask_memory_access(
        &self,
        _process: &ProcessEntry,
        _anti_cheat: &[&str],
    ) -> io::Result<AccessAnswer> {
        unsupported()
    }

    fn open(&self, _permit: &ProcessAccessPermit) -> io::Result<NoProcess> {
        unsupported()
    }

    fn screen_pixel(&self, _x: i32, _y: i32) -> io::Result<Rgb> {
        unsupported()
    }
}

#[cfg(test)]
mod tests {
    use appytappy_config::anti_cheat::AntiCheatClass;

    use super::*;

    #[test]
    fn unsupported_system_fails_every_call() {
        let system = Unsupported;
        let entry = ProcessEntry {
            id: ProcessId::new(1),
            parent: ProcessId::new(0),
            executable: "example.exe".to_owned(),
            threads: 1,
        };
        let permit = ProcessAccessPermit::issue(entry.id, AntiCheatClass::None)
            .expect("class none permits access");
        let kinds = [
            system.processes().map(drop),
            system.windows().map(drop),
            system.ask_memory_access(&entry, &[]).map(drop),
            system.open(&permit).map(drop),
            system.screen_pixel(0, 0).map(drop),
        ]
        .map(|result| result.expect_err("unsupported").kind());
        assert_eq!(kinds, [io::ErrorKind::Unsupported; 5]);
    }
}
