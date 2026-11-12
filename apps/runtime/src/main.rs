//! The always-on AppyTappy background program.
//!
//! It runs with no console window, so it reports only to its log in
//! `%LOCALAPPDATA%\AppyTappy\logs\`. See `wiki/Architecture.md` for
//! the runtime's threads and events, its lifecycle states, and the files it
//! writes.

#![forbid(unsafe_code)]
#![windows_subsystem = "windows"]

mod autostart;
mod cli;
mod click;
mod lifecycle;
mod model;
mod owner;
mod plan;
mod trace;
mod tray;
mod workers;

use std::env;
use std::io;
use std::process::ExitCode;

use appytappy_config::paths::Locations;
use appytappy_platform::{InstanceGuard, LOG_LIMIT, LocalTime, Log};

use crate::autostart::{autostart, autostart_line};
use crate::cli::Command;
use crate::owner::watch;

/// The exit code for a command line the runtime does not understand.
const USAGE: u8 = 2;

fn main() -> ExitCode {
    // The runtime has no console, so the log is the only place a problem
    // can show, even for a command that exits at once. It rotates only once
    // this copy holds the single-instance guard.
    let mut log = Locations::from_env()
        .map_err(io::Error::other)
        .and_then(|locations| Log::open(&locations.logs(), "runtime", u64::MAX));
    let command = match cli::parse(env::args_os().skip(1)) {
        Ok(command) => command,
        Err(error) => {
            if let Ok(log) = &mut log {
                note(log, &format!("command line refused: {error}"));
            }
            return ExitCode::from(USAGE);
        },
    };
    let result = match command {
        Command::Autostart(on) => {
            let result = autostart(on);
            if let Ok(log) = &mut log {
                note(log, &autostart_line(on, &result));
            }
            result
        },
        Command::Run => log.and_then(|mut log| run(&mut log)),
    };
    if result.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Runs in the background unless another copy already does, in which case
/// it notes that and exits at once.
fn run(log: &mut Log) -> io::Result<()> {
    let guard = InstanceGuard::acquire(InstanceGuard::RUNTIME);
    let _instance = match guard {
        Ok(Some(instance)) => instance,
        Ok(None) => {
            note(
                log,
                "another copy of the runtime is running, so this one exits",
            );
            return Ok(());
        },
        Err(error) => {
            note(log, &format!("the runtime could not start: {error}"));
            return Err(error);
        },
    };
    log.rotate_at(LOG_LIMIT);
    note(log, "runtime started");
    let result = Locations::from_env()
        .map_err(io::Error::other)
        .and_then(|locations| watch(&locations, log));
    match &result {
        Ok(()) => note(log, "runtime stopped"),
        Err(error) => note(log, &format!("runtime stopped: {error}")),
    }
    result
}

/// Writes a line to the log. The runtime keeps working when the log cannot
/// be written, because it has nowhere else to report.
fn note(log: &mut Log, message: &str) {
    #[expect(
        clippy::let_underscore_must_use,
        reason = "a full or locked log must not stop the runtime, and there is nowhere else to report it"
    )]
    let _ = log.write(LocalTime::now(), message);
}

#[cfg(all(test, not(windows)))]
mod tests {
    use std::fs;
    use std::process;

    use super::*;

    #[test]
    fn off_windows_the_runtime_refuses_to_run() {
        let folder = env::temp_dir().join(format!("appytappy-run-{}", process::id()));
        let mut log = Log::open(&folder, "runtime", LOG_LIMIT).expect("open");
        let ran = run(&mut log);
        let written = fs::read_to_string(folder.join("runtime.log")).expect("read");
        fs::remove_dir_all(&folder).expect("clean up");
        assert_eq!(
            ran.expect_err("unsupported").kind(),
            io::ErrorKind::Unsupported
        );
        assert!(
            written.contains("the runtime could not start"),
            "{written:?}"
        );
        for on in [true, false] {
            assert_eq!(
                autostart(on).expect_err("unsupported").kind(),
                io::ErrorKind::Unsupported
            );
        }
    }
}
