//! Turning autostart on and off from the command line, and the log line that
//! says how it went.

use std::env;
use std::ffi::OsString;
use std::io;
use std::path::Path;

use appytappy_platform::AUTOSTART_VALUE;

/// Turns autostart at sign-in on or off.
pub(crate) fn autostart(on: bool) -> io::Result<()> {
    if on {
        env::current_exe().and_then(|executable| {
            appytappy_platform::enable_autostart(AUTOSTART_VALUE, &autostart_command(&executable))
        })
    } else {
        appytappy_platform::disable_autostart(AUTOSTART_VALUE)
    }
}

/// The command line Windows runs at sign-in: the program, quoted because a
/// path with spaces would otherwise run its first word.
fn autostart_command(executable: &Path) -> OsString {
    let mut command = OsString::from("\"");
    command.push(executable);
    command.push("\"");
    command
}

/// The log line for an autostart change.
pub(crate) fn autostart_line(on: bool, result: &io::Result<()>) -> String {
    let change = if on { "autostart on" } else { "autostart off" };
    match result {
        Ok(()) => change.to_owned(),
        Err(error) => format!("{change} failed: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autostart_runs_the_quoted_program() {
        let command = autostart_command(Path::new(
            "C:/Program Files/AppyTappy/appytappy-runtime.exe",
        ));
        assert_eq!(
            command,
            OsString::from(r#""C:/Program Files/AppyTappy/appytappy-runtime.exe""#)
        );
    }

    #[test]
    fn autostart_changes_are_logged_with_their_result() {
        let failed = Err(io::Error::other("denied"));
        assert_eq!(autostart_line(true, &Ok(())), "autostart on");
        assert_eq!(autostart_line(false, &Ok(())), "autostart off");
        assert_eq!(
            autostart_line(false, &failed),
            "autostart off failed: denied"
        );
    }
}
