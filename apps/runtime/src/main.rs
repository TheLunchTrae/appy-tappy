//! The always-on AppyTappy background program.
//!
//! It runs with no console window and writes only to its log in
//! `%LOCALAPPDATA%\AppyTappy\logs\`. See `docs/architecture.md` for the
//! runtime's threads and events and its lifecycle states.

#![forbid(unsafe_code)]
#![windows_subsystem = "windows"]

mod act;
mod cli;
mod lifecycle;
mod log;
mod model;
mod plan;
mod reader;
mod tray;

use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::io;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use appytappy_config::paths::Locations;
use appytappy_config::store::{AppStore, AutomationStore};
use appytappy_platform::{
    AUTOSTART_VALUE, InstanceGuard, LocalTime, RELOAD, ReloadRequest, ReloadWatch, Tray, TrayEvent,
    WindowWatch,
};
use appytappy_sources::HostSystem;
use appytappy_sources::system::System;

use crate::act::ActionFailed;
use crate::cli::Command;
use crate::lifecycle::{Change, Input, Lifecycle};
use crate::log::Log;
use crate::model::{Model, Order};
use crate::reader::{Reader, Reading, SessionId};

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

/// Turns autostart at sign-in on or off.
fn autostart(on: bool) -> io::Result<()> {
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
fn autostart_line(on: bool, result: &io::Result<()>) -> String {
    let change = if on { "autostart on" } else { "autostart off" };
    match result {
        Ok(()) => change.to_owned(),
        Err(error) => format!("{change} failed: {error}"),
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
    log.rotate_at(log::LIMIT);
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

/// What reaches the owner thread.
#[derive(Debug)]
enum Event {
    /// Detection found a change.
    Detected(Change),
    /// The tray reported a pick, a request to close, or its own end.
    Tray(TrayEvent),
    /// Detection stopped because its input ended.
    DetectionEnded,
    /// The setup app saved, so the automations are read again.
    Reload,
    /// A reader read a value or met a problem.
    Reading(Reading),
    /// An action could not run.
    ActionFailed(ActionFailed),
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

impl From<ReloadRequest> for Event {
    fn from(_request: ReloadRequest) -> Self {
        Self::Reload
    }
}

impl From<Reading> for Event {
    fn from(reading: Reading) -> Self {
        Self::Reading(reading)
    }
}

impl From<ActionFailed> for Event {
    fn from(failed: ActionFailed) -> Self {
        Self::ActionFailed(failed)
    }
}

/// What the owner does after an event.
#[derive(Debug, PartialEq)]
enum Step {
    /// Carry out these orders and wait for the next event.
    Carry(Vec<Order>),
    /// Read the saved automations again.
    Reload,
    /// Stop the runtime.
    Stop,
}

/// Where the saved applications and automations are.
struct Stores {
    apps: AppStore,
    automations: AutomationStore,
}

/// The threads and handles the owner carries orders out with.
struct Crew {
    system: Arc<HostSystem>,
    events: Sender<Event>,
    jobs: Sender<act::Job>,
    readers: BTreeMap<SessionId, Reader>,
}

/// The owner loop: detection, the tray, the reload watch, each reader, and
/// the actions run on their own threads and send events here, where the
/// root model applies them. It ends on Quit from the tray, when Windows asks
/// it to close, or when detection ends.
#[mutants::skip]
fn watch(locations: &Locations, log: &mut Log) -> io::Result<()> {
    let stores = Stores {
        apps: AppStore::new(locations.apps()),
        automations: AutomationStore::new(locations.automations()),
    };
    // The host system sets the process's DPI awareness, which Windows
    // requires before any window exists, so it comes before the tray window.
    let system = Arc::new(HostSystem::default());
    let (inputs, input_events) = mpsc::channel();
    let windows = WindowWatch::start(inputs.clone())?;
    let (sender, events) = mpsc::channel();
    let mut model = Model::default();
    // Without a desktop the runtime still works; it only has no icon.
    let tray = Tray::start(sender.clone(), tray::menu(&model))
        .inspect_err(|error| note(log, &format!("no tray icon: {error}")))
        .ok();
    // Without the reload event the runtime still runs what it read at start.
    let reload = ReloadWatch::start(RELOAD, sender.clone())
        .inspect_err(|error| {
            note(
                log,
                &format!("saving in the setup app cannot reach the runtime: {error}"),
            );
        })
        .ok();
    let first = load(&stores, log);
    let lifecycle = Lifecycle::new(first.watchlist(), Instant::now());
    let detect = Arc::clone(&system);
    let changes = sender.clone();
    let detector = spawn("lifecycle", move || {
        lifecycle::run(lifecycle, &input_events, &changes, || detect.processes());
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a failed send means the owner already stopped, which is what this tells it"
        )]
        let _ = changes.send(Event::DetectionEnded);
    });
    let (jobs, job_events) = mpsc::channel();
    let failures = sender.clone();
    let actor = spawn("actions", move || {
        act::run(controls, &job_events, &failures);
    });
    let (detector, actor) = match (detector, actor) {
        (Ok(detector), Ok(actor)) => (detector, actor),
        (detector, actor) => {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the spawn error is the one to report; the threads only need to end"
            )]
            let _ = (
                tray.map(Tray::stop),
                reload.map(ReloadWatch::stop),
                windows.stop(),
            );
            return Err(detector
                .and(actor)
                .err()
                .unwrap_or_else(|| io::Error::other("no thread")));
        },
    };
    let mut crew = Crew {
        system,
        events: sender,
        jobs,
        readers: BTreeMap::new(),
    };
    let orders = model.replan(first.apps);
    carry(orders, &mut crew, &model, log);
    for event in &events {
        match handle(&mut model, log, event) {
            Step::Carry(orders) => carry(orders, &mut crew, &model, log),
            Step::Reload => {
                let plan = load(&stores, log);
                if inputs.send(Input::Watch(plan.watchlist())).is_err() {
                    break;
                }
                let orders = model.replan(plan.apps);
                carry(orders, &mut crew, &model, log);
            },
            Step::Stop => break,
        }
        if let Some(tray) = &tray {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a tray that ended reports why when it is stopped"
            )]
            let _ = tray.show(tray::menu(&model));
        }
    }
    // Undo what the running automations changed before anything stops.
    let orders = model.set_enabled(false);
    carry(orders, &mut crew, &model, log);
    // The tray stops first, so its icon goes before anything else. Dropping
    // the inputs and the jobs ends detection and the actions, so the joins
    // below return; the actions undo what is queued first.
    let tray_stopped = tray.map_or(Ok(()), Tray::stop);
    let reload_stopped = reload.map_or(Ok(()), ReloadWatch::stop);
    let windows_stopped = windows.stop();
    drop(inputs);
    let Crew { jobs, readers, .. } = crew;
    drop(jobs);
    let read = readers.into_values().try_for_each(Reader::stop);
    let detected = join(detector);
    let acted = join(actor);
    detected
        .and(acted)
        .and(read)
        .and(tray_stopped)
        .and(reload_stopped)
        .and(windows_stopped)
}

