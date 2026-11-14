//! The runtime's Windows host services: the window events that drive app
//! detection, the tray icon and its menu, the single-instance guard, the
//! reload requests the setup app sends, autostart at sign-in, the local
//! clock, a background thread's priority, and showing the setup app's overlay windows without the focus.
//!
//! Each service turns Windows API calls into a typed value or an error and
//! makes no decisions of its own. The Windows adapter lives in `win`; other
//! platforms get services that fail with [`io::ErrorKind::Unsupported`], so
//! the portable code that uses them still builds and tests there.
//!
//! See `wiki/Architecture.md` for this crate's boundaries.

use std::any::Any;
use std::io;

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
mod instance;
mod log;
mod reload;
mod tray;
mod window_watch;

pub use autostart::{AUTOSTART_VALUE, autostart_enabled, disable_autostart, enable_autostart};
pub use clock::LocalTime;
pub use instance::InstanceGuard;
pub use log::{LOG_LIMIT, Log};
pub use reload::{RELOAD, ReloadRequest, ReloadWatch, request_reload};
pub use tray::{MenuItem, Tray, TrayEvent, TrayMenu};
pub use window_watch::{WindowEvent, WindowWatch};

/// Shows the window with the handle `window` without activating it.
///
/// The window the user is typing in keeps the keyboard, and the window
/// stays where it is. The setup app shows its overlay windows this way,
/// since the window library it uses activates a window it shows.
///
/// # Errors
///
/// Returns the operating system's error, such as for a handle that names
/// no window.
pub fn show_without_focus(window: usize) -> io::Result<()> {
    host::show_without_focus(window)
}

/// Lowers the calling thread's priority as far as it goes, for work that
/// must never take time from the rest of the program, such as writing the
/// runtime trace.
///
/// # Errors
///
/// Returns the operating system's error.
pub fn lower_thread_priority() -> io::Result<()> {
    host::lower_thread_priority()
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
