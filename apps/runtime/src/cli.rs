//! The runtime's command line.
//!
//! Until the compiled config names the apps to watch, each
//! `--watch <executable>` names one, and the runtime runs in the background
//! watching them. `--autostart on` with the same `--watch` options makes
//! Windows start it that way at sign-in, and `--autostart off` stops that;
//! both exit at once.

use std::error::Error;
use std::ffi::OsString;
use std::fmt;

/// What the runtime was asked to do.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
    /// Run in the background, watching these executables, one app each.
    Run(Vec<String>),
    /// Turn autostart at sign-in on or off.
    Autostart(Autostart),
}

/// Whether Windows starts the runtime at sign-in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Autostart {
    /// Start the runtime at sign-in, watching these executables.
    On(Vec<String>),
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
            // Process snapshots list bare file names, so a path never matches.
            "--watch" if value.is_empty() || value.contains(['\\', '/', ':', '"']) => {
                return Err(usage(
                    "--watch takes a file name such as Example.exe, not a path",
                ));
            },
            "--watch" => watched.push(value),
            "--autostart" if autostart.is_none() => {
                autostart = Some(match value.as_str() {
                    "on" => true,
                    "off" => false,
                    _ => return Err(usage("--autostart takes on or off")),
                });
            },
            _ => return Err(usage("unknown option")),
        }
    }
    match (autostart, watched.is_empty()) {
        (None, false) => Ok(Command::Run(watched)),
        (Some(true), false) => Ok(Command::Autostart(Autostart::On(watched))),
        (Some(false), true) => Ok(Command::Autostart(Autostart::Off)),
        (None | Some(true), true) => Err(usage("name an application to watch with --watch")),
        (Some(false), false) => Err(usage("--autostart off takes no --watch")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_strs(arguments: &[&str]) -> Result<Command, UsageError> {
        parse(arguments.iter().map(OsString::from))
    }

    #[test]
    fn each_watch_names_one_executable() {
        assert_eq!(
            parse_strs(&["--watch", "Example.exe", "--watch", "Other.exe"]),
            Ok(Command::Run(vec![
                "Example.exe".to_owned(),
                "Other.exe".to_owned()
            ]))
        );
    }

    #[test]
    fn autostart_takes_on_with_apps_or_off() {
        assert_eq!(
            parse_strs(&["--autostart", "on", "--watch", "Example.exe"]),
            Ok(Command::Autostart(Autostart::On(vec![
                "Example.exe".to_owned()
            ])))
        );
        assert_eq!(
            parse_strs(&["--watch", "Example.exe", "--autostart", "on"]),
            Ok(Command::Autostart(Autostart::On(vec![
                "Example.exe".to_owned()
            ])))
        );
        assert_eq!(
            parse_strs(&["--autostart", "off"]),
            Ok(Command::Autostart(Autostart::Off))
        );
    }

    #[test]
    fn malformed_command_lines_are_refused() {
        for (arguments, problem) in [
            (&[][..], "name an application to watch with --watch"),
            (
                &["--autostart", "on"],
                "name an application to watch with --watch",
            ),
            (&["--watch"], "an option is missing its value"),
            (&["--autostart", "maybe"], "--autostart takes on or off"),
            (
                &["--autostart", "on", "--autostart", "off"],
                "unknown option",
            ),
            (
                &["--autostart", "off", "--watch", "Example.exe"],
                "--autostart off takes no --watch",
            ),
            (&["--verbose", "yes"], "unknown option"),
        ] {
            let error = parse_strs(arguments).expect_err("refused");
            assert_eq!(error.to_string(), problem, "{arguments:?}");
        }
    }

    #[test]
    fn a_watched_app_is_a_bare_file_name() {
        for value in [
            "",
            r"C:\Apps\Example.exe",
            "Apps/Example.exe",
            "C:Example.exe",
            "\"Example.exe",
        ] {
            assert_eq!(
                parse_strs(&["--watch", value]),
                Err(usage(
                    "--watch takes a file name such as Example.exe, not a path"
                )),
                "{value:?}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn an_argument_that_is_not_unicode_is_refused() {
        use std::os::unix::ffi::OsStringExt;

        let unreadable = OsString::from_vec(vec![0xff]);
        assert!(parse([OsString::from("--watch"), unreadable]).is_err());
    }
}
