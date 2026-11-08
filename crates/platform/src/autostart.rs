//! Starting the runtime when the user signs in.

use std::ffi::OsStr;
use std::io;

use crate::host;

/// The value under the user's `Run` registry key that starts the runtime at
/// sign-in.
///
/// It is the installer's product name too, because Tauri's uninstaller
/// removes the `Run` value named for the product.
pub const AUTOSTART_VALUE: &str = "AppyTappy";

/// Makes Windows run `command` when the user signs in, through the `Run`
/// key value `value`. It replaces any command the value held. The command
/// is a whole command line, with the program's path quoted.
///
/// # Errors
///
/// Returns the operating system's error.
pub fn enable_autostart(value: &str, command: &OsStr) -> io::Result<()> {
    host::enable_autostart(value, command)
}

/// Removes the `Run` key value `value`. A value that is already absent is
/// not an error.
///
/// # Errors
///
/// Returns the operating system's error.
pub fn disable_autostart(value: &str) -> io::Result<()> {
    host::disable_autostart(value)
}

/// Whether the `Run` key holds the value `value`, so Windows runs its
/// command when the user signs in.
///
/// # Errors
///
/// Returns the operating system's error.
pub fn autostart_enabled(value: &str) -> io::Result<bool> {
    host::autostart_enabled(value)
}
