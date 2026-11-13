//! The owner thread: starts the other threads, then turns each event they send
//! into changes to the root model and orders for the workers, until the runtime
//! quits.

use std::collections::BTreeMap;
use std::fmt;
use std::io;
use std::mem;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SendError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use appytappy_actions::Watched;
use appytappy_config::app_file::BuildDef;
use appytappy_config::paths::Locations;
use appytappy_config::settings_file::{ReadInterval, SettingsFile, TraceCategory};
use appytappy_config::store::{AppStore, AutomationStore, EnabledStore, SettingsStore};
use appytappy_platform::{
    Log, RELOAD, ReloadRequest, ReloadWatch, Tray, TrayEvent, TrayMenu, WindowWatch,
};
use appytappy_sources::HostSystem;
use appytappy_sources::system::{System, shows};

use crate::lifecycle::{Change, Input, Lifecycle, Watchlist};
use crate::model::{Model, Order, Owner, ReaderNumber, SessionId};
use crate::trace::switch::Switch;
use crate::trace::writer::TraceReport;
use crate::trace::{Detail, Record, Trace, What};
use crate::workers::act::ActionReport;
use crate::workers::reader::{self, CHECK_AGAIN, Reader, Reading};
use crate::workers::{act, join, spawn, tell_end};
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
    /// A reader read a value, met a problem, or ended.
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
    /// The user picked Enable or Disable from the tray: carry out these
    /// orders, and from now on in this run the settings no longer start
    /// automations as the tray last left them.
    Picked(Vec<Order>),
    /// Read the saved automations again.
    Reload,
    /// Trace `0` stopped, as its writer reported.
    TraceFailed(u64),
    /// Stop the runtime.
    Stop,
}

/// Where the saved applications, automations, and settings are, and where
/// the runtime keeps whether automations are enabled.
struct Stores {
    apps: AppStore,
    automations: AutomationStore,
    settings: SettingsStore,
    enabled: EnabledStore,
}

/// The action thread and the channel that brings it jobs.
struct Actor {
    jobs: Sender<act::Control>,
    thread: JoinHandle<()>,
}

impl Actor {
    /// Starts the action thread. It makes the controls on `system`, and
    /// sends its notes, and whether actions can run, to `reports`.
    fn start(system: &Arc<HostSystem>, reports: Sender<Event>) -> io::Result<Self> {
        let (jobs, job_events) = mpsc::channel();
        let acting = Arc::clone(system);
        let thread = spawn("actions", move || {
            act::run(|| controls(&acting), acting.as_ref(), &job_events, &reports);
        })?;
        Ok(Self { jobs, thread })
    }

    /// Ends the action thread once it has run what is queued, and waits for
    /// it.
    ///
    /// # Errors
    ///
    /// Returns an error when the thread panicked.
    fn stop(self) -> io::Result<()> {
        drop(self.jobs);
        join(self.thread)
    }
}

/// The threads and handles the owner carries orders out with.
///
/// Every send to the action thread and to detection goes through `act` and
/// `detect`, which hold what a send that fails means: the thread it went to
/// ended, because each ends only after the owner, or when it panics.
struct Crew {
    system: Arc<HostSystem>,
    events: Sender<Event>,
    actor: Actor,
    /// Detection's input.
    inputs: Sender<Input>,
    readers: BTreeMap<SessionId, Reader>,
    /// The trace, as the settings turn it on and off.
    tracing: Switch,
    /// The owner's own handle to the trace, which each reader it starts
    /// records to as well.
    trace: Trace,
}

impl Crew {
    /// Has every thread that records record to `trace` from now on: the
    /// owner, the action thread, detection, and each reader, which keeps its
    /// capture and what it read.
    fn set_trace(&mut self, trace: Trace, log: &mut Log) {
        self.act(act::Control::Trace(trace.clone()), log);
        self.detect(Input::Trace(trace.clone()));
        for reader in self.readers.values() {
            reader.set_trace(trace.clone());
        }
        self.trace = trace;
    }

    /// Hands `control` to the action thread. A send fails only when the
    /// thread panicked, so then the log says why, a new action thread starts
    /// with the owner's trace, and `control` goes to it. The jobs the old
    /// thread had not run are dropped, and what it changed is not undone,
    /// because its journal ended with it. When no new thread starts,
    /// `control` is dropped, and the next send tries again.
    fn act(&mut self, control: act::Control, log: &mut Log) {
        let Err(SendError(control)) = self.actor.jobs.send(control) else {
            return;
        };
        let started = Actor::start(&self.system, self.events.clone());
        let ended = match started {
            Ok(actor) => mem::replace(&mut self.actor, actor).stop(),
            Err(error) => {
                note(
                    log,
                    &format!(
                        "the actions thread ended and cannot start again now, so what it was \
                         sent is dropped and the next action tries again: {error}"
                    ),
                );
                return;
            },
        };
        let why = ended.map_or_else(
            |error| error.to_string(),
            |()| "the actions thread ended".to_owned(),
        );
        note(
            log,
            &format!(
                "{why}, so it starts again; the jobs it had not run are dropped, and what it \
                 changed before is not undone"
            ),
        );
        let sent = self
            .actor
            .jobs
            .send(act::Control::Trace(self.trace.clone()))
            .and_then(|()| self.actor.jobs.send(control));
        if sent.is_err() {
            note(
                log,
                "the actions thread ended again as it started, so what it was sent is dropped",
            );
        }
    }

