//! The always-on AppyTappy background program.
//!
//! It runs with no console window and writes only to its log in
//! `%LOCALAPPDATA%\AppyTappy\logs\`. See `docs/architecture.md` for the
//! runtime's threads and events and its lifecycle states.

#![forbid(unsafe_code)]
#![windows_subsystem = "windows"]

mod cli;
mod lifecycle;
mod log;
mod model;

use std::env;
use std::io;
use std::process::ExitCode;
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use appytappy_config::paths::Locations;
use appytappy_platform::{AUTOSTART_VALUE, InstanceGuard, LocalTime, WindowWatch};
use appytappy_sources::HostSystem;
use appytappy_sources::system::System;

use crate::cli::{Autostart, Command};
use crate::lifecycle::{Change, Lifecycle, Watchlist};
use crate::log::Log;
use crate::model::Model;

/// The exit code for a command line the runtime does not understand.
const USAGE: u8 = 2;

fn main() -> ExitCode {
    let Ok(command) = cli::parse(env::args_os().skip(1)) else {
        return ExitCode::from(USAGE);
    };
    let result = match command {
        Command::Autostart(Autostart::On) => env::current_exe().and_then(|executable| {
            appytappy_platform::enable_autostart(AUTOSTART_VALUE, &executable)
        }),
        Command::Autostart(Autostart::Off) => {
            appytappy_platform::disable_autostart(AUTOSTART_VALUE)
        },
        Command::Run(executables) => run(Watchlist::new(
            executables
                .into_iter()
                .map(|executable| vec![executable])
                .collect(),
        )),
    };
    if result.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Runs in the background unless another copy already does, in which case
/// it exits at once.
fn run(watchlist: Watchlist) -> io::Result<()> {
    let Some(_instance) = InstanceGuard::acquire(InstanceGuard::RUNTIME)? else {
        return Ok(());
    };
    let folder = Locations::from_env().map_err(io::Error::other)?.logs();
    let mut log = Log::open(&folder, log::LIMIT)?;
    note(&mut log, "runtime started");
    let result = watch(watchlist, &mut log);
    match &result {
        Ok(()) => note(&mut log, "runtime stopped"),
        Err(error) => note(&mut log, &format!("runtime stopped: {error}")),
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

/// The owner loop: detection runs on its own threads and sends changes
/// here, where the root model applies them. It ends when the window watch
/// ends.
fn watch(watchlist: Watchlist, log: &mut Log) -> io::Result<()> {
    let (window_sender, window_events) = mpsc::channel();
    let windows = WindowWatch::start(window_sender)?;
    let (change_sender, changes) = mpsc::channel();
    let lifecycle = Lifecycle::new(watchlist.clone(), Instant::now());
    let system = HostSystem::default();
    let detector = thread::Builder::new()
        .name("lifecycle".to_owned())
        .spawn(move || {
            lifecycle::run(lifecycle, &window_events, &change_sender, || {
                system.processes()
            });
        })?;
    let mut model = Model::default();
    for change in changes {
        note(log, &describe(&watchlist, &change));
        if let Some(state) = model.apply(change) {
            note(log, &format!("state: {state}"));
        }
    }
    detector
        .join()
        .map_err(|_panic| io::Error::other("the lifecycle thread panicked"))?;
    windows.stop()
}

fn describe(watchlist: &Watchlist, change: &Change) -> String {
    match change {
        Change::Started { game, process } => {
            format!(
                "{} started (process {process}): attached",
                watchlist.name(*game)
            )
        },
        Change::Exited { game, process } => {
            format!(
                "{} exited (process {process}): detached",
                watchlist.name(*game)
            )
        },
        Change::ScanFailed(problem) => format!("process snapshots fail: {problem}"),
        Change::ScanRestored => "process snapshots work again".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process;

    use appytappy_config::anti_cheat::ProcessId;

    use super::*;
    use crate::lifecycle::GameIndex;

    #[test]
    fn a_note_is_a_line_in_the_log() {
        let folder = env::temp_dir().join(format!("appytappy-note-{}", process::id()));
        let mut log = Log::open(&folder, log::LIMIT).expect("open");
        note(&mut log, "runtime started");
        let written = fs::read_to_string(folder.join("runtime.log")).expect("read");
        fs::remove_dir_all(&folder).expect("clean up");
        assert!(written.ends_with("runtime started\n"), "{written:?}");
    }

    #[cfg(not(windows))]
    #[test]
    fn off_windows_the_runtime_refuses_to_run() {
        let folder = env::temp_dir().join(format!("appytappy-watch-{}", process::id()));
        let mut log = Log::open(&folder, log::LIMIT).expect("open");
        let watched = watch(Watchlist::new(Vec::new()), &mut log);
        fs::remove_dir_all(&folder).expect("clean up");
        assert_eq!(
            watched.expect_err("unsupported").kind(),
            io::ErrorKind::Unsupported
        );
        let ran = run(Watchlist::new(Vec::new()));
        assert_eq!(
            ran.expect_err("unsupported").kind(),
            io::ErrorKind::Unsupported
        );
    }

    #[test]
    fn log_lines_name_the_game_and_process() {
        let watchlist = Watchlist::new(vec![vec!["GeometryDash.exe".to_owned()]]);
        let game = GameIndex(0);
        let process = ProcessId::new(42);
        let lines = [
            Change::Started { game, process },
            Change::Exited { game, process },
            Change::ScanFailed("denied".to_owned()),
            Change::ScanRestored,
        ]
        .map(|change| describe(&watchlist, &change));
        assert_eq!(
            lines,
            [
                "GeometryDash.exe started (process 42): attached",
                "GeometryDash.exe exited (process 42): detached",
                "process snapshots fail: denied",
                "process snapshots work again",
            ]
        );
    }
}
