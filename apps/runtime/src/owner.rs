//! The owner thread: starts the other threads, then turns each event they send
//! into changes to the root model and orders for the workers, until the runtime
//! quits.

use std::collections::BTreeMap;
use std::io;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use appytappy_config::paths::Locations;
use appytappy_config::settings_file::{ReadIntervalDef, SettingsFile};
use appytappy_config::store::{AppStore, AutomationStore, SettingsStore};
use appytappy_platform::{RELOAD, ReloadRequest, ReloadWatch, Tray, TrayEvent, WindowWatch};
use appytappy_sources::HostSystem;
use appytappy_sources::schedule::ReadInterval;
use appytappy_sources::system::{System, shows};

use crate::lifecycle::{Change, Input, Lifecycle, Watchlist};
use crate::log::Log;
use crate::model::{Model, Order};
use crate::trace::switch::Switch;
use crate::trace::writer::TraceReport;
use crate::trace::{Detail, Record, Trace, What};
use crate::workers::act::ActionReport;
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
    /// The setup app saved, so the settings and automations are read again.
    Reload,
    /// A reader read a value or met a problem.
    Reading(Reading),
    /// The action thread noted something for the log, or said whether
    /// actions can run.
    Actions(ActionReport),
    /// The trace writer stopped, because it could not write.
    Trace(TraceReport),
    /// An automation that its cooldown held back is due, the trace's time
    /// is up, or the saved files are to be read again, and no event came
    /// before.
    Due,
}

impl Event {
    /// The kind of event, for the trace.
    const fn kind(&self) -> &'static str {
        match self {
            Self::Detected(_) => "detected",
            Self::Tray(_) => "tray",
            Self::DetectionEnded => "detection ended",
            Self::Reload => "reload",
            Self::Reading(Reading::Sample { .. }) => "sample",
            Self::Reading(_) => "reader",
            Self::Actions(_) => "actions",
            Self::Trace(_) => "trace",
            Self::Due => "due",
        }
    }

    /// When the frame arrived whose reading this is.
    const fn cause(&self) -> Option<Instant> {
        match self {
            Self::Reading(Reading::Sample { at, .. }) => Some(*at),
            _ => None,
        }
    }
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

impl From<ActionReport> for Event {
    fn from(report: ActionReport) -> Self {
        Self::Actions(report)
    }
}

impl From<TraceReport> for Event {
    fn from(report: TraceReport) -> Self {
        Self::Trace(report)
    }
}

/// What the owner does after an event.
#[derive(Debug, PartialEq)]
enum Step {
    /// Carry out these orders and wait for the next event.
    Carry(Vec<Order>),
    /// Read the saved automations again.
    Reload,
    /// Trace `0` stopped, as its writer reported.
    TraceFailed(u64),
    /// Stop the runtime.
    Stop,
}

/// Where the saved applications, automations, and settings are.
struct Stores {
    apps: AppStore,
    automations: AutomationStore,
    settings: SettingsStore,
}

