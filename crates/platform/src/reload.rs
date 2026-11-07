//! The reload requests the setup app sends the runtime after each save.

use std::io;
use std::sync::mpsc::Sender;

use crate::host;

/// The name of the event the runtime waits on for reload requests. The
/// setup app sets it after each save, so the runtime reads the saved files
/// again.
pub const RELOAD: &str = r"Local\AppyTappy.Reload";

/// A request to read the saved files again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReloadRequest;

/// A thread that sends a [`ReloadRequest`] each time another program calls
/// [`request_reload`] with the same name.
///
/// Requests that arrive while one waits to be sent become one request. It
/// never polls: the thread sleeps until a request or [`ReloadWatch::stop`].
#[derive(Debug)]
pub struct ReloadWatch(host::ReloadWatch);

impl ReloadWatch {
    /// Starts waiting for requests to `name`, such as [`RELOAD`], and returns
    /// once the thread waits.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error when the thread or its event
    /// cannot start.
    pub fn start<E>(name: &str, events: Sender<E>) -> io::Result<Self>
    where
        E: From<ReloadRequest> + Send + 'static,
    {
        host::ReloadWatch::start(name, events).map(Self)
    }

    /// Ends the thread and waits for it.
    ///
    /// # Errors
    ///
    /// Returns the error that ended the thread's wait, if any.
    // Off Windows no watch exists, so no test there can reach this body; the
    // Windows adapter's tests call it.
    #[mutants::skip]
    pub fn stop(self) -> io::Result<()> {
        self.0.stop()
    }
}

/// Asks the program waiting on `name`, such as [`RELOAD`], to reload.
/// Returns whether one was waiting; when none is, nothing happens.
///
/// # Errors
///
/// Returns the operating system's error.
pub fn request_reload(name: &str) -> io::Result<bool> {
    host::request_reload(name)
}
