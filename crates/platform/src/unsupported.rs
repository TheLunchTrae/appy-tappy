//! The host services of a platform AppyTappy does not support: every call
//! fails with [`io::ErrorKind::Unsupported`].

#![forbid(unsafe_code)]

use std::convert::Infallible;
use std::ffi::OsStr;
use std::io;
use std::sync::mpsc::Sender;

use crate::{LocalTime, Request, TrayEvent, TrayMenu, WindowEvent};

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
    pub(crate) fn start<E>(_events: Sender<E>) -> io::Result<Self>
    where
        E: From<WindowEvent> + Send + 'static,
    {
        unsupported()
    }

    // `WindowWatch` holds `Infallible`, so no value exists and this body
    // never runs. No test can call it, so mutating it cannot be caught.
    #[mutants::skip]
    pub(crate) fn stop(self) -> io::Result<()> {
        match self.0 {}
    }
}

/// A tray icon, which cannot be constructed here.
#[derive(Debug)]
pub(crate) struct Tray(Infallible);

impl Tray {
    pub(crate) fn start<E>(_events: Sender<E>, _menu: TrayMenu) -> io::Result<Self>
    where
        E: From<TrayEvent> + Send + 'static,
    {
        unsupported()
    }

    // `Tray` holds `Infallible`, so no value exists and this body never runs.
    #[mutants::skip]
    pub(crate) fn show(&self, _menu: TrayMenu) -> io::Result<()> {
        match self.0 {}
    }

    // `Tray` holds `Infallible`, so no value exists and this body never runs.
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

/// A request watch, which cannot be constructed here.
#[derive(Debug)]
pub(crate) struct RequestWatch(Infallible);

impl RequestWatch {
    pub(crate) fn start<E>(_scope: &str, _events: Sender<E>) -> io::Result<Self>
    where
        E: From<Request> + Send + 'static,
    {
        unsupported()
    }

    // `RequestWatch` holds `Infallible`, so no value exists and this body
    // never runs.
    #[mutants::skip]
    pub(crate) fn stop(self) -> io::Result<()> {
        match self.0 {}
    }
}

pub(crate) fn request(_name: &str) -> io::Result<bool> {
    unsupported()
}

pub(crate) fn enable_autostart(_value: &str, _command: &OsStr) -> io::Result<()> {
    unsupported()
}

pub(crate) fn disable_autostart(_value: &str) -> io::Result<()> {
    unsupported()
}

pub(crate) fn autostart_enabled(_value: &str) -> io::Result<bool> {
    unsupported()
}

pub(crate) fn show_without_focus(_window: usize) -> io::Result<()> {
    unsupported()
}

pub(crate) fn lower_thread_priority() -> io::Result<()> {
    unsupported()
}

pub(crate) const fn local_time() -> Option<LocalTime> {
    None
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use crate::{InstanceGuard, RequestWatch, Tray, WindowWatch};

    use super::*;

    #[test]
    fn every_service_is_unsupported() {
        let (events, _received) = mpsc::channel::<WindowEvent>();
        let (tray_events, _tray_received) = mpsc::channel::<TrayEvent>();
        let (requests, _requests_received) = mpsc::channel::<Request>();
        let menu = TrayMenu {
            tooltip: "AppyTappy".to_owned(),
            items: Vec::new(),
        };
        let kinds = [
            WindowWatch::start(events).map(drop),
            Tray::start(tray_events, menu).map(drop),
            InstanceGuard::acquire(InstanceGuard::RUNTIME).map(drop),
            RequestWatch::start(crate::REQUESTS, requests).map(drop),
            crate::request(crate::REQUESTS, Request::RunOnce).map(drop),
            crate::enable_autostart("test", OsStr::new("runtime.exe")),
            crate::disable_autostart("test"),
            crate::autostart_enabled("test").map(drop),
            crate::lower_thread_priority(),
        ]
        .map(|result| result.expect_err("unsupported").kind());
        assert_eq!(kinds, [io::ErrorKind::Unsupported; 9]);
        assert_eq!(LocalTime::now(), None);
    }
}