/// The threads and handles the owner carries orders out with.
struct Crew {
    system: Arc<HostSystem>,
    events: Sender<Event>,
    jobs: Sender<act::Job>,
    /// Detection's input.
    inputs: Sender<Input>,
    readers: BTreeMap<SessionId, Reader>,
    /// The trace, as the settings turn it on and off.
    tracing: Switch,
    /// The owner's own handle to the trace.
    trace: Trace,
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
        settings: SettingsStore::new(locations.settings()),
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
    let mut reloads = Reloads::default();
    let mut tracing = Switch::new(locations.traces());
    // No app is attached yet, so the first read starts no reader.
    let first = read_saved(
        &stores,
        log,
        &mut model,
        &mut reloads,
        (Stale::BOTH, &mut tracing, &sender),
    );
    let lifecycle = Lifecycle::new(first.watchlist.unwrap_or_default(), Instant::now());
    let detect = Arc::clone(&system);
    let changes = sender.clone();
    let detector = spawn("lifecycle", move || {
        lifecycle::run(
            lifecycle,
            &input_events,
            &changes,
            || detect.processes(),
            || detect.windows(),
            |window| shows(detect.window_place(window)),
        );
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a failed send means the owner already stopped, which is what this tells it"
        )]
        let _ = changes.send(Event::DetectionEnded);
    });
    let (jobs, job_events) = mpsc::channel();
    let reports = sender.clone();
    let actor = spawn("actions", move || {
        act::run(controls, &job_events, &reports);
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
        inputs,
        readers: BTreeMap::new(),
        tracing,
        trace: Trace::OFF,
    };
    // The other threads exist only now, so they get a trace that started
    // with the first read here; the first readers start with it.
    if let Some(trace) = first.trace {
        tell(trace, &mut crew);
    }
    carry(first.orders, &mut crew, log);
    loop {
        let due = wake(model.next_due(), reloads.retry, crew.tracing.until());
        let Some(event) = next(&events, due) else {
            break;
        };
        let received = Instant::now();
        let (kind, cause) = (event.kind(), event.cause());
        receive(&mut crew.trace, &model, &event, received);
        let read = match handle(&mut model, log, event) {
            Step::Carry(orders) => {
                carry(orders, &mut crew, log);
                reloads.due(Instant::now())
            },
            Step::Reload => {
                // A save starts the tries over.
                reloads.retry = None;
                Some(Stale::BOTH)
            },
            Step::TraceFailed(id) => {
                if let Some(trace) = crew.tracing.failed(id, log) {
                    retrace(trace, &mut model, &mut crew, log);
                }
                None
            },
            Step::Stop => break,
        };
        if let Some(trace) = crew.tracing.expire(Instant::now(), log) {
            retrace(trace, &mut model, &mut crew, log);
        }
        if let Some(read) = read {
            let started = Instant::now();
            let saved = read_saved(
                &stores,
                log,
                &mut model,
                &mut reloads,
                (read, &mut crew.tracing, &crew.events),
            );
            if let Some(trace) = saved.trace {
                tell(trace, &mut crew);
            }
            if let Some(watchlist) = saved.watchlist
                && crew.inputs.send(Input::Watch(watchlist)).is_err()
            {
                break;
            }
            carry(saved.orders, &mut crew, log);
            let took = started.elapsed();
            crew.trace.record(What::Reloaded, |record| Record {
                took: Some(took),
                ..record
            });
        }
        let took = received.elapsed();
        crew.trace.record(What::Handled, |record| Record {
            took: Some(took),
            cause,
            detail: Detail::Note(kind),
            ..record
        });
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
    let Crew {
        jobs,
        inputs,
        readers,
        tracing,
        ..
    } = crew;
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
    // Last, so the trace holds what the other threads did up to their end.
    let traced = tracing.finish();
    detected
        .and(acted)
        .and(read)
        .and(tray_stopped)
        .and(reload_stopped)
        .and(windows_stopped)
        .and(traced)
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

    #[mutants::skip]
    fn open(&mut self, _target: &appytappy_config::action::OpenTarget) -> io::Result<()> {
        match self.0 {}
    }

    #[mutants::skip]
    fn play_sound(&mut self, _file: &std::path::Path) -> io::Result<bool> {
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

/// Which saved files the owner reads: the settings, the applications and
/// automations, or both.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Stale {
    /// The applications and automations.
    files: bool,
    /// The settings.
    settings: bool,
}

impl Stale {
    /// Every saved file.
    const BOTH: Self = Self {
        files: true,
        settings: true,
    };
}

/// What the owner knows about reading the saved files: when it tries again,
/// which files that try reads, and what it has read so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Reloads {
    /// When it reads again the files that did not read.
    retry: Option<Retry>,
    /// The files that did not read at the last try.
    failing: Stale,
    /// Whether the applications and automations ever read.
    had: Had,
    /// Whether the settings ever read.
    had_settings: bool,
}

impl Default for Reloads {
    /// Before the first read.
    fn default() -> Self {
        Self {
            retry: None,
            failing: Stale::default(),
            had: Had::Nothing,
            had_settings: false,
        }
    }
}

impl Reloads {
    /// The files to read again at `now`, when a retry is due.
    fn due(&self, now: Instant) -> Option<Stale> {
        self.retry
            .is_some_and(|retry| retry.due(now))
            .then_some(self.failing)
    }
}

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
/// rule is due, `retry`, when it reads the saved files again, and
/// `trace_ends`, when the trace's time is up.
fn wake(
    next_due: Option<Instant>,
    retry: Option<Retry>,
    trace_ends: Option<Instant>,
) -> Option<Instant> {
    [next_due, retry.map(|retry| retry.at), trace_ends]
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

impl Had {
    /// What the runtime does meanwhile, as the log says it.
    const fn keeps(self) -> &'static str {
        match self {
            Self::Nothing => "runs no automations",
            Self::Plan => "keeps running what it ran",
        }
    }
}

/// What the runtime uses while the settings cannot be read at start.
const DEFAULTS: &str = "uses the default settings";

/// What the runtime uses while the settings cannot be read later.
const KEPT: &str = "keeps its settings";

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

/// What a read of the saved files gives the owner to put to use.
#[derive(Debug)]
struct Saved {
    /// The applications to watch for, when they read.
    watchlist: Option<Watchlist>,
    /// The orders that put what was read to use.
    orders: Vec<Order>,
    /// The trace every thread records with from now on, when the settings
    /// changed it.
    trace: Option<Trace>,
}

/// Reads the saved files that `read` names, at start, after a save, or for
/// a retry, and returns what to put to use. `tracing` starts or stops the
/// trace as the settings say, and its writer reports a failure to
/// `reports`.
///
/// The settings come first, so the readers that a new plan starts read at
/// the new interval and with the new trace; then the plan; then the readers
/// the plan kept running start over with both, so no reader starts twice.
/// Files that do not read are kept as they were: the model runs the plan
/// and settings it has, and `reloads` says when to read those files again,
/// if at all.
fn read_saved(
    stores: &Stores,
    log: &mut Log,
    model: &mut Model,
    reloads: &mut Reloads,
    (read, tracing, reports): (Stale, &mut Switch, &Sender<Event>),
) -> Saved {
    let now = Instant::now();
    let previous = reloads.retry;
    let mut retry = None;
    let mut failing = Stale::default();
    let mut trace = None;
    if read.settings {
        match read_settings(&stores.settings) {
            Ok(settings) => {
                reloads.had_settings = true;
                let interval = read_interval(settings.read_interval);
                if interval != model.interval() {
                    note(log, &interval_line(interval));
                }
                trace = tracing.apply(&settings.trace, (now, unix_ms()), reports, log);
                let reading = trace.clone().unwrap_or_else(|| model.trace().clone());
                model.set_reading(interval, reading);
            },
            Err(error) => {
                failing.settings = true;
                let keeps = if reloads.had_settings { KEPT } else { DEFAULTS };
                retry = failed(log, &error, previous, now, keeps);
            },
        }
    }
    let mut watchlist = None;
    let mut orders = Vec::new();
    if read.files {
        match load(stores, log) {
            Ok(plan) => {
                reloads.had = Had::Plan;
                watchlist = Some(plan.watchlist());
                orders = model.replan(plan.apps);
            },
            Err(error) => {
                failing.files = true;
                retry = failed(log, &error, previous, now, reloads.had.keeps());
            },
        }
    }
    orders.extend(model.restart_readers());
    reloads.retry = retry;
    reloads.failing = failing;
    Saved {
        watchlist,
        orders,
        trace,
    }
}

/// Milliseconds since 1970, as the settings give the trace's end; 0 should
/// the clock be set before then.
fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        })
}