    /// Hands `input` to detection. A send fails only when detection ended,
    /// so the owner then hears that, as detection also tells it itself, and
    /// stops.
    fn detect(&self, input: Input) {
        if self.inputs.send(input).is_ok() {
            return;
        }
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the owner holds the receiver while it runs, so the send cannot fail"
        )]
        let _ = self.events.send(Event::DetectionEnded);
    }

    /// Keeps the reader `started` for `session`, its reader number `number`.
    /// When its thread could not start, the log says so for the application
    /// named `name`, and the owner hears that the reader ended, as a reader
    /// that ran tells it itself, so the session starts its next reader after
    /// a wait.
    fn read(
        &mut self,
        session: SessionId,
        number: ReaderNumber,
        started: io::Result<Reader>,
        name: &str,
        log: &mut Log,
    ) {
        match started {
            Ok(reader) => {
                self.readers.insert(session, reader);
                self.trace.record(What::Reading, |record| Record {
                    session: Some(session),
                    ..record
                });
            },
            Err(error) => {
                note(log, &format!("{name}: its reader cannot start: {error}"));
                let ended = Reading::Ended {
                    session,
                    reader: number,
                };
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the owner holds the receiver while it runs, so the send cannot fail"
                )]
                let _ = self.events.send(Event::Reading(ended));
            },
        }
    }

    /// The job that starts `session`'s reader, and the trace the reader
    /// starts with: the owner's own, so the reader records from its first
    /// frame whenever the trace is on.
    fn reader_job(
        &self,
        session: SessionId,
        reader: ReaderNumber,
        plan: Arc<plan::AppPlan>,
        watched: Watched,
        interval: ReadInterval,
    ) -> (reader::Job, Trace) {
        let job = reader::Job {
            session,
            reader,
            plan,
            watched,
            interval,
        };
        (job, self.trace.clone())
    }
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
        enabled: EnabledStore::new(locations.enabled()),
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
    // The first read can start automations disabled, as the tray last left
    // them, so the menu's check mark shows that before the first event.
    show(tray.as_ref(), &model, &mut shown);
    let lifecycle = Lifecycle::new(first.watchlist.unwrap_or_default(), Instant::now());
    let detect = Arc::clone(&system);
    let changes = sender.clone();
    let detector = spawn("lifecycle", move || {
        let detecting = || {
            lifecycle::run(
                lifecycle,
                &input_events,
                &changes,
                || detect.processes(),
                || detect.windows(),
                |window| shows(detect.window_place(window)),
                |process| detect.executable_build(process),
            );
        };
        tell_end(detecting, &changes, Event::DetectionEnded);
    });
    let actor = Actor::start(&system, sender.clone());
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
        actor,
        inputs,
        readers: BTreeMap::new(),
        tracing,
        trace: Trace::OFF,
    };
    // The other threads exist only now, so they get a trace that started
    // with the first read here.
    if let Some(trace) = first.trace {
        crew.set_trace(trace, log);
    }
    carry(first.orders, &mut crew, log);
    loop {
        let due = wake(model.next_due(), reloads.retry, crew.tracing.until());
        let Some(event) = next(&events, due) else {
            break;
        };
        // Off, the trace reads no clock here.
        let received = crew.trace.wants(TraceCategory::Rules).then(Instant::now);
        let (kind, cause) = (event.kind(), event.cause());
        receive(&mut crew.trace, &event);
        let step = handle(&mut model, log, &stores.enabled, event);
        reloads.after(&step);
        let read = match step {
            Step::Carry(orders) | Step::Picked(orders) => {
                carry(orders, &mut crew, log);
                reloads.due(Instant::now())
            },
            Step::Reload => Some(Stale::BOTH),
            Step::TraceFailed(id) => {
                if let Some(trace) = crew.tracing.failed(id, log) {
                    crew.set_trace(trace, log);
                }
                None
            },
            Step::Stop => break,
        };
        if let Some(trace) = crew.tracing.expire(Instant::now(), log) {
            crew.set_trace(trace, log);
        }
        if let Some(read) = read {
            let started = crew
                .trace
                .wants(TraceCategory::Lifecycle)
                .then(Instant::now);
            let saved = read_saved(
                &stores,
                log,
                &mut model,
                &mut reloads,
                (read, &mut crew.tracing, &crew.events),
            );
            if let Some(trace) = saved.trace {
                crew.set_trace(trace, log);
            }
            if let Some(watchlist) = saved.watchlist {
                crew.detect(Input::Watch(watchlist));
            }
            carry(saved.orders, &mut crew, log);
            crew.trace.record(What::Reloaded, |record| Record {
                took: started.map(|started| record.at.saturating_duration_since(started)),
                ..record
            });
        }
        crew.trace.record(What::Handled, |record| Record {
            took: received.map(|received| record.at.saturating_duration_since(received)),
            cause,
            detail: Detail::Note(kind),
            ..record
        });
        show(tray.as_ref(), &model, &mut shown);
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
        actor,
        inputs,
        readers,
        tracing,
        ..
    } = crew;
    let acted = actor.stop();
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

/// Shows on `tray`, if there is one, the menu the model calls for, when it
/// differs from `shown`, the menu shown last. Most events, such as each value
/// read, leave the menu as it was, and the icon is redrawn only when it
/// changes.
#[mutants::skip] // Only a real tray shows it, checked by hand on Windows.
fn show(tray: Option<&Tray>, model: &Model, shown: &mut TrayMenu) {
    let menu = tray::menu(model);
    if menu == *shown {
        return;
    }
    if let Some(tray) = tray {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a tray that ended reports why when it is stopped"
        )]
        let _ = tray.show(menu.clone());
    }
    *shown = menu;
}

/// The operating system's media and audio controls, made on the action
/// thread. They name each audio session's app from `system`'s process
/// list, which detection uses too, taken fresh for each volume action.
fn controls(system: &Arc<HostSystem>) -> io::Result<appytappy_actions::HostControls> {
    let system = Arc::clone(system);
    appytappy_actions::HostControls::new(Box::new(move || {
        system
            .processes()
            .map(appytappy_sources::system::executables)
    }))
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
    /// Whether the user picked Enable or Disable from the tray in this run,
    /// which outranks how the tray left them in an earlier run.
    picked: bool,
}

impl Default for Reloads {
    /// Before the first read.
    fn default() -> Self {
        Self {
            retry: None,
            failing: Stale::default(),
            had: Had::Nothing,
            had_settings: false,
            picked: false,
        }
    }
}

impl Reloads {
    /// Notes what `step` means for the reads to come, before the owner takes
    /// it: a pick from the tray outranks how the tray left automations in an
    /// earlier run, and a save starts the tries over.
    fn after(&mut self, step: &Step) {
        match step {
            Step::Picked(_) => self.picked = true,
            Step::Reload => self.retry = None,
            Step::Carry(_) | Step::TraceFailed(_) | Step::Stop => {},
        }
    }

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
/// the new interval; then the plan; then the readers the plan kept running
/// start over at the new interval, so no reader starts twice. Files that do
/// not read are kept as they were: the model runs the plan and settings it
/// has, and `reloads` says when to read those files again, if at all. Last,
/// the trace learns each running session's newest plan, since neither a
/// rename nor a new trace starts a reader over, and a new trace reaches the
/// threads only after this returns, so it names every row.
///
/// The first settings that read, even on a retry, decide whether
/// automations start enabled: when they say to remember, as the tray last
/// left them, and otherwise enabled, as the model starts. Settings read
/// later, or after the user picked Enable or Disable from the tray in this
/// run, leave it to the tray.
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
    let mut orders = Vec::new();
    if read.settings {
        match read_settings(&stores.settings) {
            Ok((settings, interval)) => {
                if settings.remember_enabled && !reloads.had_settings && !reloads.picked {
                    orders = remembered(&stores.enabled, model, log);
                }
                reloads.had_settings = true;
                if interval != model.interval() {
                    note(log, &interval_line(interval));
                }
                trace = tracing.apply(&settings.trace, (now, unix_ms()), reports, log);
                model.set_interval(interval);
            },
            Err(error) => {
                failing.settings = true;
                let keeps = if reloads.had_settings { KEPT } else { DEFAULTS };
                retry = failed(log, &error, previous, now, keeps);
            },
        }
    }
    let mut watchlist = None;
    if read.files {
        match load(stores, log) {
            Ok(plan) => {
                reloads.had = Had::Plan;
                watchlist = Some(Watchlist::new(plan.windows()));
                orders.extend(model.replan(plan.apps));
            },
            Err(error) => {
                failing.files = true;
                retry = failed(log, &error, previous, now, reloads.had.keeps());
            },
        }
    }
    orders.extend(model.restart_readers(now));
    for (id, plan) in model.sessions() {
        tracing.name(id, plan);
    }
    reloads.retry = retry;
    reloads.failing = failing;
    Saved {
        watchlist,
        orders,
        trace,
    }
}

