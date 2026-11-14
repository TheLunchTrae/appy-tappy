//! The requests the setup app sends the runtime: to read the saved files
//! again after each save that changes what the runtime runs, and to run one
//! automation's actions once.

use std::io;
use std::sync::mpsc::Sender;

use crate::host;

/// What the names of the runtime's request events start with. Each
/// [`Request`] has its own event, named for it after this.
pub const REQUESTS: &str = r"Local\AppyTappy";

/// What the setup app asks of the runtime. A request carries nothing: what
/// it is about is in the saved files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    /// Read the saved files again, after a save that changes what the
    /// runtime runs.
    Reload,
    /// Run one automation's actions once: the automation that the saved
    /// request names.
    RunOnce,
}

impl Request {
    /// The name of this request's event among those that start with
    /// `scope`, such as [`REQUESTS`].
    pub(crate) fn event(self, scope: &str) -> String {
        let name = match self {
            Self::Reload => "Reload",
            Self::RunOnce => "RunOnce",
        };
        format!("{scope}.{name}")
    }
}

/// A thread that sends a [`Request`] each time another program calls
/// [`request`] with the same scope.
///
/// Requests of one kind that arrive while one waits to be sent become one
/// request, and of two kinds that wait together the reload is sent first.
/// It never polls: the thread sleeps until a request or
/// [`RequestWatch::stop`].
#[derive(Debug)]
pub struct RequestWatch(host::RequestWatch);

impl RequestWatch {
    /// Starts waiting for the requests to `scope`, such as [`REQUESTS`],
    /// and returns once the thread waits.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error when the thread or its events
    /// cannot start.
    pub fn start<E>(scope: &str, events: Sender<E>) -> io::Result<Self>
    where
        E: From<Request> + Send + 'static,
    {
        host::RequestWatch::start(scope, events).map(Self)
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

/// Sends `request` to the program that watches `scope`, such as
/// [`REQUESTS`]. Returns whether one was watching; when none is, nothing
/// happens.
///
/// # Errors
///
/// Returns the operating system's error.
pub fn request(scope: &str, request: Request) -> io::Result<bool> {
    host::request(&request.event(scope))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_request_has_its_own_event() {
        assert_eq!(
            [Request::Reload, Request::RunOnce].map(|request| request.event(REQUESTS)),
            [r"Local\AppyTappy.Reload", r"Local\AppyTappy.RunOnce"]
        );
    }
}
