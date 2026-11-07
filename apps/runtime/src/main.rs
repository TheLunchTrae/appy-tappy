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
mod tray;

use std::env;
use std::ffi::OsString;
use std::io;
use std::ops::ControlFlow;
use std::path::Path;
use std::process::ExitCode;
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use appytappy_config::paths::Locations;
use appytappy_platform::{AUTOSTART_VALUE, InstanceGuard, LocalTime, Tray, TrayEvent, WindowWatch};
use appytappy_sources::HostSystem;
use appytappy_sources::system::System;

use crate::cli::{Autostart, Command};
use crate::lifecycle::{Change, Lifecycle, Watchlist};
use crate::log::Log;
use crate::model::Model;

/// The exit code for a command line the runtime does not understand.
const USAGE: u8 = 2;

fn main() -> ExitCode {
    // The runtime has no console, so the log is the only place a problem
    // can show, even for a command that exits at once. It rotates only once
    // this copy holds the single-instance guard.
    let mut log = Locations::from_env()
        .map_err(io::Error::other)
        .and_then(|locations| Log::open(&locations.logs(), u64::MAX));
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
        Command::Autostart(setting) => {
            let result = autostart(&setting);
            if let Ok(log) = &mut log {
                note(log, &autostart_line(&setting, &result));
            }
            result
        },
        Command::Run(executables) => log.and_then(|mut log| {
            let apps = executables
                .into_iter()
                .map(|executable| vec![executable])
                .collect();
            run(Watchlist::new(apps), &mut log)
        }),
    };
    if result.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Turns autostart at sign-in on or off.
fn autostart(setting: &Autostart) -> io::Result<()> {
    match setting {
        Autostart::On(watched) => env::current_exe().and_then(|executable| {
            appytappy_platform::enable_autostart(
                AUTOSTART_VALUE,
                &autostart_command(&executable, watched),
            )
        }),
        Autostart::Off => appytappy_platform::disable_autostart(AUTOSTART_VALUE),
    }
}

/// The command line Windows runs at sign-in: the program, quoted because a
/// path with spaces would otherwise run its first word, then the apps to
/// watch. The command line refuses quotes in an app's name.
fn autostart_command(executable: &Path, watched: &[String]) -> OsString {
    let mut command = OsString::from("\"");
    command.push(executable);
    command.push("\"");
    for app in watched {
        command.push(format!(" --watch \"{app}\""));
    }
    command
}

/// The log line for an autostart change.
fn autostart_line(setting: &Autostart, result: &io::Result<()>) -> String {
    let change = match setting {
        Autostart::On(_) => "autostart on",
        Autostart::Off => "autostart off",
    };
    match result {
        Ok(()) => change.to_owned(),
        Err(error) => format!("{change} failed: {error}"),
    }
}

