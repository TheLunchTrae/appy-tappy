//! The runtime's command line.
//!
//! With no arguments the runtime runs in the background, watching the
//! applications that have automations saved in the setup app.
//! `--autostart on` makes Windows start it that way at sign-in, and
//! `--autostart off` stops that; both exit at once.

use std::error::Error;
use std::ffi::OsString;
use std::fmt;

/// What the runtime was asked to do.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
    /// Run in the background.
    Run,
    /// Turn autostart at sign-in on (`true`) or off.
    Autostart(bool),
}

/// The command line made no sense.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct UsageError(String);

impl fmt::Display for UsageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for UsageError {}

fn usage(message: &str) -> UsageError {
    UsageError(message.to_owned())
}

/// Reads the arguments after the program name.
pub(crate) fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Command, UsageError> {
    let arguments: Vec<OsString> = arguments.into_iter().collect();
    let arguments: Vec<&str> = arguments
        .iter()
        .map(|argument| {
            argument
                .to_str()
                .ok_or_else(|| usage("an argument is not valid Unicode"))
        })
        .collect::<Result<_, _>>()?;
    match arguments.as_slice() {
        [] => Ok(Command::Run),
        ["--autostart", "on"] => Ok(Command::Autostart(true)),
        ["--autostart", "off"] => Ok(Command::Autostart(false)),
        ["--autostart"] | ["--autostart", _] => Err(usage("--autostart takes on or off")),
        _ => Err(usage(
            "the runtime takes no options other than --autostart on or --autostart off",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_strs(arguments: &[&str]) -> Result<Command, UsageError> {
        parse(arguments.iter().map(OsString::from))
    }

    #[test]
    fn no_arguments_run_the_runtime() {
        assert_eq!(parse_strs(&[]), Ok(Command::Run));
    }

    #[test]
    fn autostart_takes_on_or_off() {
        assert_eq!(
            parse_strs(&["--autostart", "on"]),
            Ok(Command::Autostart(true))
        );
        assert_eq!(
            parse_strs(&["--autostart", "off"]),
            Ok(Command::Autostart(false))
        );
    }

    #[test]
    fn malformed_command_lines_are_refused() {
        let other = "the runtime takes no options other than --autostart on or --autostart off";
        for (arguments, problem) in [
            (&["--autostart"][..], "--autostart takes on or off"),
            (&["--autostart", "maybe"], "--autostart takes on or off"),
            (&["--autostart", "on", "--autostart", "off"], other),
            (&["--watch", "Example.exe"], other),
            (&["--verbose"], other),
        ] {
            let error = parse_strs(arguments).expect_err("refused");
            assert_eq!(error.to_string(), problem, "{arguments:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn an_argument_that_is_not_unicode_is_refused() {
        use std::os::unix::ffi::OsStringExt;

        let unreadable = OsString::from_vec(vec![0xff]);
        assert_eq!(
            parse([OsString::from("--autostart"), unreadable]),
            Err(usage("an argument is not valid Unicode"))
        );
    }
}