/// Gives every thread `trace` after it stopped on its own, the readers by
/// starting them over.
#[mutants::skip] // Starts and stops threads; the switch and the model are tested.
fn retrace(trace: Trace, model: &mut Model, crew: &mut Crew, log: &mut Log) {
    tell(trace.clone(), crew);
    model.set_reading(model.interval(), trace);
    let orders = model.restart_readers();
    carry(orders, crew, log);
}

/// Gives the owner, the action thread, and detection `trace`. The readers
/// get it from the model when they start over.
#[mutants::skip] // Sends to threads; checked by the trace's tests.
fn tell(trace: Trace, crew: &mut Crew) {
    #[expect(
        clippy::let_underscore_must_use,
        reason = "the action thread and detection end only after the owner, so neither send fails while it runs"
    )]
    let _ = (
        crew.jobs.send(act::Job::Trace(trace.clone())),
        crew.inputs.send(Input::Trace(trace.clone())),
    );
    crew.trace = trace;
}

/// Records that the owner received a changed value, `event`, at `now`.
fn receive(trace: &mut Trace, model: &Model, event: &Event, now: Instant) {
    if let Event::Reading(Reading::Sample {
        session,
        signal,
        value,
        at,
    }) = event
    {
        trace.record(What::Received, |record| Record {
            at: now,
            session: Some(*session),
            item: model.value_position(*session, signal),
            value: Some(value.clone()),
            took: Some(now.saturating_duration_since(*at)),
            cause: Some(*at),
            ..record
        });
    }
}