/// Runs in the background unless another copy already does, in which case
/// it notes that and exits at once.
fn run(watchlist: Watchlist, log: &mut Log) -> io::Result<()> {
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
    log.rotate_at(log::LIMIT);
    note(log, "runtime started");
    let result = watch(watchlist, log);
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

/// What reaches the owner thread.
#[derive(Debug)]
enum Event {
    /// Detection found a change.
    Detected(Change),
    /// The tray reported a pick, a request to close, or its own end.
    Tray(TrayEvent),
    /// Detection stopped because the window events ended.
    DetectionEnded,
}

impl From<Change> for Event {
    fn from(change: Change) -> Self {
        Self::Detected(change)
    }
}

impl From<TrayEvent> for Event {
    fn from(event: TrayEvent) -> Self {
        Self::Tray(event)
    }
}

/// The owner loop: detection and the tray run on their own threads and send
/// events here, where the root model applies them. It ends on Quit from the
/// tray, when Windows asks it to close, or when detection ends.
fn watch(watchlist: Watchlist, log: &mut Log) -> io::Result<()> {
    // The host system sets the process's DPI awareness, which Windows
    // requires before any window exists, so it comes before the tray window.
    let system = HostSystem::default();
    let (window_sender, window_events) = mpsc::channel();
    let windows = WindowWatch::start(window_sender)?;
    let (sender, events) = mpsc::channel();
    let mut model = Model::default();
    // Without a desktop the runtime still works; it only has no icon.
    let tray = match Tray::start(sender.clone(), tray::menu(&model)) {
        Ok(tray) => Some(tray),
        Err(error) => {
            note(log, &format!("no tray icon: {error}"));
            None
        },
    };
    let lifecycle = Lifecycle::new(watchlist.clone(), Instant::now());
    let spawned = thread::Builder::new()
        .name("lifecycle".to_owned())
        .spawn(move || {
            lifecycle::run(lifecycle, &window_events, &sender, || system.processes());
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a failed send means the owner already stopped, which is what this tells it"
            )]
            let _ = sender.send(Event::DetectionEnded);
        });
    let detector = match spawned {
        Ok(detector) => detector,
        Err(error) => {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the spawn error is the one to report; the threads only need to end"
            )]
            let _ = (tray.map(Tray::stop), windows.stop());
            return Err(error);
        },
    };
    for event in &events {
        if handle(&mut model, &watchlist, log, event).is_break() {
            break;
        }
        if let Some(tray) = &tray {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a tray that ended reports why when it is stopped"
            )]
            let _ = tray.show(tray::menu(&model));
        }
    }
    // The tray stops first, so its icon goes before anything else. Stopping
    // the window watch ends detection, so the join below returns.
    let tray_stopped = tray.map_or(Ok(()), Tray::stop);
    let windows_stopped = windows.stop();
    let detected = detector.join().map_err(|payload| {
        io::Error::other(format!(
            "the lifecycle thread panicked: {}",
            appytappy_platform::panic_reason(payload.as_ref())
        ))
    });
    detected.and(tray_stopped).and(windows_stopped)
}

/// Applies one event to the model and logs it. Breaks when the runtime
/// should stop.
fn handle(
    model: &mut Model,
    watchlist: &Watchlist,
    log: &mut Log,
    event: Event,
) -> ControlFlow<()> {
    match event {
        Event::Detected(change) => {
            note(log, &describe(watchlist, &change));
            if let Some(state) = model.apply(change) {
                note(log, &format!("state: {state}"));
            }
        },
        Event::Tray(TrayEvent::CloseRequested) => {
            note(log, "Windows asked the runtime to close");
            return ControlFlow::Break(());
        },
        Event::Tray(TrayEvent::Ended(reason)) => {
            note(log, &format!("the tray icon stopped: {reason}"));
        },
        Event::Tray(TrayEvent::Picked(id)) => match tray::command(id) {
            Some(tray::Command::SetEnabled(enabled)) => {
                model.set_enabled(enabled);
                note(
                    log,
                    if enabled {
                        "automations enabled"
                    } else {
                        "automations disabled"
                    },
                );
            },
            Some(tray::Command::Quit) => {
                note(log, "quit from the tray");
                return ControlFlow::Break(());
            },
            None => {},
        },
        Event::DetectionEnded => return ControlFlow::Break(()),
    }
    ControlFlow::Continue(())
}

