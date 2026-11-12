//! The guard that keeps a program to one running copy per user session.

use std::io;

use crate::host;

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

    /// Whether another program holds the guard called `name`, without taking
    /// it, so a check never stops that program from starting.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    pub fn is_held(name: &str) -> io::Result<bool> {
        host::InstanceGuard::is_held(name)
    }
}
