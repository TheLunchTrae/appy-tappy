//! The owner thread: starts the other threads, then turns each event they send
//! into changes to the root model and orders for the workers, until the runtime
//! quits.

use std::collections::BTreeMap;
use std::io;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use appytappy_config::paths::Locations;
use appytappy_config::store::{AppStore, AutomationStore};
use appytappy_platform::{RELOAD, ReloadRequest, ReloadWatch, Tray, TrayEvent, WindowWatch};
use appytappy_sources::HostSystem;
use appytappy_sources::system::System;

use crate::lifecycle::{Change, Input, Lifecycle, Watchlist};
use crate::log::Log;
use crate::model::{Model, Order};
use crate::workers::act::ActionFailed;
use crate::workers::reader::{Reader, Reading, SessionId};
use crate::workers::{act, join, spawn};
use crate::{lifecycle, note, plan, tray};

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
    /// An automation that its cooldown held back is due, and no event came
    /// before.
    Due,
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
pub(crate) fn watch(locations: &Locations, log: &mut Log) -> io::Result<()> {
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
    let mut shown = tray::menu(&model);
    let tray = Tray::start(sender.clone(), shown.clone())
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
    let mut retry = None;
    let mut had = Had::Plan;
    let first = load(&stores, log).unwrap_or_else(|error| {
        had = Had::Nothing;
        retry = failed(log, &error, None, Instant::now(), had);
        plan::Plan::default()
    });
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
    carry(orders, &mut crew, log);
    loop {
        let due = wake(model.next_due(), retry);
        let Some(event) = next(&events, due) else {
            break;
        };
        let read_again = match handle(&mut model, log, event) {
            Step::Carry(orders) => {
                carry(orders, &mut crew, log);
                retry.is_some_and(|retry| retry.due(Instant::now()))
            },
            Step::Reload => {
                // A save starts the tries over.
                retry = None;
                true
            },
            Step::Stop => break,
        };
        if read_again
            && let Some((watchlist, orders)) = reread(
                &stores,
                log,
                &mut model,
                (&mut retry, &mut had),
                Instant::now(),
            )
        {
            if inputs.send(Input::Watch(watchlist)).is_err() {
                break;
            }
            carry(orders, &mut crew, log);
        }
        // Most events, such as each value read, leave the menu as it was, and
        // the icon is redrawn only when it changes.
        let menu = tray::menu(&model);
        if let Some(tray) = tray.as_ref().filter(|_| menu != shown) {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a tray that ended reports why when it is stopped"
            )]
            let _ = tray.show(menu.clone());
        }
        shown = menu;
    }
    // Undo what the running automations changed before anything stops.
    let orders = model.set_enabled(false);
    carry(orders, &mut crew, log);
    // Dropping the jobs ends the action thread once it has undone what is
    // queued. Only then does the tray stop: at the end of the session the
    // tray holds it until then, and Windows may end the process after. This
    // order (undo, drop the jobs, join the action thread, then Tray::stop)
    // is also what keeps the two from waiting on each other: the tray's wait
    // ends at Tray::stop, which nothing here waits behind.
    let Crew { jobs, readers, .. } = crew;
    drop(jobs);
    let acted = join(actor);
    let tray_stopped = tray.map_or(Ok(()), Tray::stop);
    // Dropping the inputs ends detection, so its join below returns.
    let reload_stopped = reload.map_or(Ok(()), ReloadWatch::stop);
    let windows_stopped = windows.stop();
    drop(inputs);
    // Every reader is stopped, and the first that stopped badly says how.
    let read = readers
        .into_values()
        .map(Reader::stop)
        .fold(Ok(()), Result::and);
    let detected = join(detector);
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
#[mutants::skip] // Makes the real controls, checked by hand on Windows.
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
    ) -> io::Result<bool> {
        match self.0 {}
    }

    #[mutants::skip]
    fn focused_process(&mut self) -> io::Result<Option<appytappy_config::anti_cheat::ProcessId>> {
        match self.0 {}
    }

    #[mutants::skip]
    fn held_modifiers(&mut self) -> io::Result<Vec<appytappy_config::action::Modifier>> {
        match self.0 {}
    }

    #[mutants::skip]
    fn send_strokes(&mut self, _strokes: &[appytappy_actions::Stroke]) -> io::Result<usize> {
        match self.0 {}
    }

    #[mutants::skip]
    fn hold(&mut self, _time: std::time::Duration) {
        match self.0 {}
    }
}

