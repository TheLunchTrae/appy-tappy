//! The window events that drive app detection.

use std::io;
use std::sync::mpsc::Sender;

use crate::host;

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
    pub fn start<E>(events: Sender<E>) -> io::Result<Self>
    where
        E: From<WindowEvent> + Send + 'static,
    {
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
