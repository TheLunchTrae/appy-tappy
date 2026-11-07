//! The host services of a platform AppyTappy does not support: every call
//! fails with [`io::ErrorKind::Unsupported`].

#![forbid(unsafe_code)]

use std::convert::Infallible;
use std::ffi::OsStr;
use std::io;
use std::sync::mpsc::Sender;

use crate::{LocalTime, WindowEvent};

fn unsupported<T>() -> io::Result<T> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "the AppyTappy runtime runs on Windows only",
    ))
}

/// A window watch, which cannot be constructed here.
#[derive(Debug)]
pub(crate) struct WindowWatch(Infallible);

impl WindowWatch {
    pub(crate) fn start(_events: Sender<WindowEvent>) -> io::Result<Self> {
        unsupported()
    }

    // `WindowWatch` holds `Infallible`, so no value exists and this body
    // never runs. No test can call it, so mutating it cannot be caught.
    #[mutants::skip]
    pub(crate) fn stop(self) -> io::Result<()> {
        match self.0 {}
    }
}

/// An instance guard, which cannot be constructed here.
#[derive(Debug)]
pub(crate) struct InstanceGuard(Infallible);

impl InstanceGuard {
    pub(crate) fn acquire(_name: &str) -> io::Result<Option<Self>> {
        unsupported()
    }
}

pub(crate) fn enable_autostart(_value: &str, _command: &OsStr) -> io::Result<()> {
    unsupported()
}

pub(crate) fn disable_autostart(_value: &str) -> io::Result<()> {
    unsupported()
}

pub(crate) const fn local_time() -> Option<LocalTime> {
    None
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use crate::{InstanceGuard, WindowWatch};

    use super::*;

    #[test]
    fn every_service_is_unsupported() {
        let (events, _received) = mpsc::channel();
        let kinds = [
            WindowWatch::start(events).map(drop),
            InstanceGuard::acquire(InstanceGuard::RUNTIME).map(drop),
            crate::enable_autostart("test", OsStr::new("runtime.exe")),
            crate::disable_autostart("test"),
        ]
        .map(|result| result.expect_err("unsupported").kind());
        assert_eq!(kinds, [io::ErrorKind::Unsupported; 4]);
        assert_eq!(LocalTime::now(), None);
    }
}