/// Enables or disables automations as the tray last left them, as `store`
/// keeps it, and logs when that leaves them disabled.
fn remembered(store: &EnabledStore, model: &mut Model, log: &mut Log) -> Vec<Order> {
    let enabled = store.load();
    if !enabled {
        note(log, "automations disabled, as the tray last left them");
    }
    model.set_enabled(enabled)
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

/// Records that the owner received a changed value, `event`, now.
fn receive(trace: &mut Trace, event: &Event) {
    if let Event::Reading(Reading::Sample {
        session,
        position,
        value,
        at,
        ..
    }) = event
    {
        trace.record(What::Received, |record| Record {
            session: Some(*session),
            item: Some(*position),
            value: Some(value.clone()),
            took: Some(record.at.saturating_duration_since(*at)),
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

/// Reads the settings and the read interval they ask for.
///
/// # Errors
///
/// Returns why the settings cannot be read, as a line for the log.
fn read_settings(store: &SettingsStore) -> Result<(SettingsFile, ReadInterval), String> {
    let unreadable = |error: &dyn fmt::Display| format!("the settings cannot be read: {error}");
    let settings = store.load().map_err(|error| unreadable(&error))?;
    let interval = settings
        .read_interval
        .checked()
        .map_err(|error| unreadable(&error))?;
    Ok((settings, interval))
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
    let automations: usize = plan.apps.values().map(|app| app.runs.rules.len()).sum();
    let names: Vec<&str> = plan
        .apps
        .values()
        .map(|app| app.labels.name.as_str())
        .collect();
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
            Order::Read {
                session,
                reader,
                plan,
                watched,
                interval,
            } => {
                let name = plan.labels.name.clone();
                crew.tracing.name(session, &plan);
                let (job, trace) = crew.reader_job(session, reader, plan, watched, interval);
                let system = Arc::clone(&crew.system);
                let started = Reader::start(CHECK_AGAIN, system, job, trace, crew.events.clone());
                crew.read(session, reader, started, &name, log);
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
            Order::Run {
                owner,
                automation,
                watched,
                actions,
                clicks,
                restore,
                cause,
            } => {
                let job = act::Job::Run {
                    owner,
                    automation,
                    watched,
                    actions,
                    clicks,
                    restore,
                    cause,
                };
                queue(crew, What::Fired, job, log);
            },
            Order::Undo {
                owner,
                automation,
                cause,
            } => {
                let job = act::Job::Undo {
                    owner,
                    automation,
                    cause,
                };
                queue(crew, What::Released, job, log);
            },
            Order::Held(owner, cause) => record_rule(&mut crew.trace, What::Held, owner, cause),
        }
    }
}

/// Hands `job` to the action thread, and records that its rule `decided`,
/// fired or released, and that the job was queued.
fn queue(crew: &mut Crew, decided: What, job: act::Job, log: &mut Log) {
    let (owner, cause) = job.traced();
    record_rule(&mut crew.trace, decided, owner, cause);
    crew.act(act::Control::Job(job), log);
    record_rule(&mut crew.trace, What::Queued, owner, cause);
}

/// Records `what` happened to the automation `owner` names, for the
/// reading of the frame that arrived at `cause`.
fn record_rule(trace: &mut Trace, what: What, (session, rule): Owner, cause: Option<Instant>) {
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
        Order::Read { plan, .. } => Some(format!("{}: reading its values", plan.labels.name)),
        Order::StopReading(..) | Order::Held(..) => None,
        Order::Run { automation, .. } => Some(format!("{automation}: runs")),
        Order::Undo { automation, .. } => Some(format!("{automation}: undone")),
    }
}

/// Applies one event to the model and logs it. A pick of Enabled from the
/// tray is also kept in `enabled`, for the settings that remember it.
fn handle(model: &mut Model, log: &mut Log, enabled: &EnabledStore, event: Event) -> Step {
    match event {
        Event::Detected(change) => {
            if let Some(line) = describe(model, &change) {
                note(log, &line);
            }
            let before = model.state();
            let orders = model.apply(change);
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
            Some(tray::Command::SetEnabled(on)) => {
                note(
                    log,
                    if on {
                        "automations enabled"
                    } else {
                        "automations disabled"
                    },
                );
                // The pick applies before it is kept, so a slow disk does not
                // hold it up.
                let orders = model.set_enabled(on);
                if let Err(error) = enabled.save(on) {
                    note(
                        log,
                        &format!("whether automations are enabled cannot be kept: {error}"),
                    );
                }
                Step::Picked(orders)
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
            reader,
            position,
            value,
            at,
        }) => Step::Carry(model.sample(session, reader, position, value, at)),
        Event::Reading(Reading::Problem { session, problem }) => {
            if let Some(name) = model.app_name(session) {
                note(log, &format!("{name}: {problem}"));
            }
            Step::Carry(Vec::new())
        },
        Event::Reading(Reading::ValueProblem {
            session,
            value,
            problem,
        }) => {
            if let Some(plan) = model.session_plan(session) {
                let (app, value) = (&plan.labels.name, plan.value_name(value));
                note(log, &format!("{app}: {value} {problem}"));
            }
            Step::Carry(Vec::new())
        },
        Event::Reading(Reading::Unseen { session, reader }) => {
            if model.forget(session, reader)
                && let Some(name) = model.app_name(session)
            {
                note(
                    log,
                    &format!(
                        "{name}: its values can no longer be seen, so they are read \
                         again before any automation runs or is undone"
                    ),
                );
            }
            Step::Carry(Vec::new())
        },
        Event::Reading(Reading::Ended { session, reader }) => {
            let orders = model.reader_ended(session, reader, Instant::now());
            // No order means the session stopped that reader itself.
            if let Some(name) = model.app_name(session).filter(|_| !orders.is_empty()) {
                note(
                    log,
                    &format!(
                        "{name}: its reader ended, so nothing reads its values until a new \
                         reader starts after a wait"
                    ),
                );
            }
            Step::Carry(orders)
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

/// The log line for an app named `name` that runs `running` while its
/// values were checked against `checked`, or `None` when that needs no
/// line: the builds match, or no build was checked.
fn build_change(
    name: &str,
    checked: Option<&BuildDef>,
    running: &Result<BuildDef, String>,
) -> Option<String> {
    let checked = checked?;
    match running {
        Ok(running) if running == checked => None,
        Ok(running) => Some(format!(
            "{name}: its executable changed since its values were checked ({}). Its values are \
             still read; check them in AppyTappy Setup",
            build_difference(checked, running)
        )),
        Err(error) => Some(format!(
            "{name}: which build of its executable runs cannot be read, so a change since its \
             values were checked goes unnoticed: {error}"
        )),
    }
}

/// How `running` differs from the `checked` build, in a few words: both
/// versions when they differ, and otherwise that the file does.
fn build_difference(checked: &BuildDef, running: &BuildDef) -> String {
    let version = |build: &BuildDef| build.version.as_deref().unwrap_or("no version").to_owned();
    match (&checked.version, &running.version) {
        (then, now) if then != now => {
            format!("{} then, {} now", version(checked), version(running))
        },
        (Some(both), _) => format!("still {both}, but another file"),
        (None, _) => "another file, with no version either time".to_owned(),
    }
}

/// The log line for `change`, if it needs one.
fn describe(model: &Model, change: &Change) -> Option<String> {
    let line = match change {
        Change::Started {
            app,
            process,
            executable,
        } => format!("{app} started as {executable} (process {process}): attached"),
        Change::Exited { app, process } => format!("{app} exited (process {process}): detached"),
        Change::Identified { app, build } => {
            let plan = model.plan(app)?;
            return build_change(&plan.labels.name, plan.labels.checked.as_ref(), build);
        },
        Change::ScanFailed(problem) => format!("process snapshots fail: {problem}"),
        Change::ScanRestored => "process snapshots work again".to_owned(),
    };
    Some(line)
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
    use crate::model;
    use appytappy_platform::LOG_LIMIT;

    #[test]
    fn events_update_the_model_and_the_log_until_quit() {
        let folder = env::temp_dir().join(format!("appytappy-handle-{}", process::id()));
        let mut log = Log::open(&folder, "runtime", LOG_LIMIT).expect("open");
        let enabled = EnabledStore::new(folder.join("enabled.txt"));
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
            steps.push(handle(&mut model, &mut log, &enabled, event));
        }
        let disabled = model.enabled();
        let kept_disabled = enabled.load();
        let could_run = model.actions_can_run();
        let ends = [
            Event::from(TrayEvent::Picked(tray::ENABLE)),
            Event::from(ReloadRequest),
            Event::from(TrayEvent::Picked(tray::QUIT)),
            Event::from(TrayEvent::CloseRequested),
            Event::DetectionEnded,
        ];
        for event in ends {
            steps.push(handle(&mut model, &mut log, &enabled, event));
        }
        let kept_enabled = enabled.load();
        let written = fs::read_to_string(folder.join("runtime.log")).expect("read");
        fs::remove_dir_all(&folder).expect("clean up");
        let carry = || Step::Carry(Vec::new());
        let picked = || Step::Picked(Vec::new());
        assert_eq!(
            steps,
            [
                carry(),
                carry(),
                carry(),
                carry(),
                picked(),
                carry(),
                carry(),
                picked(),
                Step::Reload,
                Step::Stop,
                Step::Stop,
                Step::Stop,
            ]
        );
        assert!(!could_run, "the action thread said actions cannot run");
        assert!(!disabled, "Disable disables");
        assert!(!kept_disabled, "Disable is kept");
        assert!(model.enabled(), "Enable enables again");
        assert!(kept_enabled, "Enable is kept");
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
        .map(|change| describe(&Model::default(), &change));
        assert_eq!(
            lines,
            [
                "example started as Example.exe (process 42): attached",
                "example exited (process 42): detached",
                "process snapshots fail: denied",
                "process snapshots work again",
            ]
            .map(|line| Some(line.to_owned()))
        );
    }

    #[test]
    fn a_build_difference_names_both_versions_or_the_file() {
        let build = |version: Option<&str>, size| BuildDef {
            version: version.map(str::to_owned),
            size,
            modified: 1_762_588_800,
        };
        assert_eq!(
            build_difference(&build(Some("1.4.2.0"), 1), &build(Some("1.5.0.0"), 1)),
            "1.4.2.0 then, 1.5.0.0 now"
        );
        assert_eq!(
            build_difference(&build(None, 1), &build(Some("1.5.0.0"), 1)),
            "no version then, 1.5.0.0 now"
        );
        assert_eq!(
            build_difference(&build(Some("1.4.2.0"), 1), &build(Some("1.4.2.0"), 2)),
            "still 1.4.2.0, but another file"
        );
        assert_eq!(
            build_difference(&build(None, 1), &build(None, 2)),
            "another file, with no version either time"
        );
    }

    #[test]
    fn a_build_other_than_the_checked_one_gets_a_log_line() {
        let build = |version: Option<&str>, size| BuildDef {
            version: version.map(str::to_owned),
            size,
            modified: 1_762_588_800,
        };
        let checked = build(Some("1.4.2.0"), 1_024);
        let mut model = Model::default();
        let mut plan = app_plan("Example App", &["Pause music"]);
        Arc::get_mut(&mut plan).expect("unshared").labels.checked = Some(checked.clone());
        model.replan(BTreeMap::from([
            ("example".to_owned(), plan),
            ("unchecked".to_owned(), app_plan("Unchecked", &["Mute"])),
        ]));
        let line = |app: &str, running: Result<BuildDef, String>| {
            describe(
                &model,
                &Change::Identified {
                    app: app.to_owned(),
                    build: running,
                },
            )
        };
        assert_eq!(line("example", Ok(checked.clone())), None);
        assert_eq!(
            line("example", Ok(build(Some("1.5.0.0"), 1_024))).as_deref(),
            Some(
                "Example App: its executable changed since its values were checked (1.4.2.0 \
                 then, 1.5.0.0 now). Its values are still read; check them in AppyTappy Setup"
            )
        );
        // The same version in a file of another size is another build.
        assert_eq!(
            line("example", Ok(build(Some("1.4.2.0"), 2_048))).as_deref(),
            Some(
                "Example App: its executable changed since its values were checked (still \
                 1.4.2.0, but another file). Its values are still read; check them in \
                 AppyTappy Setup"
            )
        );
        assert_eq!(
            line("example", Ok(build(None, 2_048))).as_deref(),
            Some(
                "Example App: its executable changed since its values were checked (1.4.2.0 \
                 then, no version now). Its values are still read; check them in AppyTappy Setup"
            )
        );
        assert_eq!(
            line("example", Err("access denied".to_owned())).as_deref(),
            Some(
                "Example App: which build of its executable runs cannot be read, so a change \
                 since its values were checked goes unnoticed: access denied"
            )
        );
        assert_eq!(line("unchecked", Ok(checked.clone())), None);
        assert_eq!(line("gone", Ok(checked)), None);
    }

    #[test]
    fn a_value_s_problem_is_logged_with_its_newest_name() {
        let folder = env::temp_dir().join(format!("appytappy-value-names-{}", process::id()));
        let mut log = Log::open(&folder, "runtime", LOG_LIMIT).expect("open");
        let named = |app: &str, value: &str| {
            let mut plan = app_plan(app, &["Pause"]);
            Arc::get_mut(&mut plan).expect("unshared").labels.values = vec![value.to_owned()];
            BTreeMap::from([("example".to_owned(), plan)])
        };
        let mut model = Model::default();
        model.replan(named("Example App", "Progress"));
        let started = Event::from(Change::Started {
            app: "example".to_owned(),
            process: ProcessId::new(7),
            executable: "Example.exe".to_owned(),
        });
        let problem = || {
            Event::from(Reading::ValueProblem {
                session: SessionId(0),
                value: 0,
                problem: "cannot be read: too dark".to_owned(),
            })
        };
        let enabled = EnabledStore::new(folder.join("enabled.txt"));
        handle(&mut model, &mut log, &enabled, started);
        handle(&mut model, &mut log, &enabled, problem());
        assert_eq!(model.replan(named("Renamed", "Health")), []);
        handle(&mut model, &mut log, &enabled, problem());
        let written = fs::read_to_string(folder.join("runtime.log")).expect("read");
        fs::remove_dir_all(&folder).expect("clean up");
        let problems: Vec<&str> = written
            .lines()
            .filter(|line| line.contains("too dark"))
            .collect();
        assert_eq!(problems.len(), 2, "{written:?}");
        assert!(
            problems.first().is_some_and(
                |line| line.ends_with("Example App: Progress cannot be read: too dark")
            ),
            "{problems:?}"
        );
        assert!(
            problems
                .get(1)
                .is_some_and(|line| line.ends_with("Renamed: Health cannot be read: too dark")),
            "{problems:?}"
        );
    }

    #[test]
    fn only_the_reader_that_reads_a_session_is_logged_as_unseeing_or_ended() {
        let folder = env::temp_dir().join(format!("appytappy-reader-ended-{}", process::id()));
        let mut log = Log::open(&folder, "runtime", LOG_LIMIT).expect("open");
        let enabled = EnabledStore::new(folder.join("enabled.txt"));
        let mut model = Model::default();
        model.replan(BTreeMap::from([(
            "example".to_owned(),
            app_plan("Example App", &["Pause"]),
        )]));
        let started = Event::from(Change::Started {
            app: "example".to_owned(),
            process: ProcessId::new(7),
            executable: "Example.exe".to_owned(),
        });
        handle(&mut model, &mut log, &enabled, started);
        let session = SessionId(0);
        let unseen = |reader| Event::from(Reading::Unseen { session, reader });
        let ended = |reader| Event::from(Reading::Ended { session, reader });
        // The log's lines about the session's reader, without their times.
        let said = || -> Vec<String> {
            fs::read_to_string(folder.join("runtime.log"))
                .expect("read")
                .lines()
                .filter_map(|line| line.split_once("Example App: its "))
                .map(|(_, about)| about.to_owned())
                .collect()
        };
        let (stale, reads) = (ReaderNumber(4), ReaderNumber(0));
        let nothing = || Step::Carry(Vec::new());

        assert_eq!(
            handle(&mut model, &mut log, &enabled, unseen(stale)),
            nothing()
        );
        assert_eq!(
            handle(&mut model, &mut log, &enabled, ended(stale)),
            nothing()
        );
        assert_eq!(
            said(),
            [""; 0],
            "the session still runs, but another reader reads it"
        );
        assert_eq!(model.next_due(), None, "and no reader starts for it");

        assert_eq!(
            handle(&mut model, &mut log, &enabled, unseen(reads)),
            nothing()
        );
        assert_eq!(
            handle(&mut model, &mut log, &enabled, ended(reads)),
            Step::Carry(vec![Order::StopReading(session, "Example App".to_owned())]),
            "the reader that ended is joined, which logs why it ended"
        );
        assert!(model.next_due().is_some(), "a reader starts after a wait");
        let lines = said();
        fs::remove_dir_all(&folder).expect("clean up");
        assert_eq!(
            lines,
            [
                "values can no longer be seen, so they are read again before any automation \
                 runs or is undone",
                "reader ended, so nothing reads its values until a new reader starts after a \
                 wait",
            ]
        );
    }

    /// An app plan named `name` that reads no value, with an automation of
    /// each name in `automations`.
    fn app_plan(name: &str, automations: &[&str]) -> Arc<plan::AppPlan> {
        Arc::new(plan::AppPlan {
            labels: plan::Labels {
                name: name.to_owned(),
                values: Vec::new(),
                checked: None,
            },
            runs: plan::Runs {
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
                rules: automations
                    .iter()
                    .map(|&name| plan::examples::pause_at(name, 90.0, Duration::ZERO))
                    .collect(),
                clicks: Arc::default(),
            },
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
                enabled: EnabledStore::new(folder.join("enabled.txt")),
            };
            let log = Log::open(&folder.join("logs"), "runtime", LOG_LIMIT).expect("open");
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
        let orders = model.apply(Change::Started {
            app: "a".to_owned(),
            process: ProcessId::new(7),
            executable: "Example.exe".to_owned(),
        });
        assert!(matches!(orders.first(), Some(Order::Read { .. })));
        let mut reloads = Reloads::default();
        assert_eq!(
            scratch.reload(&mut model, &mut reloads, Stale::BOTH),
            [Order::StopReading(SessionId(0), "Example App".to_owned())],
            "the app the plan dropped stops and does not start again"
        );
    }

    const REMEMBER: &str = r#"{"format":1,"rememberEnabled":true}"#;

    #[test]
    fn the_first_settings_start_automations_as_the_tray_left_them_if_they_remember() {
        // Each case: whether the settings remember, what the enabled file
        // holds, and whether automations start enabled.
        for (settings, kept, starts) in [
            (r#"{"format":1}"#, Some("off\n"), true),
            (REMEMBER, Some("off\n"), false),
            (REMEMBER, Some("on\n"), true),
            (REMEMBER, None, true),
        ] {
            let mut scratch = Scratch::new("remember");
            scratch.write("settings.json", settings);
            if let Some(kept) = kept {
                scratch.write("enabled.txt", kept);
            }
            let mut model = Model::default();
            let mut reloads = Reloads::default();
            assert_eq!(scratch.reload(&mut model, &mut reloads, SETTINGS_ONLY), []);
            assert_eq!(model.enabled(), starts, "{settings} {kept:?}");
            assert_eq!(
                scratch
                    .log()
                    .contains("automations disabled, as the tray last left them"),
                !starts,
                "{settings} {kept:?}"
            );
        }
    }

    #[test]
    fn settings_read_later_leave_automations_as_the_tray_has_them() {
        let mut scratch = Scratch::new("remember-later");
        let mut model = Model::default();
        let mut reloads = Reloads::default();
        scratch.reload(&mut model, &mut reloads, SETTINGS_ONLY);
        scratch.write("settings.json", REMEMBER);
        scratch.write("enabled.txt", "off\n");
        scratch.reload(&mut model, &mut reloads, SETTINGS_ONLY);
        assert!(
            model.enabled(),
            "turning the setting on changes nothing now"
        );
    }

    /// A scratch folder whose settings remember and whose enabled file says
    /// off, and a model that runs one application, whose reader started.
    fn running_with_the_tray_left_off(name: &str) -> (Scratch, Model) {
        let scratch = Scratch::new(name);
        scratch.write("enabled.txt", "off\n");
        let mut model = Model::default();
        model.replan(BTreeMap::from([(
            "a".to_owned(),
            app_plan("Example App", &["Mute"]),
        )]));
        let orders = model.apply(Change::Started {
            app: "a".to_owned(),
            process: ProcessId::new(7),
            executable: "Example.exe".to_owned(),
        });
        assert!(matches!(orders.first(), Some(Order::Read { .. })));
        (scratch, model)
    }

    #[test]
    fn settings_that_first_read_late_stop_what_runs_when_the_tray_left_it_off() {
        let (mut scratch, mut model) = running_with_the_tray_left_off("remember-late");
        let mut reloads = Reloads::default();
        // The first read fails, so the first settings that work come with a
        // retry, while the application runs.
        scratch.write("settings.json", "{");
        scratch.reload(&mut model, &mut reloads, SETTINGS_ONLY);
        assert!(reloads.retry.is_some() && model.enabled());
        scratch.write("settings.json", REMEMBER);
        assert_eq!(
            scratch.reload(&mut model, &mut reloads, SETTINGS_ONLY),
            [Order::StopReading(SessionId(0), "Example App".to_owned())]
        );
        assert!(!model.enabled());
    }

    #[test]
    fn settings_that_first_read_after_a_tray_pick_leave_the_pick() {
        let (mut scratch, mut model) = running_with_the_tray_left_off("remember-picked");
        let mut reloads = Reloads::default();
        scratch.write("settings.json", "{");
        scratch.reload(&mut model, &mut reloads, SETTINGS_ONLY);
        // The pick cannot be kept, so the enabled file still says off.
        let unkept = EnabledStore::new(scratch.folder.join("enabled.txt").join("enabled.txt"));
        let enable = Event::from(TrayEvent::Picked(tray::ENABLE));
        let step = handle(&mut model, &mut scratch.log, &unkept, enable);
        assert_eq!(step, Step::Picked(Vec::new()));
        reloads.after(&step);
        scratch.write("settings.json", REMEMBER);
        assert_eq!(scratch.reload(&mut model, &mut reloads, SETTINGS_ONLY), []);
        assert!(model.enabled(), "the pick stands");
        assert!(reloads.had_settings && reloads.retry.is_none());
        assert!(
            !scratch
                .log()
                .contains("automations disabled, as the tray last left them")
        );
    }

    #[test]
    fn only_a_pick_marks_the_reloads_picked_and_only_a_save_ends_the_tries() {
        let retrying = || Reloads {
            retry: Some(Retry {
                at: Instant::now(),
                left: RETRIES,
            }),
            ..Reloads::default()
        };
        for step in [Step::Carry(Vec::new()), Step::TraceFailed(0), Step::Stop] {
            let mut reloads = retrying();
            reloads.after(&step);
            assert!(!reloads.picked && reloads.retry.is_some(), "{step:?}");
        }
        let mut reloads = retrying();
        reloads.after(&Step::Picked(Vec::new()));
        assert!(reloads.picked && reloads.retry.is_some());
        let mut reloads = retrying();
        reloads.after(&Step::Reload);
        assert!(!reloads.picked && reloads.retry.is_none());
    }

    #[test]
    fn a_tray_pick_that_cannot_be_kept_is_logged_and_still_applies() {
        let mut scratch = Scratch::new("remember-unkept");
        scratch.write("enabled.txt", "");
        let enabled = EnabledStore::new(scratch.folder.join("enabled.txt").join("enabled.txt"));
        let mut model = Model::default();
        let disable = Event::from(TrayEvent::Picked(tray::DISABLE));
        assert_eq!(
            handle(&mut model, &mut scratch.log, &enabled, disable),
            Step::Picked(Vec::new())
        );
        assert!(!model.enabled());
        let written = scratch.log();
        assert!(
            written.contains("whether automations are enabled cannot be kept: "),
            "{written:?}"
        );
    }

    #[test]
    fn settings_turn_the_trace_on_and_off_while_readers_keep_their_values() {
        let mut scratch = Scratch::new("trace");
        scratch.write("settings.json", r#"{"format":1,"trace":{"on":true}}"#);
        let mut model = Model::default();
        model.replan(BTreeMap::from([(
            "a".to_owned(),
            plan::examples::plan_with_cooldown("Example App", Duration::from_secs(60)),
        )]));
        model.apply(Change::Started {
            app: "a".to_owned(),
            process: ProcessId::new(7),
            executable: "Example.exe".to_owned(),
        });
        let at = Instant::now();
        for progress in [95.0, 10.0, 95.0] {
            model.sample(
                SessionId(0),
                ReaderNumber(0),
                0,
                plan::examples::number(progress),
                at,
            );
        }
        let held = model.next_due();
        assert!(held.is_some(), "held back by its cooldown");
        let mut reloads = Reloads::default();
        let on = scratch.read(&mut model, &mut reloads, SETTINGS_ONLY);
        let mut trace = on.trace.expect("the trace starts");
        assert_ne!(trace, Trace::OFF);
        assert_eq!(on.orders, [], "no reader starts over");
        // The session ran before the trace started, and its rows are named.
        trace.record(What::Reading, |record| Record {
            session: Some(SessionId(0)),
            ..record
        });
        let same = scratch.read(&mut model, &mut reloads, SETTINGS_ONLY);
        assert!(same.trace.is_none(), "the same settings keep the trace");
        assert_eq!(same.orders, []);
        scratch.write("settings.json", r#"{"format":1}"#);
        let off = scratch.read(&mut model, &mut reloads, SETTINGS_ONLY);
        assert_eq!(off.trace, Some(Trace::OFF));
        assert_eq!(off.orders, [], "the reader keeps reading without it");
        assert_eq!(model.next_due(), held, "the values read before are kept");
        let sessions: Vec<SessionId> = model.sessions().map(|(id, _)| id).collect();
        assert_eq!(sessions, [SessionId(0)], "the same session reads on");
        let written = scratch.log();
        assert!(written.contains("the trace records to "), "{written:?}");
        assert!(written.contains("the trace stopped\n"), "{written:?}");
        let folder = scratch.folder.join("traces");
        let tracing = std::mem::replace(&mut scratch.tracing, Switch::new(folder.clone()));
        crate::trace::within(move || tracing.finish()).expect("the writer stops");
        let rows: Vec<String> = fs::read_dir(&folder)
            .expect("the trace's folder")
            .map(|file| fs::read_to_string(file.expect("a file").path()).expect("read"))
            .collect();
        assert!(
            rows.iter()
                .any(|rows| rows.contains(",owner,reading,Example App,")),
            "{rows:?}"
        );
    }

    #[test]
    fn a_trace_that_ends_on_its_own_stops_and_a_late_failure_changes_nothing() {
        let mut scratch = Scratch::new("trace-ends");
        scratch.write(
            "settings.json",
            r#"{"format":1,"trace":{"on":true,"until":9999999999999}}"#,
        );
        let mut model = Model::default();
        let mut reloads = Reloads::default();
        let on = scratch.read(&mut model, &mut reloads, SETTINGS_ONLY);
        assert!(on.trace.is_some_and(|trace| trace != Trace::OFF));
        let until = scratch.tracing.until().expect("an end time");
        assert_eq!(
            scratch.tracing.expire(until, &mut scratch.log),
            Some(Trace::OFF),
            "the trace's time is up"
        );
        assert_eq!(
            scratch.tracing.failed(1, &mut scratch.log),
            None,
            "a late failure report from the trace that ended changes nothing"
        );
    }

    /// A crew whose threads are only channels: it returns what reaches the
    /// action thread, detection, and the owner.
    fn crew() -> (
        Crew,
        Receiver<act::Control>,
        Receiver<Input>,
        Receiver<Event>,
    ) {
        let (events, heard) = mpsc::channel();
        let (jobs, controls) = mpsc::channel();
        let (inputs, received) = mpsc::channel();
        let crew = Crew {
            system: Arc::new(HostSystem::default()),
            events,
            actor: Actor {
                jobs,
                thread: spawn("actions", || {}).expect("spawn"),
            },
            inputs,
            readers: BTreeMap::new(),
            tracing: Switch::new(env::temp_dir().join("appytappy-unused-traces")),
            trace: Trace::OFF,
        };
        (crew, controls, received, heard)
    }

    #[test]
    fn a_new_trace_reaches_every_thread_that_records() {
        let (mut crew, controls, received, _heard) = crew();
        let mut scratch = Scratch::new("new-trace");
        let (first, first_told) = Reader::stand_in();
        let (second, second_told) = Reader::stand_in();
        crew.readers.insert(SessionId(0), first);
        crew.readers.insert(SessionId(1), second);
        let (trace, _entries) = Trace::for_test(1, &[TraceCategory::Rules]);
        crew.set_trace(trace.clone(), &mut scratch.log);
        assert_eq!(crew.trace, trace, "the owner's own");
        assert!(
            matches!(controls.try_recv(), Ok(act::Control::Trace(sent)) if sent == trace),
            "the action thread's"
        );
        assert!(
            matches!(received.try_recv(), Ok(Input::Trace(sent)) if sent == trace),
            "detection's"
        );
        assert_eq!(
            first_told.traces(),
            std::slice::from_ref(&trace),
            "each reader's"
        );
        assert_eq!(second_told.traces(), [trace]);
    }

    #[test]
    fn a_reader_starts_with_the_owners_trace() {
        let (mut crew, _controls, _received, _heard) = crew();
        let (trace, _entries) = Trace::for_test(1, &[TraceCategory::Readings]);
        crew.trace = trace.clone();
        let plan = app_plan("Example App", &[]);
        let watched = Watched {
            process: ProcessId::new(7),
            executable: "Example.exe".to_owned(),
        };
        let interval = ReadInterval::DEFAULT;
        let (job, started) = crew.reader_job(
            SessionId(3),
            ReaderNumber(1),
            Arc::clone(&plan),
            watched.clone(),
            interval,
        );
        assert_eq!(started, trace);
        assert_eq!(
            job,
            reader::Job {
                session: SessionId(3),
                reader: ReaderNumber(1),
                plan,
                watched,
                interval,
            }
        );
    }

    #[test]
    fn a_reader_that_cannot_start_tells_the_owner_it_ended() {
        let (mut crew, _controls, _received, heard) = crew();
        let mut scratch = Scratch::new("reader-cannot-start");
        let no_thread = Err(io::Error::other("no more threads"));
        let (session, reader) = (SessionId(3), ReaderNumber(1));
        crew.read(session, reader, no_thread, "Example App", &mut scratch.log);
        assert!(
            matches!(
                heard.try_recv(),
                Ok(Event::Reading(Reading::Ended { session: of, reader: number }))
                    if of == session && number == reader
            ),
            "so the session starts its next reader after a wait"
        );
        assert!(crew.readers.is_empty());
        assert!(
            scratch
                .log()
                .trim_end()
                .ends_with("Example App: its reader cannot start: no more threads"),
            "{:?}",
            scratch.log()
        );
    }

    #[test]
    fn a_reader_that_started_is_kept_for_its_session_and_traced() {
        let (mut crew, _controls, _received, heard) = crew();
        let mut scratch = Scratch::new("reader-started");
        let (trace, entries) = Trace::for_test(1, &[TraceCategory::Lifecycle]);
        crew.trace = trace;
        let (reader, _told) = Reader::stand_in();
        let session = SessionId(3);
        crew.read(
            session,
            ReaderNumber(1),
            Ok(reader),
            "Example App",
            &mut scratch.log,
        );
        assert_eq!(
            crew.readers.keys().collect::<Vec<_>>(),
            [&session],
            "the owner stops it with its session"
        );
        let sent: Vec<(What, Option<SessionId>)> = crate::trace::sent(&entries)
            .into_iter()
            .map(|record| (record.what, record.session))
            .collect();
        assert_eq!(sent, [(What::Reading, Some(session))]);
        assert!(heard.try_recv().is_err(), "nothing ended");
        assert_eq!(scratch.log(), "", "and nothing is logged");
    }

    #[test]
    fn a_queued_job_reaches_the_action_thread_between_its_decision_and_queued() {
        let (mut crew, controls, _received, _heard) = crew();
        let mut scratch = Scratch::new("queued-job");
        let (trace, entries) = Trace::for_test(1, &[TraceCategory::Rules, TraceCategory::Actions]);
        crew.trace = trace;
        let cause = Instant::now();
        let owner = (SessionId(2), appytappy_engine::RuleIndex(4));
        let job = act::Job::Undo {
            owner,
            automation: "Mute".to_owned(),
            cause: Some(cause),
        };
        queue(&mut crew, What::Released, job.clone(), &mut scratch.log);
        assert!(
            matches!(controls.try_recv(), Ok(act::Control::Job(sent)) if sent == job),
            "the action thread gets the job"
        );
        let sent = crate::trace::sent(&entries);
        let what: Vec<What> = sent.iter().map(|record| record.what).collect();
        assert_eq!(what, [What::Released, What::Queued]);
        assert!(
            sent.iter()
                .all(|record| record.session == Some(SessionId(2))
                    && record.item == Some(4)
                    && record.cause == Some(cause)),
            "{sent:?}"
        );
    }

    #[test]
    fn an_action_thread_that_panicked_starts_again_and_takes_the_job() {
        let (mut crew, controls, _received, _heard) = crew();
        drop(controls);
        let ended = mem::replace(
            &mut crew.actor.thread,
            spawn("actions", || panic!("broke")).expect("spawn"),
        );
        join(ended).expect("the stand-in ends");
        let mut scratch = Scratch::new("action-thread-panicked");
        let (trace, entries) = Trace::for_test(1, &[TraceCategory::Actions]);
        crew.trace = trace;
        let owner = (SessionId(2), appytappy_engine::RuleIndex(4));
        let job = act::Job::Run {
            owner,
            automation: "Mute".to_owned(),
            watched: Watched {
                process: ProcessId::new(7),
                executable: "Example.exe".to_owned(),
            },
            actions: Box::new([]),
            clicks: Arc::default(),
            restore: appytappy_config::rule::Restore::OnRelease,
            cause: None,
        };
        queue(&mut crew, What::Fired, job, &mut scratch.log);
        let Crew { actor, .. } = crew;
        actor.stop().expect("the new thread runs the job and ends");
        assert!(
            scratch.log().trim_end().ends_with(
                "the actions thread panicked: broke, so it starts again; the jobs it had \
                 not run are dropped, and what it changed before is not undone"
            ),
            "{:?}",
            scratch.log()
        );
        let ran: Vec<(What, Option<SessionId>, Option<usize>)> = crate::trace::sent(&entries)
            .into_iter()
            .filter(|record| record.what != What::Queued)
            .map(|record| (record.what, record.session, record.item))
            .collect();
        assert_eq!(
            ran,
            [
                (What::Started, Some(SessionId(2)), Some(4)),
                (What::Done, Some(SessionId(2)), Some(4)),
            ],
            "the new thread records to the owner's trace and runs the job"
        );
    }

    #[test]
    fn a_send_that_detection_cannot_take_tells_the_owner_detection_ended() {
        let (crew, _controls, received, heard) = crew();
        drop(received);
        crew.detect(Input::Trace(Trace::OFF));
        assert!(matches!(heard.try_recv(), Ok(Event::DetectionEnded)));
    }

    #[test]
    fn a_saved_end_time_counts_from_the_wall_clock() {
        let mut scratch = Scratch::new("trace-until");
        let before = Instant::now();
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("after 1970")
            .as_millis();
        let until = now_ms.checked_add(60_000).expect("a minute on");
        scratch.write(
            "settings.json",
            &format!(r#"{{"format":1,"trace":{{"on":true,"until":{until}}}}}"#),
        );
        let mut model = Model::default();
        let mut reloads = Reloads::default();
        let on = scratch.read(&mut model, &mut reloads, SETTINGS_ONLY);
        assert!(on.trace.is_some_and(|trace| trace != Trace::OFF));
        let ends = scratch.tracing.until().expect("an end time");
        let soonest = before
            .checked_add(Duration::from_secs(30))
            .expect("in range");
        let latest = Instant::now()
            .checked_add(Duration::from_secs(60))
            .expect("in range");
        assert!(
            soonest < ends && ends <= latest,
            "the trace ends a minute from now"
        );
    }

    #[test]
    fn a_sample_is_handled_as_a_sample_of_its_frame() {
        let at = Instant::now();
        let sample = Event::from(Reading::Sample {
            session: SessionId(0),
            reader: ReaderNumber(0),
            position: 0,
            value: plan::examples::number(95.0),
            at,
        });
        assert_eq!((sample.kind(), sample.cause()), ("sample", Some(at)));
        let problem = Event::from(Reading::Problem {
            session: SessionId(0),
            problem: "cannot read".to_owned(),
        });
        assert_eq!((problem.kind(), problem.cause()), ("reader", None));
        assert_eq!((Event::Due.kind(), Event::Due.cause()), ("due", None));
    }

    #[test]
    fn a_received_value_is_traced_with_its_place_and_its_frame() {
        let (mut trace, entries) = Trace::for_test(1, &[TraceCategory::Changes]);
        let at = Instant::now();
        let value = plan::examples::number(95.0);
        receive(
            &mut trace,
            &Event::from(Reading::Sample {
                session: SessionId(0),
                reader: ReaderNumber(0),
                position: 2,
                value: value.clone(),
                at,
            }),
        );
        receive(&mut trace, &Event::Due);
        let sent = crate::trace::sent(&entries);
        let [received] = sent.as_slice() else {
            panic!("one record for the sample: {sent:?}");
        };
        assert_eq!(
            *received,
            Record {
                at: received.at,
                session: Some(SessionId(0)),
                item: Some(2),
                value: Some(value),
                took: Some(received.at.saturating_duration_since(at)),
                cause: Some(at),
                ..Record::now(What::Received)
            }
        );
    }

    #[test]
    fn a_rule_is_traced_with_its_automation_and_its_frame() {
        let (mut trace, entries) = Trace::for_test(1, &[TraceCategory::Rules]);
        let cause = Instant::now();
        record_rule(
            &mut trace,
            What::Held,
            (SessionId(2), appytappy_engine::RuleIndex(4)),
            Some(cause),
        );
        let sent = crate::trace::sent(&entries);
        let [held] = sent.as_slice() else {
            panic!("one record: {sent:?}");
        };
        assert_eq!(
            *held,
            Record {
                at: held.at,
                session: Some(SessionId(2)),
                item: Some(4),
                cause: Some(cause),
                ..Record::now(What::Held)
            }
        );
    }

    #[test]
    fn a_reload_that_cannot_read_the_files_keeps_the_plan_and_tries_again() {
        let mut scratch = Scratch::new("reload");
        let mut model = Model::default();
        model.replan(BTreeMap::from([(
            "a".to_owned(),
            app_plan("Example App", &["Mute"]),
        )]));
        model.apply(Change::Started {
            app: "a".to_owned(),
            process: ProcessId::new(7),
            executable: "Example.exe".to_owned(),
        });
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
        let watched = appytappy_actions::Watched {
            process: ProcessId::new(1),
            executable: "Example.exe".to_owned(),
        };
        let orders = [
            Order::Read {
                session: SessionId(1),
                reader: ReaderNumber(0),
                plan,
                watched: watched.clone(),
                interval: ReadInterval::DEFAULT,
            },
            Order::StopReading(SessionId(1), "Example App".to_owned()),
            Order::Run {
                owner,
                automation: "Mute".to_owned(),
                watched,
                actions: Box::new([]),
                clicks: Arc::default(),
                restore: appytappy_config::rule::Restore::OnRelease,
                cause: None,
            },
            Order::Undo {
                owner,
                automation: "Mute".to_owned(),
                cause: None,
            },
            Order::Held(owner, None),
        ];
        assert_eq!(
            orders.each_ref().map(order_line),
            [
                Some("Example App: reading its values".to_owned()),
                None,
                Some("Mute: runs".to_owned()),
                Some("Mute: undone".to_owned()),
                None,
            ]
        );
    }
}