/// The operating system's media and audio controls, made on the action
/// thread.
#[cfg(windows)]
fn controls() -> io::Result<appytappy_actions::win::WindowsControls> {
    appytappy_actions::win::WindowsControls::new()
}

/// Off Windows there are no controls, so no action can run.
#[cfg(not(windows))]
fn controls() -> io::Result<NoControls> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "actions run only on Windows",
    ))
}

/// Controls that cannot exist, for systems other than Windows.
#[cfg(not(windows))]
#[derive(Debug)]
struct NoControls(std::convert::Infallible);

#[cfg(not(windows))]
impl appytappy_actions::Controls for NoControls {
    type Media = ();
    type Volume = ();

    #[mutants::skip]
    fn current_media(&mut self) -> io::Result<Option<appytappy_actions::MediaSession<()>>> {
        match self.0 {}
    }

    #[mutants::skip]
    fn media_sessions(&mut self) -> io::Result<Vec<appytappy_actions::MediaSession<()>>> {
        match self.0 {}
    }

    #[mutants::skip]
    fn media_command(
        &mut self,
        (): &(),
        _command: appytappy_config::action::MediaCommand,
    ) -> io::Result<bool> {
        match self.0 {}
    }

    #[mutants::skip]
    fn system_volume(&mut self) -> io::Result<Option<appytappy_actions::Volume<()>>> {
        match self.0 {}
    }

    #[mutants::skip]
    fn app_volumes(&mut self) -> io::Result<Vec<(String, appytappy_actions::Volume<()>)>> {
        match self.0 {}
    }

    #[mutants::skip]
    fn set_volume(
        &mut self,
        (): &(),
        _level: appytappy_config::action::VolumeLevel,
    ) -> io::Result<()> {
        match self.0 {}
    }
}

/// Starts a named thread.
fn spawn(name: &str, work: impl FnOnce() + Send + 'static) -> io::Result<JoinHandle<()>> {
    thread::Builder::new().name(name.to_owned()).spawn(work)
}

