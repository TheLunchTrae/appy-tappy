//! Starting the runtime when the user signs in.

use std::ffi::OsString;
use std::io;
use std::path::Path;

use crate::host;

/// The value under the user's `Run` registry key that starts the runtime at
/// sign-in.
///
/// It is the installer's product name too, because Tauri's uninstaller
/// removes the `Run` value named for the product.
const AUTOSTART_VALUE: &str = "AppyTappy";

/// Makes Windows run the runtime at `runtime` when the user signs in. It
/// replaces the command any earlier call wrote.
///
/// # Errors
///
/// Returns the operating system's error.
pub fn enable_autostart(runtime: &Path) -> io::Result<()> {
    host::enable_autostart(AUTOSTART_VALUE, &command(runtime))
}

/// Stops Windows running the runtime when the user signs in. Autostart that
/// is already off is not an error.
///
/// # Errors
///
/// Returns the operating system's error.
pub fn disable_autostart() -> io::Result<()> {
    host::disable_autostart(AUTOSTART_VALUE)
}

/// Whether Windows runs the runtime when the user signs in.
///
/// # Errors
///
/// Returns the operating system's error.
pub fn autostart_enabled() -> io::Result<bool> {
    host::autostart_enabled(AUTOSTART_VALUE)
}

/// The command line Windows runs at sign-in: the program, quoted because a
/// path with spaces would otherwise run its first word.
fn command(runtime: &Path) -> OsString {
    let mut command = OsString::from("\"");
    command.push(runtime);
    command.push("\"");
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autostart_runs_the_quoted_program() {
        let command = command(Path::new(
            "C:/Program Files/AppyTappy/appytappy-runtime.exe",
        ));
        assert_eq!(
            command,
            OsString::from(r#""C:/Program Files/AppyTappy/appytappy-runtime.exe""#)
        );
    }
}
