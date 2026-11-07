//! The runtime's command line.
//!
//! With no arguments the runtime runs in the background. `--autostart on`
//! and `--autostart off` change whether Windows starts it at sign-in, then
//! exit. Until the compiled config names the games to watch, each
//! `--watch <executable>` names one.

use std::error::Error;
use std::ffi::OsString;
use std::fmt;

/// What the runtime was asked to do.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
    /// Run in the background, watching these executables, one game each.
    Run(Vec<String>),
    /// Turn autostart at sign-in on or off.
    Autostart(Autostart),
}

/// Whether Windows starts the runtime at sign-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Autostart {
    /// Start the runtime at sign-in.
    On,
    /// Do not start the runtime at sign-in.
    Off,
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
    let mut arguments = arguments.into_iter().map(|argument| {
        argument
            .into_string()
            .map_err(|_unreadable| usage("an argument is not valid Unicode"))
    });
    let mut watched = Vec::new();
    let mut autostart = None;
    while let Some(argument) = arguments.next() {
        let argument = argument?;
        let value = arguments
            .next()
            .transpose()?
            .ok_or_else(|| usage("an option is missing its value"))?;
        match argument.as_str() {
            "--watch" => watched.push(value),
            "--autostart" if autostart.is_none() => {
                autostart = Some(match value.as_str() {
                    "on" => Autostart::On,
                    "off" => Autostart::Off,
                    _ => return Err(usage("--autostart takes on or off")),
                });
            },
            _ => return Err(usage("unknown option")),
        }
    }
    match autostart {
        None => Ok(Command::Run(watched)),
        Some(setting) if watched.is_empty() => Ok(Command::Autostart(setting)),
        Some(_) => Err(usage("--autostart cannot be combined with --watch")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_strs(arguments: &[&str]) -> Result<Command, UsageError> {
        parse(arguments.iter().map(OsString::from))
    }

    #[test]
    fn no_arguments_runs_watching_nothing() {
        assert_eq!(parse_strs(&[]), Ok(Command::Run(Vec::new())));
    }

    #[test]
    fn each_watch_names_one_executable() {
        assert_eq!(
            parse_strs(&["--watch", "GeometryDash.exe", "--watch", "Other.exe"]),
            Ok(Command::Run(vec![
                "GeometryDash.exe".to_owned(),
                "Other.exe".to_owned()
            ]))
        );
    }

    #[test]
    fn autostart_takes_on_or_off() {
        assert_eq!(
            parse_strs(&["--autostart", "on"]),
            Ok(Command::Autostart(Autostart::On))
        );
        assert_eq!(
            parse_strs(&["--autostart", "off"]),
            Ok(Command::Autostart(Autostart::Off))
        );
    }

    #[test]
    fn malformed_command_lines_are_refused() {
        for arguments in [
            &["--watch"][..],
            &["--autostart", "maybe"],
            &["--autostart", "on", "--autostart", "off"],
            &["--autostart", "on", "--watch", "Game.exe"],
            &["--watch", "Game.exe", "--autostart", "on"],
            &["--verbose", "yes"],
        ] {
            assert!(parse_strs(arguments).is_err(), "{arguments:?}");
        }
        assert_eq!(
            parse_strs(&["--verbose", "yes"])
                .expect_err("unknown")
                .to_string(),
            "unknown option"
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_argument_that_is_not_unicode_is_refused() {
        use std::os::unix::ffi::OsStringExt;

        let unreadable = OsString::from_vec(vec![0xff]);
        assert!(parse([OsString::from("--watch"), unreadable]).is_err());
    }
}