/// Waits for a thread and turns its panic into an error.
fn join(thread: JoinHandle<()>) -> io::Result<()> {
    let name = thread.thread().name().unwrap_or("a").to_owned();
    thread.join().map_err(|payload| {
        io::Error::other(format!(
            "the {name} thread panicked: {}",
            appytappy_platform::panic_reason(payload.as_ref())
        ))
    })
}

/// Reads the saved automations and logs what cannot run.
fn load(stores: &Stores, log: &mut Log) -> plan::Plan {
    let plan = plan::load(&stores.apps, &stores.automations);
    for problem in &plan.problems {
        note(log, problem);
    }
    note(log, &loaded_line(&plan));
    plan
}

/// The log line that says what the runtime runs.
fn loaded_line(plan: &plan::Plan) -> String {
    let automations: usize = plan.apps.values().map(|app| app.automations.len()).sum();
    let names: Vec<&str> = plan.apps.values().map(|app| app.name.as_str()).collect();
    match automations {
        0 => "no saved automation can run".to_owned(),
        1 => format!("1 automation runs for {}", names.join(", ")),
        _ => format!("{automations} automations run for {}", names.join(", ")),
    }
}

/// Carries out `orders`: starts and stops readers and hands actions to the
/// action thread.
#[mutants::skip]
fn carry(orders: Vec<Order>, crew: &mut Crew, model: &Model, log: &mut Log) {
    for order in orders {
        if let Some(line) = order_line(&order) {
            note(log, &line);
        }
        match order {
            Order::Read(job) => {
                let id = job.session;
                let name = job.plan.name.clone();
                match Reader::start(Arc::clone(&crew.system), job, crew.events.clone()) {
                    Ok(reader) => {
                        crew.readers.insert(id, reader);
                    },
                    Err(error) => note(log, &format!("{name} cannot be read: {error}")),
                }
            },
            Order::StopReading(id) => {
                if let Some(Err(error)) = crew.readers.remove(&id).map(Reader::stop) {
                    let name = model.app_name(id).unwrap_or("an application");
                    note(log, &format!("{name}'s reader stopped badly: {error}"));
                }
            },
            Order::Act(job) => {
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the action thread ends only after the owner drops the jobs, so a send cannot fail while it runs"
                )]
                let _ = crew.jobs.send(job);
            },
        }
    }
}

/// The log line for an order, if it gets one.
fn order_line(order: &Order) -> Option<String> {
    match order {
        Order::Read(job) => Some(format!("{}: reading its values", job.plan.name)),
        Order::StopReading(_) => None,
        Order::Act(act::Job::Run { automation, .. }) => Some(format!("{automation}: runs")),
        Order::Act(act::Job::Undo { automation, .. }) => Some(format!("{automation}: undone")),
    }
}

/// Applies one event to the model and logs it.
fn handle(model: &mut Model, log: &mut Log, event: Event) -> Step {
    match event {
        Event::Detected(change) => {
            note(log, &describe(&change));
            let before = model.state();
            let orders = model.apply(change, Instant::now());
            let after = model.state();
            if after != before {
                note(log, &format!("state: {after}"));
            }
            Step::Carry(orders)
        },
        Event::Tray(TrayEvent::CloseRequested) => {
            note(log, "Windows asked the runtime to close");
            Step::Stop
        },
        Event::Tray(TrayEvent::Ended(reason)) => {
            note(log, &format!("the tray icon stopped: {reason}"));
            Step::Carry(Vec::new())
        },
        Event::Tray(TrayEvent::Picked(id)) => match tray::command(id) {
            Some(tray::Command::SetEnabled(enabled)) => {
                note(
                    log,
                    if enabled {
                        "automations enabled"
                    } else {
                        "automations disabled"
                    },
                );
                Step::Carry(model.set_enabled(enabled))
            },
            Some(tray::Command::Quit) => {
                note(log, "quit from the tray");
                Step::Stop
            },
            None => Step::Carry(Vec::new()),
        },
        Event::DetectionEnded => Step::Stop,
        Event::Reload => {
            note(
                log,
                "the setup app saved, so the automations are read again",
            );
            Step::Reload
        },
        Event::Reading(Reading::Sample {
            session,
            signal,
            value,
            at,
        }) => Step::Carry(model.sample(session, signal, value, at)),
        Event::Reading(Reading::Problem { session, problem }) => {
            if let Some(name) = model.app_name(session) {
                note(log, &format!("{name}: {problem}"));
            }
            Step::Carry(Vec::new())
        },
        Event::ActionFailed(ActionFailed(message)) => {
            note(log, &message);
            Step::Carry(Vec::new())
        },
    }
}

