//! The host services of a platform AppyTappy does not support: every call
//! fails with [`io::ErrorKind::Unsupported`], and no service value can be
//! made. The Windows adapter in `win` documents what each one does there.

#![forbid(unsafe_code)]

use std::convert::Infallible;
use std::ffi::OsStr;
use std::io;
use std::sync::mpsc::Sender;

use crate::{LocalTime, ReloadRequest, TrayEvent, TrayMenu, WindowEvent};

fn unsupported<T>() -> io::Result<T> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "the AppyTappy runtime runs on Windows only",
    ))
}

/// The window event watch, which cannot start here.
#[derive(Debug)]
pub struct WindowWatch(Infallible);

impl WindowWatch {
    /// Fails, since there are no window events to watch here.
    ///
    /// # Errors
    ///
    /// Always returns [`io::ErrorKind::Unsupported`].
    pub fn start<E>(_events: Sender<E>) -> io::Result<Self>
    where
        E: From<WindowEvent> + Send + 'static,
    {
        unsupported()
    }

    /// Never runs, since no watch exists here.
    ///
    /// # Errors
    ///
    /// None, since it never runs.
    // `WindowWatch` holds `Infallible`, so no value exists and this body
    // never runs. No test can call it, so mutating it cannot be caught.
    #[mutants::skip]
    pub fn stop(self) -> io::Result<()> {
        match self.0 {}
    }
}

/// The tray icon, which cannot start here.
#[derive(Debug)]
pub struct Tray(Infallible);

impl Tray {
    /// Fails, since there is no tray here.
    ///
    /// # Errors
    ///
    /// Always returns [`io::ErrorKind::Unsupported`].
    pub fn start<E>(_events: Sender<E>, _menu: TrayMenu) -> io::Result<Self>
    where
        E: From<TrayEvent> + Send + 'static,
    {
        unsupported()
    }

    /// Never runs, since no tray exists here.
    ///
    /// # Errors
    ///
    /// None, since it never runs.
    // `Tray` holds `Infallible`, so no value exists and this body never runs.
    #[mutants::skip]
    pub fn show(&self, _menu: TrayMenu) -> io::Result<()> {
        match self.0 {}
    }

    /// Never runs, since no tray exists here.
    ///
    /// # Errors
    ///
    /// None, since it never runs.
    // `Tray` holds `Infallible`, so no value exists and this body never runs.
    #[mutants::skip]
    pub fn stop(self) -> io::Result<()> {
        match self.0 {}
    }
}

/// Fails, since there are no sessions to list here.
///
/// # Errors
///
/// Always returns [`io::ErrorKind::Unsupported`].
pub fn process_runs_in_session(_executable: &str) -> io::Result<bool> {
    unsupported()
}

/// The runtime's single-instance guard, which cannot be taken here.
#[derive(Debug)]
pub struct InstanceGuard(Infallible);

impl InstanceGuard {
    /// Fails, since the runtime does not run here.
    ///
    /// # Errors
    ///
    /// Always returns [`io::ErrorKind::Unsupported`].
    pub fn acquire() -> io::Result<Option<Self>> {
        unsupported()
    }
}

/// The reload request watch, which cannot start here.
#[derive(Debug)]
pub struct ReloadWatch(Infallible);

impl ReloadWatch {
    /// Fails, since the runtime does not run here.
    ///
    /// # Errors
    ///
    /// Always returns [`io::ErrorKind::Unsupported`].
    pub fn start<E>(_events: Sender<E>) -> io::Result<Self>
    where
        E: From<ReloadRequest> + Send + 'static,
    {
        unsupported()
    }

    /// Never runs, since no watch exists here.
    ///
    /// # Errors
    ///
    /// None, since it never runs.
    // `ReloadWatch` holds `Infallible`, so no value exists and this body
    // never runs.
    #[mutants::skip]
    pub fn stop(self) -> io::Result<()> {
        match self.0 {}
    }
}

/// Fails, since the runtime does not run here.
///
/// # Errors
///
/// Always returns [`io::ErrorKind::Unsupported`].
pub fn request_reload() -> io::Result<bool> {
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

/// Fails, since there are no windows to show here.
///
/// # Errors
///
/// Always returns [`io::ErrorKind::Unsupported`].
pub fn show_without_focus(_window: usize) -> io::Result<()> {
    unsupported()
}

/// Fails, since the runtime does not run here.
///
/// # Errors
///
/// Always returns [`io::ErrorKind::Unsupported`].
pub fn lower_thread_priority() -> io::Result<()> {
    unsupported()
}

impl LocalTime {
    /// `None`, since AppyTappy does not run here.
    #[must_use]
    pub const fn now() -> Option<Self> {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::mpsc;

    use super::*;

    #[test]
    fn every_service_is_unsupported() {
        let (events, _received) = mpsc::channel::<WindowEvent>();
        let (tray_events, _tray_received) = mpsc::channel::<TrayEvent>();
        let (reloads, _reload_received) = mpsc::channel::<ReloadRequest>();
        let menu = TrayMenu {
            tooltip: "AppyTappy".to_owned(),
            items: Vec::new(),
        };
        let kinds = [
            WindowWatch::start(events).map(drop),
            Tray::start(tray_events, menu).map(drop),
            crate::process_runs_in_session("runtime.exe").map(drop),
            InstanceGuard::acquire().map(drop),
            ReloadWatch::start(reloads).map(drop),
            crate::request_reload().map(drop),
            crate::enable_autostart(Path::new("runtime.exe")),
            crate::disable_autostart(),
            crate::autostart_enabled().map(drop),
            crate::lower_thread_priority(),
        ]
        .map(|result| result.expect_err("unsupported").kind());
        assert_eq!(kinds, [io::ErrorKind::Unsupported; 10]);
        assert_eq!(LocalTime::now(), None);
    }
}