fn describe(watchlist: &Watchlist, change: &Change) -> String {
    match change {
        Change::Started { app, process } => {
            format!(
                "{} started (process {process}): attached",
                watchlist.name(*app)
            )
        },
        Change::Exited { app, process } => {
            format!(
                "{} exited (process {process}): detached",
                watchlist.name(*app)
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

    use std::num::NonZeroU16;

    use appytappy_config::anti_cheat::ProcessId;

    use super::*;
    use crate::lifecycle::AppIndex;

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
        let mut log = Log::open(&folder, log::LIMIT).expect("open");
        let ran = run(Watchlist::new(Vec::new()), &mut log);
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
        let on = Autostart::On(vec!["Example.exe".to_owned()]);
        for setting in [on, Autostart::Off] {
            assert_eq!(
                autostart(&setting).expect_err("unsupported").kind(),
                io::ErrorKind::Unsupported
            );
        }
    }

    #[test]
    fn autostart_runs_the_quoted_program_with_its_apps() {
        let command = autostart_command(
            Path::new("C:/Program Files/AppyTappy/appytappy-runtime.exe"),
            &["Example.exe".to_owned(), "Other.exe".to_owned()],
        );
        assert_eq!(
            command,
            OsString::from(
                r#""C:/Program Files/AppyTappy/appytappy-runtime.exe" --watch "Example.exe" --watch "Other.exe""#
            )
        );
    }

    #[test]
    fn autostart_changes_are_logged_with_their_result() {
        let on = Autostart::On(Vec::new());
        let failed = Err(io::Error::other("denied"));
        assert_eq!(autostart_line(&on, &Ok(())), "autostart on");
        assert_eq!(autostart_line(&Autostart::Off, &Ok(())), "autostart off");
        assert_eq!(
            autostart_line(&Autostart::Off, &failed),
            "autostart off failed: denied"
        );
    }

    #[test]
    fn events_update_the_model_and_the_log_until_quit() {
        let folder = env::temp_dir().join(format!("appytappy-handle-{}", process::id()));
        let mut log = Log::open(&folder, log::LIMIT).expect("open");
        let watchlist = Watchlist::new(vec![vec!["Example.exe".to_owned()]]);
        let mut model = Model::default();
        let started = Event::from(Change::Started {
            app: AppIndex(0),
            process: ProcessId::new(7),
        });
        let disable = Event::from(TrayEvent::Picked(tray::DISABLE));
        let unknown = Event::from(TrayEvent::Picked(NonZeroU16::MAX));
        let ended = Event::from(TrayEvent::Ended("its window was destroyed".to_owned()));
        let mut flows = Vec::new();
        for event in [started, disable, unknown, ended] {
            flows.push(handle(&mut model, &watchlist, &mut log, event));
        }
        let disabled = model.enabled();
        let ends = [
            Event::from(TrayEvent::Picked(tray::ENABLE)),
            Event::from(TrayEvent::Picked(tray::QUIT)),
            Event::from(TrayEvent::CloseRequested),
            Event::DetectionEnded,
        ];
        for event in ends {
            flows.push(handle(&mut model, &watchlist, &mut log, event));
        }
        let written = fs::read_to_string(folder.join("runtime.log")).expect("read");
        fs::remove_dir_all(&folder).expect("clean up");
        assert_eq!(
            flows,
            [
                ControlFlow::Continue(()),
                ControlFlow::Continue(()),
                ControlFlow::Continue(()),
                ControlFlow::Continue(()),
                ControlFlow::Continue(()),
                ControlFlow::Break(()),
                ControlFlow::Break(()),
                ControlFlow::Break(()),
            ]
        );
        assert!(!disabled, "Disable disables");
        assert!(model.enabled(), "Enable enables again");
        assert_eq!(model.state(), model::State::Attached);
        assert_eq!(written.lines().count(), 7, "{written:?}");
        assert!(
            written.contains("Example.exe started (process 7): attached"),
            "{written:?}"
        );
        assert!(written.contains("state: attached"), "{written:?}");
        assert!(written.contains("automations disabled"), "{written:?}");
        assert!(written.contains("automations enabled"), "{written:?}");
        assert!(written.contains("quit from the tray"), "{written:?}");
        assert!(
            written.contains("the tray icon stopped: its window was destroyed"),
            "{written:?}"
        );
        assert!(
            written.contains("Windows asked the runtime to close"),
            "{written:?}"
        );
    }

    #[test]
    fn log_lines_name_the_app_and_process() {
        let watchlist = Watchlist::new(vec![vec!["Example.exe".to_owned()]]);
        let app = AppIndex(0);
        let process = ProcessId::new(42);
        let lines = [
            Change::Started { app, process },
            Change::Exited { app, process },
            Change::ScanFailed("denied".to_owned()),
            Change::ScanRestored,
        ]
        .map(|change| describe(&watchlist, &change));
        assert_eq!(
            lines,
            [
                "Example.exe started (process 42): attached",
                "Example.exe exited (process 42): detached",
                "process snapshots fail: denied",
                "process snapshots work again",
            ]
        );
    }
}