fn describe(change: &Change) -> String {
    match change {
        Change::Started {
            app,
            process,
            executable,
        } => format!("{app} started as {executable} (process {process}): attached"),
        Change::Exited { app, process } => format!("{app} exited (process {process}): detached"),
        Change::ScanFailed(problem) => format!("process snapshots fail: {problem}"),
        Change::ScanRestored => "process snapshots work again".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::num::NonZeroU16;
    use std::process;

    use appytappy_config::anti_cheat::ProcessId;

    use super::*;

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
        let folder = env::temp_dir().join(format!("appytappy-run-{}", process::id()));
        let mut log = Log::open(&folder, log::LIMIT).expect("open");
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
        assert_eq!(
            controls().expect_err("unsupported").kind(),
            io::ErrorKind::Unsupported
        );
    }

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

    #[test]
    fn events_update_the_model_and_the_log_until_quit() {
        let folder = env::temp_dir().join(format!("appytappy-handle-{}", process::id()));
        let mut log = Log::open(&folder, log::LIMIT).expect("open");
        let mut model = Model::default();
        let started = Event::from(Change::Started {
            app: "example".to_owned(),
            process: ProcessId::new(7),
            executable: "Example.exe".to_owned(),
        });
        let problem = Event::from(Reading::Problem {
            session: SessionId(3),
            problem: "dropped, because no session 3 runs".to_owned(),
        });
        let failed = Event::from(ActionFailed("Pause: action 1 failed: no player".to_owned()));
        let disable = Event::from(TrayEvent::Picked(tray::DISABLE));
        let unknown = Event::from(TrayEvent::Picked(NonZeroU16::MAX));
        let ended = Event::from(TrayEvent::Ended("its window was destroyed".to_owned()));
        let mut steps = Vec::new();
        for event in [started, problem, failed, disable, unknown, ended] {
            steps.push(handle(&mut model, &mut log, event));
        }
        let disabled = model.enabled();
        let ends = [
            Event::from(TrayEvent::Picked(tray::ENABLE)),
            Event::from(ReloadRequest),
            Event::from(TrayEvent::Picked(tray::QUIT)),
            Event::from(TrayEvent::CloseRequested),
            Event::DetectionEnded,
        ];
        for event in ends {
            steps.push(handle(&mut model, &mut log, event));
        }
        let written = fs::read_to_string(folder.join("runtime.log")).expect("read");
        fs::remove_dir_all(&folder).expect("clean up");
        let carry = || Step::Carry(Vec::new());
        assert_eq!(
            steps,
            [
                carry(),
                carry(),
                carry(),
                carry(),
                carry(),
                carry(),
                carry(),
                Step::Reload,
                Step::Stop,
                Step::Stop,
                Step::Stop,
            ]
        );
        assert!(!disabled, "Disable disables");
        assert!(model.enabled(), "Enable enables again");
        assert_eq!(model.state(), model::State::Attached);
        // On Windows each line starts with the local time.
        let lines: Vec<&str> = written.lines().collect();
        let expected = [
            "example started as Example.exe (process 7): attached",
            "state: attached",
            "Pause: action 1 failed: no player",
            "automations disabled",
            "the tray icon stopped: its window was destroyed",
            "automations enabled",
            "the setup app saved, so the automations are read again",
            "quit from the tray",
            "Windows asked the runtime to close",
        ];
        assert_eq!(lines.len(), expected.len(), "{written:?}");
        for (line, message) in lines.iter().zip(expected) {
            assert!(
                line.ends_with(message),
                "{line:?} should end with {message:?}"
            );
        }
    }

    #[test]
    fn log_lines_name_the_app_and_process() {
        let process = ProcessId::new(42);
        let lines = [
            Change::Started {
                app: "example".to_owned(),
                process,
                executable: "Example.exe".to_owned(),
            },
            Change::Exited {
                app: "example".to_owned(),
                process,
            },
            Change::ScanFailed("denied".to_owned()),
            Change::ScanRestored,
        ]
        .map(|change| describe(&change));
        assert_eq!(
            lines,
            [
                "example started as Example.exe (process 42): attached",
                "example exited (process 42): detached",
                "process snapshots fail: denied",
                "process snapshots work again",
            ]
        );
    }

    #[test]
    fn the_log_says_how_many_automations_run() {
        let empty = plan::Plan::default();
        assert_eq!(loaded_line(&empty), "no saved automation can run");
    }
}