/// Waits for the next event, or, when an automation is `due`, until then at
/// most. Returns `None` once nobody can send another event.
fn next(events: &Receiver<Event>, due: Option<Instant>) -> Option<Event> {
    let Some(due) = due else {
        return events.recv().ok();
    };
    // A due time that passed goes first, so a busy channel cannot hold back
    // an automation that is due.
    let now = Instant::now();
    if due <= now {
        return Some(Event::Due);
    }
    match events.recv_timeout(due.saturating_duration_since(now)) {
        Ok(event) => Some(event),
        Err(RecvTimeoutError::Timeout) => Some(Event::Due),
        Err(RecvTimeoutError::Disconnected) => None,
    }
}

/// How long the owner waits before it reads the saved files again after
/// they could not be read, such as while another program held one.
const RETRY_WAIT: Duration = Duration::from_secs(2);

/// How many times in a row the owner reads the saved files again before it
/// waits for the next save.
const RETRIES: u32 = 5;

/// When the owner reads the saved files again after a read failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Retry {
    /// When.
    at: Instant,
    /// How many more tries follow this one, should it fail too.
    left: u32,
}

impl Retry {
    /// Whether the files are to be read again at `now`.
    fn due(self, now: Instant) -> bool {
        self.at <= now
    }
}

/// When the owner wakes with no event: the first of `next_due`, when a held
/// rule is due, and `retry`, when it reads the saved files again.
fn wake(next_due: Option<Instant>, retry: Option<Retry>) -> Option<Instant> {
    [next_due, retry.map(|retry| retry.at)]
        .into_iter()
        .flatten()
        .min()
}

/// What the runtime runs while the saved files cannot be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Had {
    /// Nothing, since no read has worked since it started.
    Nothing,
    /// The plan from the last read that worked.
    Plan,
}

/// Reads the saved automations and logs what cannot run.
///
/// # Errors
///
/// Returns why the saved files cannot be read as a whole.
fn load(stores: &Stores, log: &mut Log) -> Result<plan::Plan, String> {
    let plan = plan::load(&stores.apps, &stores.automations)?;
    for problem in &plan.problems {
        note(log, problem);
    }
    note(log, &loaded_line(&plan));
    Ok(plan)
}

/// Reads the saved files again at `now`, after a save or for `retry`. When
/// they read, returns the watchlist and the orders that switch `model` to
/// their plan. When they do not, `model` keeps running its plan, and
/// `retry` says when to read them again, if at all. `had` says whether a
/// read has worked yet.
fn reread(
    stores: &Stores,
    log: &mut Log,
    model: &mut Model,
    (retry, had): (&mut Option<Retry>, &mut Had),
    now: Instant,
) -> Option<(Watchlist, Vec<Order>)> {
    match load(stores, log) {
        Ok(plan) => {
            *retry = None;
            *had = Had::Plan;
            Some((plan.watchlist(), model.replan(plan.apps)))
        },
        Err(error) => {
            *retry = failed(log, &error, *retry, now, *had);
            None
        },
    }
}