/// Logs that the saved files cannot be read for `error`, and what the
/// runtime `keeps` doing meanwhile, and returns when to read them again:
/// soon, unless `previous`, the try that just failed, was the last.
fn failed(
    log: &mut Log,
    error: &str,
    previous: Option<Retry>,
    now: Instant,
    keeps: &str,
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
    note(log, &format!("{error}, so the runtime {keeps} and {then}"));
    retry
}

/// Reads the settings.
///
/// # Errors
///
/// Returns why the settings cannot be read, as a line for the log.
fn read_settings(store: &SettingsStore) -> Result<SettingsFile, String> {
    store
        .load()
        .map_err(|error| format!("the settings cannot be read: {error}"))
}

/// The read interval `saved` asks for. A saved file is checked, so its
/// fastest read is never slower than its slowest; were it, the default
/// stands in.
fn read_interval(saved: ReadIntervalDef) -> ReadInterval {
    ReadInterval::new(saved.fastest(), saved.slowest()).unwrap_or_default()
}

/// The log line for a new read interval.
fn interval_line(interval: ReadInterval) -> String {
    format!(
        "values are read every {} to {} ms",
        interval.min().as_millis(),
        interval.max().as_millis()
    )
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
                crew.tracing.name(id, &job.plan);
                crew.trace.record(What::Reading, |record| Record {
                    session: Some(id),
                    ..record
                });
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
                crew.trace.record(What::Stopped, |record| Record {
                    session: Some(id),
                    ..record
                });
            },
            Order::Act(job) => {
                let decided = match &job {
                    act::Job::Run { owner, cause, .. } => Some((What::Fired, *owner, *cause)),
                    act::Job::Undo { owner, cause, .. } => Some((What::Released, *owner, *cause)),
                    act::Job::Trace(_) => None,
                };
                if let Some((what, owner, cause)) = decided {
                    record_rule(&mut crew.trace, what, owner, cause);
                }
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the action thread ends only after the owner drops the jobs, so a send cannot fail while it runs"
                )]
                let _ = crew.jobs.send(job);
                if let Some((_, owner, cause)) = decided {
                    record_rule(&mut crew.trace, What::Queued, owner, cause);
                }
            },
            Order::Held(owner, cause) => record_rule(&mut crew.trace, What::Held, owner, cause),
        }
    }
}

/// Records `what` happened to the automation `owner` names, for the
/// reading of the frame that arrived at `cause`.
fn record_rule(trace: &mut Trace, what: What, (session, rule): act::Owner, cause: Option<Instant>) {
    trace.record(what, |record| Record {
        session: Some(session),
        item: Some(rule.0),
        cause,
        ..record
    });
}

