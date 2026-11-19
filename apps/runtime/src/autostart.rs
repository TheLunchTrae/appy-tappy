//! Turning autostart on and off from the command line, and the log line that
//! says how it went.

use std::env;
use std::io;

/// Turns autostart at sign-in on or off.
pub(crate) fn autostart(on: bool) -> io::Result<()> {
    if on {
        env::current_exe().and_then(|executable| appytappy_platform::enable_autostart(&executable))
    } else {
        appytappy_platform::disable_autostart()
    }
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