/// Logs that the saved files cannot be read for `error`, and what the
/// runtime runs meanwhile, as `had` says, and returns when to read them
/// again: soon, unless `previous`, the try that just failed, was the last.
fn failed(
    log: &mut Log,
    error: &str,
    previous: Option<Retry>,
    now: Instant,
    had: Had,
) -> Option<Retry> {
    let left = previous
        .map_or(RETRIES, |previous| previous.left)
        .checked_sub(1);
    let retry = left.and_then(|left| {
        Some(Retry {
            at: now.checked_add(RETRY_WAIT)?,
            left,
        })
    });
    let then = if retry.is_some() {
        format!("reads them again in {} seconds", RETRY_WAIT.as_secs())
    } else {
        "waits for the next save".to_owned()
    };
    let keeps = match had {
        Had::Nothing => "runs no automations",
        Had::Plan => "keeps running what it ran",
    };
    note(log, &format!("{error}, so the runtime {keeps} and {then}"));
    retry
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
fn carry(orders: Vec<Order>, crew: &mut Crew, log: &mut Log) {
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
            Order::StopReading(id, name) => {
                if let Some(Err(error)) = crew.readers.remove(&id).map(Reader::stop) {
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
        Order::StopReading(..) => None,
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
        Event::Reading(Reading::Unseen { session }) => {
            if let Some(name) = model.app_name(session) {
                note(
                    log,
                    &format!(
                        "{name}: its window closed or changed size, so its values are read \
                         again before any automation runs or is undone"
                    ),
                );
            }
            model.forget(session);
            Step::Carry(Vec::new())
        },
        Event::ActionFailed(ActionFailed(message)) => {
            note(log, &message);
            Step::Carry(Vec::new())
        },
        Event::Due => Step::Carry(model.recheck(Instant::now())),
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
    use std::env;
    use std::fs;
    use std::num::NonZeroU16;
    use std::process;
    use std::time::Duration;

    use appytappy_config::anti_cheat::ProcessId;
    use appytappy_config::window_match::{MustMatch, Pattern, WindowMatch};

    use super::*;
    use crate::workers::reader;
    use crate::{log, model};

    #[cfg(not(windows))]
    #[test]
    fn off_windows_actions_cannot_run() {
        assert_eq!(
            controls().expect_err("unsupported").kind(),
            io::ErrorKind::Unsupported
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
    fn the_owner_wakes_when_an_automation_is_due() {
        let (sender, events) = mpsc::channel();
        let passed = Instant::now();
        let later = passed
            .checked_add(Duration::from_secs(60))
            .expect("a test time fits in an Instant");
        assert!(matches!(
            next(&events, passed.checked_add(Duration::from_millis(10))),
            Some(Event::Due)
        ));
        sender.send(Event::Reload).expect("send");
        assert!(
            matches!(next(&events, Some(passed)), Some(Event::Due)),
            "a due time that passed goes before a waiting event"
        );
        assert!(
            matches!(next(&events, Some(later)), Some(Event::Reload)),
            "an event goes before a due time still to come"
        );
        sender.send(Event::DetectionEnded).expect("send");
        assert!(matches!(next(&events, None), Some(Event::DetectionEnded)));
        drop(sender);
        assert!(next(&events, Some(later)).is_none());
        assert!(next(&events, None).is_none());
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

    /// An app plan named `name` whose automations are only names.
    fn app_plan(name: &str, automations: &[&str]) -> Arc<plan::AppPlan> {
        Arc::new(plan::AppPlan {
            name: name.to_owned(),
            window: WindowMatch {
                title: Pattern::new("*"),
                class: Pattern::new("*"),
                executable: Pattern::new("Example.exe"),
                must_match: MustMatch {
                    title: true,
                    class: true,
                },
                fallback: false,
            },
            values: Vec::new(),
            rules: Arc::<[appytappy_config::rule::Rule]>::from(Vec::new()),
            automations: automations.iter().map(|&name| name.to_owned()).collect(),
        })
    }

    #[test]
    fn the_log_counts_the_automations_and_names_their_apps() {
        let mut plan = plan::Plan::default();
        plan.apps
            .insert("a".to_owned(), app_plan("Example App", &["Mute"]));
        assert_eq!(loaded_line(&plan), "1 automation runs for Example App");
        plan.apps
            .insert("b".to_owned(), app_plan("Other App", &["Pause", "Play"]));
        assert_eq!(
            loaded_line(&plan),
            "3 automations run for Example App, Other App"
        );
    }

    #[test]
    fn a_reload_that_cannot_read_the_files_keeps_the_plan_and_tries_again() {
        let folder = env::temp_dir().join(format!("appytappy-reload-{}", process::id()));
        fs::create_dir_all(&folder).expect("make the folder");
        let stores = Stores {
            apps: AppStore::new(folder.join("apps")),
            automations: AutomationStore::new(folder.join("automations.json")),
        };
        let mut log = Log::open(&folder.join("logs"), log::LIMIT).expect("open");
        let mut model = Model::default();
        model.replan(BTreeMap::from([(
            "a".to_owned(),
            app_plan("Example App", &["Mute"]),
        )]));
        model.apply(
            Change::Started {
                app: "a".to_owned(),
                process: ProcessId::new(7),
                executable: "Example.exe".to_owned(),
            },
            Instant::now(),
        );
        fs::write(folder.join("automations.json"), "not JSON").expect("break the file");
        let now = Instant::now();
        let mut retry = None;
        let mut had = Had::Plan;
        let mut tries = Vec::new();
        for _ in 0..=RETRIES {
            assert_eq!(
                reread(&stores, &mut log, &mut model, (&mut retry, &mut had), now),
                None
            );
            tries.push(retry.map(|retry| retry.left));
        }
        assert_eq!(
            model.active_automations(),
            ["Mute"],
            "the plan keeps running"
        );
        assert_eq!(
            tries,
            [Some(4), Some(3), Some(2), Some(1), Some(0), None],
            "the tries are bounded"
        );
        assert_eq!(
            retry, None,
            "after the last try only a save reads the files again"
        );
        // As at start, when no read has worked yet.
        had = Had::Nothing;
        retry = failed(
            &mut log,
            "the automations cannot be read: locked",
            None,
            now,
            had,
        );
        assert_eq!(
            retry.map(|retry| retry.at),
            now.checked_add(RETRY_WAIT),
            "a failed read tries again soon"
        );
        assert_eq!(
            reread(&stores, &mut log, &mut model, (&mut retry, &mut had), now),
            None
        );
        assert_eq!(retry.map(|retry| retry.left), Some(3));
        fs::write(
            folder.join("automations.json"),
            "{\"format\":1,\"automations\":[]}",
        )
        .expect("mend the file");
        let (_, orders) = reread(&stores, &mut log, &mut model, (&mut retry, &mut had), now)
            .expect("the files read");
        assert_eq!(had, Had::Plan, "a read that works gives a plan");
        let written = fs::read_to_string(folder.join("logs").join("runtime.log")).expect("read");
        fs::remove_dir_all(&folder).expect("clean up");
        assert_eq!(retry, None, "a read that works needs no retry");
        assert_eq!(
            orders,
            [Order::StopReading(SessionId(0), "Example App".to_owned())]
        );
        assert!(
            written.contains("the automations cannot be read: the automations file is not valid: "),
            "{written:?}"
        );
        assert!(
            written.contains(
                ", so the runtime keeps running what it ran and reads them again in 2 seconds\n"
            ),
            "{written:?}"
        );
        assert!(
            written.contains(
                ", so the runtime keeps running what it ran and waits for the next save\n"
            ),
            "{written:?}"
        );
        assert_eq!(
            written
                .matches(", so the runtime runs no automations and reads them again in 2 seconds\n")
                .count(),
            2,
            "the start's failure and the retry after it: {written:?}"
        );
        assert!(
            written.ends_with("no saved automation can run\n"),
            "{written:?}"
        );
    }

    #[test]
    fn a_retry_alone_wakes_the_owner() {
        let now = Instant::now();
        let soon = now.checked_add(Duration::from_secs(1)).expect("soon");
        let later = now.checked_add(Duration::from_secs(3)).expect("later");
        let retry = |at| Retry { at, left: 2 };
        assert_eq!(wake(None, None), None);
        assert_eq!(wake(None, Some(retry(soon))), Some(soon));
        assert_eq!(wake(Some(later), Some(retry(soon))), Some(soon));
        assert_eq!(wake(Some(soon), Some(retry(later))), Some(soon));
        assert_eq!(wake(Some(soon), None), Some(soon));
        assert!(!retry(soon).due(now), "not yet");
        assert!(retry(soon).due(soon), "right on time");
        assert!(retry(now).due(soon), "late");
    }

    #[test]
    fn orders_that_change_something_get_a_log_line() {
        let plan = app_plan("Example App", &[]);
        let owner = (SessionId(1), appytappy_engine::RuleIndex(0));
        let orders = [
            Order::Read(reader::Job {
                session: SessionId(1),
                plan,
                executable: "Example.exe".to_owned(),
            }),
            Order::StopReading(SessionId(1), "Example App".to_owned()),
            Order::Act(act::Job::Run {
                owner,
                automation: "Mute".to_owned(),
                watched: appytappy_actions::Watched {
                    process: appytappy_config::anti_cheat::ProcessId::new(1),
                    executable: "Example.exe".to_owned(),
                },
                actions: Box::new([]),
                restore: appytappy_config::rule::Restore::OnRelease,
            }),
            Order::Act(act::Job::Undo {
                owner,
                automation: "Mute".to_owned(),
            }),
        ];
        assert_eq!(
            orders.each_ref().map(order_line),
            [
                Some("Example App: reading its values".to_owned()),
                None,
                Some("Mute: runs".to_owned()),
                Some("Mute: undone".to_owned()),
            ]
        );
    }
}
