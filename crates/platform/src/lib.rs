//! The Windows host services that the runtime and the setup app use: the
//! window events that drive app detection, the tray icon and its menu, the
//! single-instance guard, the reload requests the setup app sends, autostart
//! at sign-in, the local clock, a background thread's priority, and showing
//! the setup app's overlay windows without the focus.
//!
//! Each service turns Windows API calls into a typed value or an error and
//! makes no decisions of its own. The Windows adapter lives in `win`; other
//! platforms get services that fail with [`std::io::ErrorKind::Unsupported`], so
//! the portable code that uses them still builds and tests there. Both
//! adapters define the same items, which this crate exports under one name
//! each.
//!
//! See `wiki/Architecture.md` for this crate's boundaries.

use std::any::Any;

#[cfg(not(windows))]
mod unsupported;
#[cfg(windows)]
mod win;

#[cfg(not(windows))]
use unsupported as host;
#[cfg(windows)]
use win as host;

mod autostart;
mod clock;
mod log;
mod tray;

pub use autostart::{autostart_enabled, disable_autostart, enable_autostart};
pub use clock::LocalTime;
pub use host::{
    InstanceGuard, ReloadWatch, Tray, WindowWatch, lower_thread_priority, process_runs_in_session,
    request_reload, show_without_focus,
};
pub use log::{LOG_LIMIT, Log};
pub use tray::{MenuItem, TrayEvent, TrayMenu};

/// A change to the set of windows on the desktop, which [`WindowWatch`]
/// reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowEvent {
    /// A top-level window was created.
    Created,
    /// A window was destroyed. Windows cannot tell whether a destroyed
    /// window was top-level, so this covers child windows too.
    Destroyed,
}

/// A request to read the saved files again, which [`ReloadWatch`] reports
/// after each [`request_reload`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReloadRequest;

/// The message a panicking thread left, for the error that reports it.
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the Windows adapter runs threads of its own")
)]
pub(crate) fn panic_reason(payload: &(dyn Any + Send)) -> &str {
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("an unknown reason")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn showing_a_window_that_does_not_exist_fails() {
        assert!(show_without_focus(0).is_err());
    }

    #[test]
    fn a_panic_reason_is_its_message() {
        let fixed: Box<dyn Any + Send> = Box::new("fixed");
        let formatted: Box<dyn Any + Send> = Box::new(format!("formatted {}", 1));
        let other: Box<dyn Any + Send> = Box::new(1_u8);
        let reasons =
            [fixed, formatted, other].map(|payload| panic_reason(payload.as_ref()).to_owned());
        assert_eq!(reasons, ["fixed", "formatted 1", "an unknown reason"]);
    }
}