/// The log line for an order, if it gets one.
fn order_line(order: &Order) -> Option<String> {
    match order {
        Order::Read(job) => Some(format!("{}: reading its values", job.plan.name)),
        Order::StopReading(..) | Order::Held(..) | Order::Act(act::Job::Trace(_)) => None,
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
                "the setup app saved, so the settings and automations are read again",
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
                        "{name}: its values can no longer be seen, so they are read \
                         again before any automation runs or is undone"
                    ),
                );
            }
            model.forget(session);
            Step::Carry(Vec::new())
        },
        Event::Actions(ActionReport::Note(message)) => {
            note(log, &message);
            Step::Carry(Vec::new())
        },
        Event::Actions(ActionReport::CanRun(can_run)) => {
            model.set_actions_can_run(can_run);
            Step::Carry(Vec::new())
        },
        Event::Trace(TraceReport::Failed { id, message }) => {
            note(log, &message);
            Step::TraceFailed(id)
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
        let failed = Event::from(ActionReport::Note(
            "Pause: action 1 failed: no player".to_owned(),
        ));
        let cannot_run = Event::from(ActionReport::CanRun(false));
        let disable = Event::from(TrayEvent::Picked(tray::DISABLE));
        let unknown = Event::from(TrayEvent::Picked(NonZeroU16::MAX));
        let ended = Event::from(TrayEvent::Ended("its window was destroyed".to_owned()));
        let mut steps = Vec::new();
        for event in [
            started, problem, failed, cannot_run, disable, unknown, ended,
        ] {
            steps.push(handle(&mut model, &mut log, event));
        }
        let disabled = model.enabled();
        let could_run = model.actions_can_run();
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
                carry(),
                Step::Reload,
                Step::Stop,
                Step::Stop,
                Step::Stop,
            ]
        );
        assert!(!could_run, "the action thread said actions cannot run");
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
            "the setup app saved, so the settings and automations are read again",
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

    /// The saved files in a scratch folder, which it removes when dropped.
    struct Scratch {
        folder: std::path::PathBuf,
        stores: Stores,
        log: Log,
        tracing: Switch,
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let folder = env::temp_dir().join(format!("appytappy-{name}-{}", process::id()));
            drop(fs::remove_dir_all(&folder));
            fs::create_dir_all(&folder).expect("make the folder");
            let stores = Stores {
                apps: AppStore::new(folder.join("apps")),
                automations: AutomationStore::new(folder.join("automations.json")),
                settings: SettingsStore::new(folder.join("settings.json")),
            };
            let log = Log::open(&folder.join("logs"), log::LIMIT).expect("open");
            let tracing = Switch::new(folder.join("traces"));
            Self {
                folder,
                stores,
                log,
                tracing,
            }
        }

        fn write(&self, file: &str, text: &str) {
            fs::write(self.folder.join(file), text).expect("write");
        }

        fn log(&self) -> String {
            fs::read_to_string(self.folder.join("logs").join("runtime.log")).expect("read the log")
        }

        fn read(&mut self, model: &mut Model, reloads: &mut Reloads, read: Stale) -> Saved {
            let (reports, _received) = mpsc::channel();
            read_saved(
                &self.stores,
                &mut self.log,
                model,
                reloads,
                (read, &mut self.tracing, &reports),
            )
        }

        fn reload(&mut self, model: &mut Model, reloads: &mut Reloads, read: Stale) -> Vec<Order> {
            self.read(model, reloads, read).orders
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            drop(fs::remove_dir_all(&self.folder));
        }
    }

    const NO_AUTOMATIONS: &str = "{\"format\":1,\"automations\":[]}";

    const SETTINGS_ONLY: Stale = Stale {
        files: false,
        settings: true,
    };

    #[test]
    fn settings_set_the_read_interval_and_unreadable_ones_are_kept() {
        let mut scratch = Scratch::new("settings");
        let mut model = Model::default();
        let mut reloads = Reloads::default();
        assert_eq!(scratch.reload(&mut model, &mut reloads, SETTINGS_ONLY), []);
        assert_eq!(
            model.interval(),
            ReadInterval::DEFAULT,
            "no file is the defaults"
        );
        scratch.write(
            "settings.json",
            r#"{"format":1,"readInterval":{"fastestMs":40,"slowestMs":2000}}"#,
        );
        scratch.reload(&mut model, &mut reloads, SETTINGS_ONLY);
        let slower = ReadInterval::new(Duration::from_millis(40), Duration::from_secs(2));
        assert_eq!(Some(model.interval()), slower);
        scratch.write("settings.json", "not JSON");
        scratch.reload(&mut model, &mut reloads, SETTINGS_ONLY);
        assert_eq!(Some(model.interval()), slower, "the settings are kept");
        assert_eq!(
            reloads.due(Instant::now() + RETRY_WAIT),
            Some(SETTINGS_ONLY),
            "only the settings are read again, soon"
        );
        let written = scratch.log();
        let lines: Vec<&str> = written.lines().collect();
        assert_eq!(lines.len(), 2, "{written:?}");
        assert!(
            lines[0].ends_with("values are read every 40 to 2000 ms"),
            "{written:?}"
        );
        assert!(
            lines[1].contains("the settings cannot be read: the settings file is not valid: "),
            "{written:?}"
        );
        assert!(
            lines[1]
                .ends_with(", so the runtime keeps its settings and reads them again in 2 seconds"),
            "{written:?}"
        );
    }

    #[test]
    fn settings_that_never_read_leave_the_defaults() {
        let mut scratch = Scratch::new("first-settings");
        scratch.write("settings.json", "not JSON");
        scratch.write("automations.json", NO_AUTOMATIONS);
        let mut model = Model::default();
        let mut reloads = Reloads::default();
        let Saved {
            watchlist, orders, ..
        } = scratch.read(&mut model, &mut reloads, Stale::BOTH);
        assert_eq!(orders, []);
        assert!(watchlist.is_some(), "the files read");
        assert_eq!(model.interval(), ReadInterval::DEFAULT);
        assert_eq!(reloads.failing, SETTINGS_ONLY);
        let written = scratch.log();
        assert!(
            written.contains(
                ", so the runtime uses the default settings and reads them again in 2 seconds\n"
            ),
            "{written:?}"
        );
        // A retry reads the settings only, so the plan is not logged again.
        scratch.write("settings.json", r#"{"format":1}"#);
        let retry = reloads.due(Instant::now() + RETRY_WAIT).expect("a retry");
        assert_eq!(scratch.reload(&mut model, &mut reloads, retry), []);
        assert_eq!(reloads.retry, None);
        assert_eq!(
            scratch.log().matches("no saved automation can run").count(),
            1,
            "the files were read once"
        );
    }

    #[test]
    fn a_reload_that_changes_the_interval_restarts_only_the_readers_the_plan_keeps() {
        let mut scratch = Scratch::new("interval");
        scratch.write(
            "settings.json",
            r#"{"format":1,"readInterval":{"fastestMs":40,"slowestMs":2000}}"#,
        );
        scratch.write("automations.json", NO_AUTOMATIONS);
        let mut model = Model::default();
        model.replan(BTreeMap::from([(
            "a".to_owned(),
            app_plan("Example App", &["Mute"]),
        )]));
        let orders = model.apply(
            Change::Started {
                app: "a".to_owned(),
                process: ProcessId::new(7),
                executable: "Example.exe".to_owned(),
            },
            Instant::now(),
        );
        assert!(matches!(orders.first(), Some(Order::Read(_))));
        let mut reloads = Reloads::default();
        assert_eq!(
            scratch.reload(&mut model, &mut reloads, Stale::BOTH),
            [Order::StopReading(SessionId(0), "Example App".to_owned())],
            "the app the plan dropped stops and does not start again"
        );
    }

    #[test]
    fn settings_turn_the_trace_on_and_off_and_readers_start_over_with_it() {
        let mut scratch = Scratch::new("trace");
        scratch.write("settings.json", r#"{"format":1,"trace":{"on":true}}"#);
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
        let mut reloads = Reloads::default();
        let on = scratch.read(&mut model, &mut reloads, SETTINGS_ONLY);
        let trace = on.trace.expect("the trace starts");
        assert_ne!(trace, Trace::OFF);
        assert_eq!(*model.trace(), trace, "new readers start with it");
        let restarted: Vec<_> = on
            .orders
            .iter()
            .map(|order| match order {
                Order::StopReading(id, _) => format!("stop {}", id.0),
                Order::Read(job) => format!("read {} {}", job.session.0, job.trace == trace),
                other => format!("{other:?}"),
            })
            .collect();
        assert_eq!(restarted, ["stop 0", "read 0 true"]);
        let same = scratch.read(&mut model, &mut reloads, SETTINGS_ONLY);
        assert!(same.trace.is_none(), "the same settings keep the trace");
        assert_eq!(same.orders, [], "and restart no reader");
        scratch.write("settings.json", r#"{"format":1}"#);
        let off = scratch.read(&mut model, &mut reloads, SETTINGS_ONLY);
        assert_eq!(off.trace, Some(Trace::OFF));
        assert_eq!(*model.trace(), Trace::OFF);
        assert_eq!(off.orders.len(), 2, "the reader starts over without it");
        let written = scratch.log();
        assert!(written.contains("the trace records to "), "{written:?}");
        assert!(written.contains("the trace stopped\n"), "{written:?}");
    }

    #[test]
    fn a_reload_that_cannot_read_the_files_keeps_the_plan_and_tries_again() {
        let mut scratch = Scratch::new("reload");
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
        scratch.write("automations.json", "not JSON");
        let mut reloads = Reloads {
            had: Had::Plan,
            ..Reloads::default()
        };
        let mut tries = Vec::new();
        let mut read = Stale::BOTH;
        for _ in 0..=RETRIES {
            assert_eq!(scratch.reload(&mut model, &mut reloads, read), []);
            tries.push(reloads.retry.map(|retry| retry.left));
            if let Some(again) = reloads.due(Instant::now() + RETRY_WAIT) {
                read = again;
            }
        }
        assert_eq!(
            read,
            Stale {
                files: true,
                settings: false
            },
            "the settings read"
        );
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
            reloads.due(Instant::now() + RETRY_WAIT),
            None,
            "after the last try only a save reads the files again"
        );
        // A save starts the tries over; both files failing share one try.
        scratch.write("settings.json", "not JSON");
        reloads.retry = None;
        scratch.reload(&mut model, &mut reloads, Stale::BOTH);
        assert_eq!(reloads.retry.map(|retry| retry.left), Some(4));
        assert_eq!(reloads.failing, Stale::BOTH);
        scratch.write("settings.json", r#"{"format":1}"#);
        scratch.write("automations.json", NO_AUTOMATIONS);
        let Saved {
            watchlist, orders, ..
        } = scratch.read(&mut model, &mut reloads, Stale::BOTH);
        assert!(watchlist.is_some(), "the files read");
        assert_eq!(reloads.retry, None, "a read that works needs no retry");
        assert_eq!(
            orders,
            [Order::StopReading(SessionId(0), "Example App".to_owned())]
        );
        let written = scratch.log();
        assert!(
            written.contains("the automations cannot be read: the automations file is not valid: "),
            "{written:?}"
        );
        assert_eq!(
            written
                .matches(
                    ", so the runtime keeps running what it ran and reads them again in 2 seconds\n"
                )
                .count(),
            6,
            "five tries, then the save's: {written:?}"
        );
        assert!(
            written.contains(
                ", so the runtime keeps running what it ran and waits for the next save\n"
            ),
            "{written:?}"
        );
        assert!(
            written.contains(
                ", so the runtime keeps its settings and reads them again in 2 seconds\n"
            ),
            "both failures are logged: {written:?}"
        );
        assert!(
            written.ends_with("no saved automation can run\n"),
            "{written:?}"
        );
    }

    #[test]
    fn the_first_read_that_fails_runs_nothing() {
        let mut scratch = Scratch::new("start");
        scratch.write("automations.json", "not JSON");
        let mut model = Model::default();
        let mut reloads = Reloads::default();
        scratch.reload(&mut model, &mut reloads, Stale::BOTH);
        assert_eq!(reloads.had, Had::Nothing);
        assert!(
            scratch.log().contains(
                ", so the runtime runs no automations and reads them again in 2 seconds\n"
            )
        );
    }

    #[test]
    fn a_retry_alone_wakes_the_owner() {
        let now = Instant::now();
        let soon = now.checked_add(Duration::from_secs(1)).expect("soon");
        let later = now.checked_add(Duration::from_secs(3)).expect("later");
        let retry = |at| Retry { at, left: 2 };
        assert_eq!(wake(None, None, None), None);
        assert_eq!(wake(None, Some(retry(soon)), None), Some(soon));
        assert_eq!(wake(Some(later), Some(retry(soon)), None), Some(soon));
        assert_eq!(wake(Some(soon), Some(retry(later)), None), Some(soon));
        assert_eq!(wake(Some(soon), None, None), Some(soon));
        assert_eq!(wake(None, None, Some(soon)), Some(soon), "the trace ends");
        assert_eq!(
            wake(Some(later), Some(retry(later)), Some(soon)),
            Some(soon)
        );
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
                process: appytappy_config::anti_cheat::ProcessId::new(1),
                executable: "Example.exe".to_owned(),
                interval: ReadInterval::DEFAULT,
                trace: Trace::OFF,
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
                cause: None,
            }),
            Order::Act(act::Job::Undo {
                owner,
                automation: "Mute".to_owned(),
                cause: None,
            }),
            Order::Held(owner, None),
            Order::Act(act::Job::Trace(Trace::OFF)),
        ];
        assert_eq!(
            orders.each_ref().map(order_line),
            [
                Some("Example App: reading its values".to_owned()),
                None,
                Some("Mute: runs".to_owned()),
                Some("Mute: undone".to_owned()),
                None,
                None,
            ]
        );
    }
}
