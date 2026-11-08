//! Reading one attached application's values from its window.
//!
//! Each running session has a reader thread. It finds the application's
//! window with the application's
//! [`appytappy_config::window_match::WindowMatch`], among the windows of
//! every running process whose executable fits it, and reads one of the
//! windows that fit best only when it belongs to the process the session
//! follows, picking among them with [`read_window`] as detection does
//! (see the Lifecycle section of `docs/developer/architecture.md`). Then it
//! captures the region of every value that reads at the window's current
//! size, which it asks the system for before the capture starts, so a window
//! at another size than the values were drawn at is not captured twice,
//! packed one under another by a [`Layout`], so the capture copies about
//! as many pixels as the values need. Windows pushes a
//! frame only when the window's content changes, so a still window is
//! copied no more, and the [`ReadSchedule`] backs off while the values hold
//! still. What a still window costs is the checks below: the reader lists
//! the processes and windows every [`CHECK_AGAIN`], and the capture checks
//! its graphics device on a timer while no frame comes.
//! A value is sent to the owner only when it reads differently from its last
//! read.
//!
//! When frames show the window at another size, the reader reads nothing
//! until the size settles: frames show it, and no other, for
//! [`SETTLE`](appytappy_sources::screen::settle::SETTLE), so dragging a
//! window's corner neither starts a capture again for each size it passes
//! nor redraws state pictures for each. The
//! reader then resolves where each value reads at the settled size once,
//! with `SavedValue::at`: the value's drawing at that size, or its first
//! drawing scaled to it while its application's window changes size often.
//! When some value reads there, the capture starts again for them, once; a
//! value that does not read there is not read, and the reader says why once
//! until the reason changes, as soon as the size settles, even when the
//! window draws no more frames.
//!
//! When the capture ends, the reader looks for the window again, because
//! many applications replace their window as they run, such as when they
//! switch to full screen. A capture ends when the window closes, when the
//! capture cannot go on, such as after the graphics device was lost, and
//! after [`RESTART_AFTER`] frames in a row fail. After a capture ends this
//! way, the reader waits before it captures again, from [`FIND_AGAIN`] up to
//! [`RESTART_WAIT_MAX`], doubling while each new capture ends before it reads
//! a frame, and logs why once until the reason changes. While capturing, the reader
//! checks every [`CHECK_AGAIN`] that the window still shows and still fits
//! the application best, because a hidden window sends no frames and no
//! close, and the application may open a better window, such as its main
//! window after a launcher. While no window fits, the window it would read
//! is minimized, or the window that fits best is another process's, it
//! looks again every [`FIND_AGAIN`]. Each time a capture
//! ends, the values it read can no longer be seen, so the reader sends
//! [`Reading::Unseen`], and the capture's handler sends nothing more.

use std::io;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use appytappy_config::anti_cheat::ProcessId;
use appytappy_config::settings_file::TraceCategory;
use appytappy_config::signal::{SignalName, Value};
use appytappy_sources::schedule::{Activity, ReadInterval, ReadSchedule};
use appytappy_sources::screen::saved::{AtSize, Unread};
use appytappy_sources::screen::settle::Settle;
use appytappy_sources::screen::{
    Capture, CaptureSession, Crop, Frame, Layout, OutsideWindow, Size,
};
use appytappy_sources::system::{System, best_windows, read_window, shows};

use crate::plan::AppPlan;
use crate::trace::{Record, Trace, What};

/// How often a reader looks for a window while none fits.
pub(crate) const FIND_AGAIN: Duration = Duration::from_secs(1);

/// How often a reader checks that the window it captures still shows and
/// still fits best.
pub(crate) const CHECK_AGAIN: Duration = Duration::from_secs(2);

/// How many frames in a row may fail before the capture starts again.
pub(crate) const RESTART_AFTER: u32 = 5;

/// The longest wait before a capture whose frames keep failing starts again.
pub(crate) const RESTART_WAIT_MAX: Duration = Duration::from_secs(30);

/// One session of an attached application. A new session starts each time
/// the runtime starts reading an app, so a reading from a session that ended
/// is told apart and dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SessionId(pub(crate) u64);

/// What a reader reads.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Job {
    /// The session the readings belong to.
    pub(crate) session: SessionId,
    /// The application's values and window.
    pub(crate) plan: Arc<AppPlan>,
    /// The process the session follows, whose window the reader reads.
    pub(crate) process: ProcessId,
    /// The attached process's executable, such as `Example.exe`, for the
    /// log.
    pub(crate) executable: String,
    /// How often to read the values, from the settings.
    pub(crate) interval: ReadInterval,
    /// The trace its frame handler records each reading to.
    pub(crate) trace: Trace,
}

/// What a reader reports.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Reading {
    /// A value read differently from its last read.
    Sample {
        /// The session.
        session: SessionId,
        /// The value's signal.
        signal: SignalName,
        /// What it read.
        value: Value,
        /// When the frame it was read from arrived.
        at: Instant,
    },
    /// Something keeps the reader from reading. Each problem is sent once
    /// until a different one, or a good read, replaces it.
    Problem {
        /// The session.
        session: SessionId,
        /// What is wrong, as a sentence fragment for the log.
        problem: String,
    },
    /// Something keeps one value from being read. The log puts the value's
    /// newest name before `problem`, so a renamed value shows its new name.
    /// Each is sent once until a different one, or a good read, replaces it.
    ValueProblem {
        /// The session.
        session: SessionId,
        /// The value's position among the application's values.
        value: usize,
        /// What is wrong, as a sentence fragment that follows the value's
        /// name.
        problem: String,
    },
    /// The values read so far can no longer be seen: the window closed, was
    /// minimized or hidden, took a size at which they were not drawn, or
    /// gave way to a window that fits better, no window fits any more, such
    /// as after a title change, or the capture lost its graphics device,
    /// could not follow the window to a new size, or its frames kept
    /// failing. Rules on them should forget them rather than hold their last
    /// readings.
    Unseen {
        /// The session.
        session: SessionId,
    },
}

/// Each value's last problem, in the order of its application's values.
type Problems = Vec<Option<String>>;

/// What reaches the reader thread.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Control {
    /// The session ended.
    Stop,
    /// The capture ended, such as when the window closed, and this says
    /// why, for the log.
    Lost(String),
    /// Frames show the window at `size`, which is not the size the
    /// capture reads at, or back at that size. The reader acts on it once
    /// it settles.
    Resizing {
        /// The size frames show.
        size: Size,
        /// Each value's last problem, handed to the reader when frames first
        /// leave the capture's size, so the reader says why values are not
        /// read at a new size, and a capture started for it does not say
        /// again what was said.
        problems: Option<Problems>,
    },
    /// [`RESTART_AFTER`] frames in a row failed, the last with this error,
    /// so the capture starts again after a wait.
    Failing(String),
    /// A capture started again for failing frames read a frame.
    Recovered,
}

/// How the reader recovers from captures that end because they fail.
#[derive(Debug)]
struct Recovery {
    /// Why the last capture ended, until a capture reads a frame.
    problem: Option<String>,
    /// The error that made the last capture's frames fail, which they
    /// logged already.
    frame_error: Option<String>,
    /// How long to wait before the next capture.
    wait: Duration,
}

impl Recovery {
    const fn new() -> Self {
        Self {
            problem: None,
            frame_error: None,
            wait: FIND_AGAIN,
        }
    }

    /// Notes that a capture ended for `problem`, after frames that failed
    /// with `frame_error`, if any. Returns the wait before the next capture,
    /// and whether `problem` is new, so it is logged.
    fn failed(&mut self, problem: String, frame_error: Option<String>) -> (Duration, bool) {
        let new = self.problem.as_ref() != Some(&problem);
        let wait = self.wait;
        self.wait = self.wait.saturating_mul(2).min(RESTART_WAIT_MAX);
        self.problem = Some(problem);
        self.frame_error = frame_error;
        (wait, new)
    }

    /// Notes that a capture read a frame again.
    fn recovered(&mut self) {
        *self = Self::new();
    }
}

/// A running reader thread.
#[derive(Debug)]
pub(crate) struct Reader {
    control: Sender<Control>,
    thread: JoinHandle<()>,
}

impl Reader {
    /// Starts reading `job` on a new thread, which sends what it reads to
    /// `events`.
    ///
    /// # Errors
    ///
    /// Returns the error from starting the thread.
    pub(crate) fn start<S, E>(system: Arc<S>, job: Job, events: Sender<E>) -> io::Result<Self>
    where
        S: System + Capture + Send + Sync + 'static,
        E: From<Reading> + Send + 'static,
    {
        Self::checking_every(CHECK_AGAIN, system, job, events)
    }

    /// Starts reading `job` like [`Reader::start`], checking the captured
    /// window every `check`.
    fn checking_every<S, E>(
        check: Duration,
        system: Arc<S>,
        job: Job,
        events: Sender<E>,
    ) -> io::Result<Self>
    where
        S: System + Capture + Send + Sync + 'static,
        E: From<Reading> + Send + 'static,
    {
        let (control, received) = mpsc::channel();
        let lost = control.clone();
        let thread = thread::Builder::new()
            .name(format!("reader {}", job.session.0))
            .spawn(move || read(system.as_ref(), &job, check, &received, &lost, &events))?;
        Ok(Self { control, thread })
    }

    /// Stops the reader and its capture, and waits for both.
    ///
    /// # Errors
    ///
    /// Returns an error when the reader thread panicked.
    pub(crate) fn stop(self) -> io::Result<()> {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a failed send means the thread already ended, and the join below says how"
        )]
        let _ = self.control.send(Control::Stop);
        super::join(self.thread)
    }
}

/// Sends `reading`, returning whether the owner still listens.
fn send<E: From<Reading>>(events: &Sender<E>, reading: Reading) -> bool {
    events.send(E::from(reading)).is_ok()
}

/// The reader thread: finds the window that fits best and shows, and
/// captures it until the capture ends, the window takes another size, is
/// hidden or gives way to a better one, or the session stops. Then it sends
/// [`Reading::Unseen`] and looks again, after a wait when the capture failed.
fn read<S: System + Capture, E: From<Reading> + Send + 'static>(
    system: &S,
    job: &Job,
    check: Duration,
    control: &Receiver<Control>,
    lost: &Sender<Control>,
    events: &Sender<E>,
) {
    // Where the values read at the window size the capture reads, until a
    // frame shows the window has another. A plan always has a value to
    // read, and it reads at the size it was drawn at.
    let Some(first) = job
        .plan
        .runs
        .values
        .first()
        .map(|value| value.check.drawn.window_size)
    else {
        return;
    };
    let mut placed = Placed::new(&job.plan, first);
    // Whether `placed` is at the size the last capture's frames settled at,
    // rather than a guess.
    let mut known_size = false;
    // Each value's last problem, carried to the capture started for a new
    // size.
    let mut carried: Problems = Vec::new();
    // The last problem sent, so each is sent once until it changes.
    let mut reported: Option<String> = None;
    let report = |reported: &mut Option<String>, problem: String| {
        if reported.as_ref() == Some(&problem) {
            return true;
        }
        *reported = Some(problem.clone());
        send(
            events,
            Reading::Problem {
                session: job.session,
                problem,
            },
        )
    };
    // Waits before looking again; false once the session stopped.
    let wait = || pause(control, FIND_AGAIN);
    let mut recovery = Recovery::new();
    loop {
        let window = match find_window(system, job) {
            Ok(Found::Shown(window)) => window,
            Ok(Found::Minimized) => {
                if !report(&mut reported, minimized()) || !wait() {
                    return;
                }
                continue;
            },
            Ok(Found::Nothing) => {
                if !report(&mut reported, no_window(job)) || !wait() {
                    return;
                }
                continue;
            },
            Ok(Found::Elsewhere) => {
                if !report(&mut reported, elsewhere()) || !wait() {
                    return;
                }
                continue;
            },
            Err(error) => {
                let problem = format!("its windows cannot be listed: {error}");
                if !report(&mut reported, problem) || !wait() {
                    return;
                }
                continue;
            },
        };
        if !known_size {
            placed = framed_at(system, job, window, placed);
        }
        // Gives the capture's handler back each value's last problem, as the
        // reader said it while frames showed another size.
        let (said, heard) = mpsc::channel();
        let Some(mut frames) =
            FrameReader::new(job, placed.clone(), events.clone(), lost.clone(), heard)
        else {
            return;
        };
        frames.carry(&recovery, std::mem::take(&mut carried));
        let crop = Crop::Layout(frames.layout.clone());
        let throttle = job.interval.min();
        let handler = Box::new(move |frame: io::Result<Frame<'_>>| frames.handle(frame));
        let session = match system.start(window, crop, throttle, handler) {
            Ok(session) => session,
            Err(error) => {
                let problem = format!("its window cannot be captured: {error}");
                if !report(&mut reported, problem) || !wait() {
                    return;
                }
                continue;
            },
        };
        reported = None;
        let next = watch(
            system,
            Watching {
                job,
                window,
                check,
                reads_at: placed.size,
                said: &said,
                events,
            },
            control,
            &mut recovery,
        );
        if let Err(error) = session.close() {
            report(&mut reported, format!("its capture ended badly: {error}"));
        }
        if stopped_meanwhile(control, &mut recovery) {
            return;
        }
        known_size = matches!(next, Ok(Ended::Resized(..)));
        match next {
            Err(problem) => {
                // The frame handler cannot see why, so it does not send
                // `Unseen` itself. It is sent after the capture closed, so
                // no reading from it can follow.
                let unseen = Reading::Unseen {
                    session: job.session,
                };
                if !report(&mut reported, problem) || !send(events, unseen) {
                    return;
                }
            },
            Ok(Ended::Resized(resized, problems)) => {
                placed = resized;
                carried = problems;
            },
            Ok(Ended::Lost(problem)) => {
                let (pause_for, new) = recovery.failed(problem.clone(), None);
                if (new && !report(&mut reported, problem)) || !pause(control, pause_for) {
                    return;
                }
            },
            Ok(Ended::Failing(error)) => {
                let problem = format!(
                    "{RESTART_AFTER} frames in a row cannot be read, so its capture starts \
                     again: {error}"
                );
                let (pause_for, new) = recovery.failed(problem.clone(), Some(error));
                if (new && !report(&mut reported, problem)) || !pause(control, pause_for) {
                    return;
                }
            },
            Ok(Ended::Stop) => return,
        }
    }
}

/// Empties `control` of what a closed capture sent after the control that
/// ended it, which is moot now, and says whether a stop came meanwhile. A
/// capture that read a frame meanwhile still counts as recovered.
fn stopped_meanwhile(control: &Receiver<Control>, recovery: &mut Recovery) -> bool {
    let mut stopped = false;
    for left in control.try_iter() {
        match left {
            Control::Stop => stopped = true,
            Control::Recovered => recovery.recovered(),
            Control::Lost(_) | Control::Resizing { .. } | Control::Failing(_) => {},
        }
    }
    stopped
}

/// Waits for `time` while the session runs; false once it stopped. Any
/// other control is moot while no capture runs.
fn pause(control: &Receiver<Control>, time: Duration) -> bool {
    let until = Instant::now().checked_add(time);
    loop {
        let left = until.map_or(time, |until| {
            until.saturating_duration_since(Instant::now())
        });
        match control.recv_timeout(left) {
            Ok(Control::Stop) | Err(RecvTimeoutError::Disconnected) => return false,
            Err(RecvTimeoutError::Timeout) => return true,
            Ok(_moot) => {},
        }
    }
}

/// What ended a capture, as [`watch`] reports it.
#[derive(Debug, PartialEq, Eq)]
enum Ended {
    /// The session ended.
    Stop,
    /// See [`Control::Lost`].
    Lost(String),
    /// The window settled at a size where some values read, placed here,
    /// with each value's last problem, so the capture starts again for
    /// them.
    Resized(Placed, Problems),
    /// See [`Control::Failing`].
    Failing(String),
}

/// Says why each value is not read where `placed` places it, once until
/// its reason in `problems` changes.
fn say_unread<E: From<Reading>>(
    placed: &Placed,
    problems: &mut [Option<String>],
    session: SessionId,
    events: &Sender<E>,
) {
    for (value, (at, last)) in placed.values.iter().zip(problems).enumerate() {
        if let Err(unread) = at
            && let Some(problem) = newly(last, not_read(*unread))
        {
            send(
                events,
                Reading::ValueProblem {
                    session,
                    value,
                    problem,
                },
            );
        }
    }
}

/// `problem` when it is not `last`, which it then becomes.
fn newly(last: &mut Option<String>, problem: String) -> Option<String> {
    (last.as_ref() != Some(&problem)).then(|| last.insert(problem).clone())
}

/// What [`watch`] watches.
struct Watching<'job, E> {
    job: &'job Job,
    /// The captured window, checked every `check`.
    window: u64,
    check: Duration,
    /// The window size the capture reads at.
    reads_at: Size,
    /// Where it gives the capture's handler back each value's last problem.
    said: &'job Sender<Problems>,
    /// Where it says why values are not read at a settled size.
    events: &'job Sender<E>,
}

/// Waits while a window is captured, checking it every `check`, and
/// returns what ended the capture, or why the reader should look for the
/// window again. A size that frames show is acted on once it settles: when
/// no value reads there, it says why each does not, once until the reason
/// changes, even when the window draws no more frames.
fn watch<S: System, E: From<Reading>>(
    system: &S,
    watching: Watching<'_, E>,
    control: &Receiver<Control>,
    recovery: &mut Recovery,
) -> Result<Ended, String> {
    let Watching {
        job,
        window,
        check,
        reads_at,
        said,
        events,
    } = watching;
    let mut settle = Settle::new(reads_at);
    // Each value's last problem, as the handler handed it over when frames
    // left the capture's size.
    let mut problems = vec![None; job.plan.runs.values.len()];
    let mut next_check = Instant::now().checked_add(check);
    loop {
        let now = Instant::now();
        let to_check = next_check.map_or(check, |next| next.saturating_duration_since(now));
        let wait = settle.left(now).map_or(to_check, |left| left.min(to_check));
        match control.recv_timeout(wait) {
            Ok(Control::Recovered) => recovery.recovered(),
            Ok(Control::Stop) | Err(RecvTimeoutError::Disconnected) => return Ok(Ended::Stop),
            Ok(Control::Lost(problem)) => return Ok(Ended::Lost(problem)),
            Ok(Control::Resizing {
                size,
                problems: handed,
            }) => {
                settle.saw(size, Instant::now());
                if let Some(handed) = handed {
                    problems = handed;
                }
            },
            Ok(Control::Failing(error)) => return Ok(Ended::Failing(error)),
            Err(RecvTimeoutError::Timeout) => {
                let now = Instant::now();
                if let Some(other) = settled_elsewhere(&mut settle, now, &job.plan, reads_at) {
                    if other.reads() {
                        return Ok(Ended::Resized(other, problems));
                    }
                    say_unread(&other, &mut problems, job.session, events);
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "a failed send means the capture already ended"
                    )]
                    let _ = said.send(problems.clone());
                }
                if next_check.is_none_or(|next| now >= next) {
                    next_check = now.checked_add(check);
                    if let Some(problem) = moved_on(system, job, window) {
                        return Err(problem);
                    }
                }
            },
        }
    }
}

/// Where the values of `plan` read at the size `settle` settled at by
/// `now`, once it has, unless that is `reads_at`, where the capture already
/// reads them. Each settled size is resolved once.
fn settled_elsewhere(
    settle: &mut Settle,
    now: Instant,
    plan: &AppPlan,
    reads_at: Size,
) -> Option<Placed> {
    let size = settle.settled(now)?;
    (size != reads_at).then(|| Placed::new(plan, size))
}

/// Where the values of `job` read at the size the system frames `window`
/// at now, when it says and some value reads there, so the capture starts
/// at the size its first frame has and is not started again for it.
/// Otherwise `placed`, and the capture's frames tell the size.
fn framed_at<S: System>(system: &S, job: &Job, window: u64, placed: Placed) -> Placed {
    match system.frame_size(window) {
        Ok(Some(size)) if size != placed.size => {
            let there = Placed::new(&job.plan, size);
            if there.reads() { there } else { placed }
        },
        _ => placed,
    }
}

/// What the reader found to capture.
#[derive(Debug, PartialEq, Eq)]
enum Found {
    /// The window to read, which shows.
    Shown(u64),
    /// The window to read is minimized.
    Minimized,
    /// The windows that fit best are another process's, which the session
    /// does not follow, at least not yet.
    Elsewhere,
    /// No window fits.
    Nothing,
}

/// The window to read: the one [`read_window`] picks among the windows that
/// fit the app best for the session's process. Detection follows the
/// process that owns the window it picks, so the reader reads it only when
/// it is the session's process's.
fn find_window<S: System>(system: &S, job: &Job) -> io::Result<Found> {
    let processes = system.processes()?;
    let best = best_windows(&job.plan.runs.window, processes, system.windows()?);
    let shown = |handle| shows(system.window_place(handle));
    Ok(match read_window(&best, Some(job.process), shown) {
        Some((window, _)) if window.process != job.process => Found::Elsewhere,
        Some((window, true)) => Found::Shown(window.handle),
        Some((_, false)) => Found::Minimized,
        None => Found::Nothing,
    })
}

fn elsewhere() -> String {
    "the window that fits best belongs to another process, so it is read once the session \
     follows that process"
        .to_owned()
}

fn minimized() -> String {
    "its window is minimized or hidden".to_owned()
}

fn no_window(job: &Job) -> String {
    format!("no window of {} fits yet", job.executable)
}

/// Why the reader should stop capturing `window`, a window of the session's
/// process, and look again, or `None` while it still shows and fits the app
/// as well as any window. A check that fails keeps the capture.
fn moved_on<S: System>(system: &S, job: &Job, window: u64) -> Option<String> {
    if !shows(system.window_place(window)) {
        return Some(minimized());
    }
    let processes = system.processes().ok()?;
    let best = best_windows(&job.plan.runs.window, processes, system.windows().ok()?);
    if best.is_empty() {
        return Some(no_window(job));
    }
    // Any window tied for the best fit is kept, so tied windows do not swap
    // back and forth. The reader captures only the session's process's
    // windows, so this one is.
    if best.iter().any(|entry| entry.handle == window) {
        return None;
    }
    Some(if best.iter().any(|entry| entry.process == job.process) {
        "another window fits better, so it is read instead".to_owned()
    } else {
        elsewhere()
    })
}

/// Where each of an application's values reads at one window size, or why it
/// does not, in the order of its values. It is resolved once for each size
/// the window settles at, since a scaled state check redraws its pictures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Placed {
    /// The window size.
    size: Size,
    /// Each value's place at it.
    values: Vec<Result<AtSize, Unread>>,
}

impl Placed {
    /// The values of `plan` placed at `size`.
    fn new(plan: &AppPlan, size: Size) -> Self {
        Self {
            size,
            values: plan
                .runs
                .values
                .iter()
                .map(|value| value.check.at(size))
                .collect(),
        }
    }

    /// Whether any value reads at its size.
    fn reads(&self) -> bool {
        self.values.iter().any(Result::is_ok)
    }
}

/// Why a value is not read at the window's size, as a sentence fragment
/// that follows the value's name. It leaves the size out, so the log says
/// it once for each reason, not once for each size a window passes.
fn not_read(unread: Unread) -> String {
    let advice = match unread {
        Unread::NotDrawn => "resize the window, or fix the value at this size in AppyTappy Setup",
        Unread::Outside | Unread::TooSmall => "fix the value at this size in AppyTappy Setup",
    };
    format!("is not read at the window's size now: {unread}; {advice}")
}

/// The capture's frame handler: reads each value that reads at one window
/// size from a frame and sends what changed.
struct FrameReader<E> {
    session: SessionId,
    plan: Arc<AppPlan>,
    /// Where each value reads at the window size it reads.
    placed: Placed,
    /// The other size it last told the reader frames show, until frames
    /// show the size it reads at again.
    told: Option<Size>,
    /// What the capture copies: each value that reads at `placed`'s size,
    /// packed, and where each value then sits in the frame's image.
    layout: Layout,
    /// Each value's last reading.
    last: Vec<Option<Value>>,
    /// Each value's last problem. It is handed to the reader when frames
    /// leave the size it reads at, since the reader says why values are not
    /// read at another size, and taken back when they show it again.
    problems: Problems,
    /// Where the reader gives `problems` back.
    heard: Receiver<Problems>,
    /// The last problem with a frame as a whole.
    frame_problem: Option<String>,
    /// How many frames in a row failed.
    failures: u32,
    /// Whether the reader started this capture again for failing frames,
    /// and waits to hear that it read one.
    restarted: bool,
    /// Whether it sent a reading since it last sent [`Reading::Unseen`].
    seen: bool,
    /// Whether it told the reader that its capture ended. The capture can
    /// still hand it frames until the reader closes it, and it reads none
    /// of them, so nothing it sends follows the reader's `Unseen`.
    ended: bool,
    schedule: ReadSchedule,
    /// Where it records each reading and change, when the trace is on.
    trace: Trace,
    events: Sender<E>,
    control: Sender<Control>,
}

impl<E: From<Reading>> FrameReader<E> {
    /// The handler for the values of `job` as `placed` places them, or
    /// `None` when none reads. `heard` gives back each value's last problem
    /// as the reader said it while frames showed another size.
    fn new(
        job: &Job,
        placed: Placed,
        events: Sender<E>,
        control: Sender<Control>,
        heard: Receiver<Problems>,
    ) -> Option<Self> {
        let layout = Layout::new(
            placed
                .values
                .iter()
                .map(|at| at.as_ref().ok().map(|at| at.region)),
        )?;
        let count = job.plan.runs.values.len();
        Some(Self {
            session: job.session,
            plan: Arc::clone(&job.plan),
            placed,
            told: None,
            layout,
            last: vec![None; count],
            problems: vec![None; count],
            heard,
            frame_problem: None,
            failures: 0,
            restarted: false,
            seen: false,
            ended: false,
            schedule: ReadSchedule::new(job.interval),
            trace: job.trace.clone(),
            events,
            control,
        })
    }

    /// Reads `frame` and returns how long to wait before the next one.
    fn handle(&mut self, frame: io::Result<Frame<'_>>) -> Duration {
        if self.ended {
            return FIND_AGAIN;
        }
        let (window_size, frame) = match frame {
            Ok(frame) => (frame.window_size, Some(frame)),
            Err(error) if error.kind() == io::ErrorKind::NotConnected => {
                self.unseen();
                self.tell(Control::Lost(format!("its capture ended: {error}")));
                return Duration::ZERO;
            },
            Err(error) => match OutsideWindow::in_error(&error) {
                Some(window_size) if window_size != self.placed.size => (window_size, None),
                // The region lies outside a window of this reader's own size
                // only when the saved region does not fit the size it was
                // drawn at. A new capture would fail the same way.
                Some(_) => return self.frame_failed(&error),
                None => {
                    let wait = self.frame_failed(&error);
                    self.failures = self.failures.saturating_add(1);
                    if self.failures < RESTART_AFTER {
                        return wait;
                    }
                    self.failures = 0;
                    self.unseen();
                    self.tell(Control::Failing(error.to_string()));
                    return Duration::ZERO;
                },
            },
        };
        self.frame_problem = None;
        self.failures = 0;
        if self.restarted {
            self.restarted = false;
            self.tell(Control::Recovered);
        }
        if window_size != self.placed.size {
            // Nothing is read at another size. The reader acts on it once it
            // settled, and says why values are not read there.
            self.unseen();
            if self.told != Some(window_size) {
                // Leaving its size, it hands the reader its problems.
                let problems = self.told.is_none().then(|| {
                    self.heard.try_iter().for_each(drop);
                    self.problems.clone()
                });
                self.told = Some(window_size);
                self.tell(Control::Resizing {
                    size: window_size,
                    problems,
                });
            }
            return Duration::ZERO;
        }
        if self.told.take().is_some() {
            if let Some(problems) = self.heard.try_iter().last()
                && problems.len() == self.problems.len()
            {
                self.problems = problems;
            }
            self.tell(Control::Resizing {
                size: window_size,
                problems: None,
            });
        }
        let mut changed = false;
        // The clock is read only when each reading is traced.
        let timed = self.trace.wants(TraceCategory::Readings);
        let session = self.session;
        for position in 0..self.plan.runs.values.len() {
            let started = timed.then(Instant::now);
            let read = self.read(position, frame.as_ref());
            if let (Some(started), Some(Ok(value)), Some(frame)) = (started, &read, &frame) {
                let took = started.elapsed();
                self.trace.record(What::Read, |record| Record {
                    session: Some(session),
                    item: Some(position),
                    value: Some(value.clone()),
                    took: Some(took),
                    cause: Some(frame.received),
                    ..record
                });
            }
            match read {
                Some(Ok(reading)) => changed |= self.record(position, reading, frame.as_ref()),
                Some(Err(problem)) => self.report(position, problem),
                None => {},
            }
        }
        self.schedule.after_read(if changed {
            Activity::Changed
        } else {
            Activity::Steady
        })
    }

    /// Notes whether the reader started this capture again after the last
    /// one failed, the frame error it logged already, and each value's last
    /// problem, which a capture started for a new size carries.
    fn carry(&mut self, recovery: &Recovery, problems: Problems) {
        self.restarted = recovery.problem.is_some();
        self.frame_problem = recovery.frame_error.as_deref().map(frame_problem);
        if problems.len() == self.problems.len() {
            self.problems = problems;
        }
    }

    /// Says once that frames cannot be read, until another problem or a
    /// good frame replaces it, and returns the wait before the next frame.
    fn frame_failed(&mut self, error: &io::Error) -> Duration {
        let problem = frame_problem(&error.to_string());
        if self.frame_problem.as_ref() != Some(&problem) {
            self.frame_problem = Some(problem.clone());
            self.problem(problem);
        }
        self.schedule.after_read(Activity::Steady)
    }

    /// Reads the value at `position`, or says why it cannot be read.
    fn read(&self, position: usize, frame: Option<&Frame<'_>>) -> Option<Result<Value, String>> {
        let value = self.plan.runs.values.get(position)?;
        let at = match self.placed.values.get(position)? {
            Ok(at) => at,
            Err(unread) => return Some(Err(not_read(*unread))),
        };
        let region = self.layout.packed(position)?;
        let image = frame?.image.crop(region)?;
        Some(
            value
                .check
                .check(at)
                .read(&image)
                .map_err(|error| format!("cannot be read: {error}")),
        )
    }

    /// Keeps a good reading and sends it when it changed; returns whether it
    /// did.
    fn record(&mut self, position: usize, reading: Value, frame: Option<&Frame<'_>>) -> bool {
        if let Some(slot) = self.problems.get_mut(position) {
            *slot = None;
        }
        let (Some(last), Some(value), Some(frame)) = (
            self.last.get_mut(position),
            self.plan.runs.values.get(position),
            frame,
        ) else {
            return false;
        };
        if last.as_ref() == Some(&reading) {
            return false;
        }
        *last = Some(reading.clone());
        self.seen = true;
        let session = self.session;
        self.trace.record(What::Changed, |record| Record {
            session: Some(session),
            item: Some(position),
            value: Some(reading.clone()),
            cause: Some(frame.received),
            ..record
        });
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a failed send means the owner stopped this session"
        )]
        let _ = self.events.send(E::from(Reading::Sample {
            session: self.session,
            signal: value.signal.clone(),
            value: reading,
            at: frame.received,
        }));
        true
    }

    /// Sends a value's problem once until it changes.
    fn report(&mut self, position: usize, problem: String) {
        let Some(slot) = self.problems.get_mut(position) else {
            return;
        };
        if let Some(problem) = newly(slot, problem) {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a failed send means the owner stopped this session"
            )]
            let _ = self.events.send(E::from(Reading::ValueProblem {
                session: self.session,
                value: position,
                problem,
            }));
        }
    }

    /// Says once that the values it read can no longer be seen.
    /// Every value then reads as new when it is seen again.
    fn unseen(&mut self) {
        if self.seen {
            self.seen = false;
            self.last.fill(None);
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a failed send means the owner stopped this session"
            )]
            let _ = self.events.send(E::from(Reading::Unseen {
                session: self.session,
            }));
        }
    }

    /// Tells the reader `control`; once that ends the capture, it reads no
    /// more frames.
    fn tell(&mut self, control: Control) {
        if matches!(control, Control::Lost(_) | Control::Failing(_)) {
            self.ended = true;
        }
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a failed send means the reader already stopped"
        )]
        let _ = self.control.send(control);
    }

    fn problem(&self, problem: String) {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a failed send means the owner stopped this session"
        )]
        let _ = self.events.send(E::from(Reading::Problem {
            session: self.session,
            problem,
        }));
    }
}

fn frame_problem(error: &str) -> String {
    format!("a frame cannot be read: {error}")
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{SyncSender, TryRecvError};

    use appytappy_config::anti_cheat::ProcessId;
    use appytappy_config::app_file::{PinDef, SizeChangesDef};
    use appytappy_config::rule::Rule;
    use appytappy_config::window_match::{MustMatch, Pattern, WindowMatch};
    use appytappy_sources::screen::check::{Check, ColorMatch};
    use appytappy_sources::screen::digits::DigitReader;
    use appytappy_sources::screen::saved::{SavedCheck, SavedFix, SavedValue};
    use appytappy_sources::screen::settle::SETTLE;
    use appytappy_sources::screen::{FrameHandler, Image, Placement, Region};
    use appytappy_sources::system::{Bounds, ProcessEntry, Rgb, WindowEntry, WindowPlace};

    use super::*;
    use crate::plan::{Labels, Runs, ValuePlan};

    const RED: Rgb = Rgb {
        red: 255,
        green: 0,
        blue: 0,
    };
    const WINDOW: Size = Size {
        width: 800,
        height: 600,
    };
    const SMALL: Size = Size {
        width: 640,
        height: 480,
    };
    const PATIENCE: Duration = Duration::from_secs(3);
    /// Shorter than [`FIND_AGAIN`], so nothing should happen in it.
    const QUIET: Duration = Duration::from_millis(300);

    /// A 2x2 value that is true while its region is red.
    fn red_at(name: &str, x: u32, y: u32) -> ValuePlan {
        drawn_at(name, x, y, WINDOW)
    }

    /// A 2x2 value drawn on a window of `window_size`, read only at that
    /// size.
    fn drawn_at(name: &str, x: u32, y: u32, window_size: Size) -> ValuePlan {
        scaling(name, x, y, window_size, SizeChangesDef::Rarely)
    }

    /// A 2x2 value drawn on a window of `window_size`, pinned to its top
    /// left, read as `size_changes` say.
    fn scaling(
        name: &str,
        x: u32,
        y: u32,
        window_size: Size,
        size_changes: SizeChangesDef,
    ) -> ValuePlan {
        ValuePlan {
            signal: SignalName::new(name).expect("valid"),
            check: SavedValue {
                drawn: SavedCheck {
                    window_size,
                    region: Region::new(x, y, 2, 2).expect("not empty"),
                    check: Check::Color(ColorMatch {
                        color: RED,
                        tolerance: 10,
                    }),
                },
                fixes: Vec::new(),
                pin: PinDef::TopLeft,
                size_changes,
            },
        }
    }

    /// The frame handler for `job`'s values placed at `size`.
    fn frames_at(
        job: &Job,
        size: Size,
        events: Sender<Reading>,
        control: Sender<Control>,
    ) -> Option<FrameReader<Reading>> {
        frames_with(job, size, events, control).map(|(frames, _)| frames)
    }

    /// The frame handler for `job`'s values placed at `size`, and where the
    /// reader gives it back each value's last problem.
    fn frames_with(
        job: &Job,
        size: Size,
        events: Sender<Reading>,
        control: Sender<Control>,
    ) -> Option<(FrameReader<Reading>, Sender<Problems>)> {
        let (said, heard) = mpsc::channel();
        let frames = FrameReader::new(job, Placed::new(&job.plan, size), events, control, heard)?;
        Some((frames, said))
    }

    fn plan(values: Vec<ValuePlan>) -> Arc<AppPlan> {
        Arc::new(AppPlan {
            labels: Labels {
                name: "Example App".to_owned(),
                automations: Vec::new(),
                values: values
                    .iter()
                    .map(|value| value.signal.to_string())
                    .collect(),
                checked: None,
            },
            runs: Runs {
                window: WindowMatch {
                    title: Pattern::new("Main*"),
                    class: Pattern::new("*"),
                    executable: Pattern::new("Example.exe"),
                    must_match: MustMatch {
                        title: true,
                        class: true,
                    },
                    fallback: false,
                },
                values,
                rules: Arc::<[Rule]>::from(Vec::new()),
            },
        })
    }

    fn job(values: Vec<ValuePlan>) -> Job {
        Job {
            session: SessionId(3),
            plan: plan(values),
            process: ProcessId::new(43),
            executable: "Example.exe".to_owned(),
            interval: ReadInterval::DEFAULT,
            trace: Trace::OFF,
        }
    }

    /// A `width` by `height` frame, red where `red` says and black elsewhere.
    fn frame_image(width: u32, height: u32, red: impl Fn(u32, u32) -> bool) -> Image {
        let mut bgra = Vec::new();
        for y in 0..height {
            for x in 0..width {
                bgra.extend(if red(x, y) {
                    [0, 0, 255, 255]
                } else {
                    [0, 0, 0, 255]
                });
            }
        }
        Image::new(Size { width, height }, bgra).expect("matching length")
    }

    fn frame(image: &Image, window_size: Size) -> Frame<'_> {
        Frame {
            window_size,
            image,
            received: Instant::now(),
            delay: Duration::ZERO,
        }
    }

    fn outside(window_size: Size) -> io::Result<Frame<'static>> {
        Err(OutsideWindow { window_size }.into())
    }

    fn flags(readings: &Receiver<Reading>) -> Vec<(String, Value)> {
        readings
            .try_iter()
            .filter_map(|reading| match reading {
                Reading::Sample { signal, value, .. } => Some((signal.to_string(), value)),
                Reading::Problem { .. } | Reading::ValueProblem { .. } | Reading::Unseen { .. } => {
                    None
                },
            })
            .collect()
    }

    fn problems(readings: &Receiver<Reading>) -> Vec<String> {
        readings
            .try_iter()
            .filter_map(|reading| match reading {
                Reading::Problem { problem, .. } => Some(problem),
                Reading::ValueProblem { value, problem, .. } => {
                    Some(format!("value {value} {problem}"))
                },
                Reading::Sample { .. } | Reading::Unseen { .. } => None,
            })
            .collect()
    }

    #[test]
    fn the_capture_packs_every_value_and_reads_each_from_its_place() {
        let (events, readings) = mpsc::channel::<Reading>();
        let (control, _control) = mpsc::channel();
        let job = job(vec![red_at("left", 10, 20), red_at("right", 16, 22)]);
        let mut frames = frames_at(&job, WINDOW, events, control).expect("values to read");
        let region = |x, y| Region::new(x, y, 2, 2).expect("not empty");
        assert_eq!(
            frames.layout.copies(),
            [
                Placement {
                    from: region(10, 20),
                    y: 0
                },
                Placement {
                    from: region(16, 22),
                    y: 2
                },
            ],
            "the two squares are stacked, not the 8x4 box around them"
        );
        // Only the left value's square, at the top of the packed image, is red.
        let left_red = frame_image(2, 4, |_, y| y < 2);
        let wait = frames.handle(Ok(frame(&left_red, WINDOW)));
        assert_eq!(
            wait,
            ReadInterval::DEFAULT.min(),
            "a change reads again soon"
        );
        assert_eq!(
            flags(&readings),
            [
                ("left".to_owned(), Value::Flag(true)),
                ("right".to_owned(), Value::Flag(false))
            ]
        );
        let steady = frames.handle(Ok(frame(&left_red, WINDOW)));
        assert!(steady > wait, "a steady read backs off");
        assert_eq!(flags(&readings), [], "nothing changed, so nothing is sent");
        let right_red = frame_image(2, 4, |_, y| y >= 2);
        frames.handle(Ok(frame(&right_red, WINDOW)));
        assert_eq!(
            flags(&readings),
            [
                ("left".to_owned(), Value::Flag(false)),
                ("right".to_owned(), Value::Flag(true))
            ]
        );
    }

    #[test]
    fn each_read_and_each_change_is_traced_with_its_value_and_its_frame() {
        let (events, _readings) = mpsc::channel::<Reading>();
        let (control, _control) = mpsc::channel();
        let (trace, entries) =
            Trace::for_test(1, &[TraceCategory::Readings, TraceCategory::Changes]);
        let job = Job {
            trace,
            ..job(vec![red_at("left", 10, 20), red_at("right", 16, 22)])
        };
        let mut frames = frames_at(&job, WINDOW, events, control).expect("values to read");
        let left_red = frame_image(8, 4, |x, y| x < 2 && y < 2);
        let arrived = frame(&left_red, WINDOW);
        let received = arrived.received;
        frames.handle(Ok(arrived));
        let rows: Vec<_> = crate::trace::sent(&entries)
            .into_iter()
            .map(|record| {
                (
                    record.what,
                    record.session,
                    record.item,
                    record.value,
                    record.took.is_some(),
                    record.cause,
                )
            })
            .collect();
        let cause = Some(received);
        let session = Some(SessionId(3));
        let (on, off) = (Some(Value::Flag(true)), Some(Value::Flag(false)));
        assert_eq!(
            rows,
            [
                (What::Read, session, Some(0), on.clone(), true, cause),
                (What::Changed, session, Some(0), on, false, cause),
                (What::Read, session, Some(1), off.clone(), true, cause),
                (What::Changed, session, Some(1), off, false, cause),
            ],
            "a read says how long it took; a change does not"
        );
    }

    /// Why a value drawn only at other sizes is not read, as it is sent.
    const NOT_DRAWN: &str = "is not read at the window's size now: it is read only at the window \
                             sizes it was drawn at; resize the window, or fix the value at this \
                             size in AppyTappy Setup";

    /// Why a value that scales past the window is not read, as it is sent.
    const OUTSIDE: &str = "is not read at the window's size now: scaled to this window size, its \
                           region reaches outside the window; fix the value at this size in \
                           AppyTappy Setup";

    #[test]
    fn a_window_of_another_size_is_read_only_once_it_settled_and_says_why_once() {
        let (events, readings) = mpsc::channel::<Reading>();
        let (control, told) = mpsc::channel();
        let job = job(vec![red_at("icon", 0, 0)]);
        let (mut frames, said) =
            frames_with(&job, WINDOW, events.clone(), control).expect("values to read");
        let image = frame_image(2, 2, |_, _| true);
        frames.handle(Ok(frame(&image, WINDOW)));
        assert_eq!(flags(&readings), [("icon".to_owned(), Value::Flag(true))]);
        assert_eq!(frames.handle(Ok(frame(&image, SMALL))), Duration::ZERO);
        frames.handle(outside(SMALL));
        frames.handle(Ok(frame(&image, SMALL)));
        assert_eq!(
            told.try_iter().collect::<Vec<_>>(),
            [Control::Resizing {
                size: SMALL,
                problems: Some(vec![None])
            }],
            "the reader is told once, and gets the problems"
        );
        assert_eq!(
            readings.try_iter().collect::<Vec<_>>(),
            [Reading::Unseen {
                session: SessionId(3)
            }],
            "nothing is read before the size settles"
        );
        // The reader found that nothing reads at the settled size and says
        // why, once, with no frame needed.
        let small = Placed::new(&job.plan, SMALL);
        let mut problems_now = vec![None];
        say_unread(&small, &mut problems_now, job.session, &events);
        say_unread(&small, &mut problems_now, job.session, &events);
        assert_eq!(problems(&readings), [format!("value 0 {NOT_DRAWN}")]);
        said.send(problems_now).expect("the handler listens");
        frames.handle(outside(SMALL));
        frames.handle(Ok(frame(&image, SMALL)));
        assert_eq!(problems(&readings), Vec::<String>::new());
        assert_eq!(told.try_recv(), Err(TryRecvError::Empty));
        frames.handle(Ok(frame(&image, WINDOW)));
        assert_eq!(
            told.try_iter().collect::<Vec<_>>(),
            [Control::Resizing {
                size: WINDOW,
                problems: None
            }],
            "back at its own size, the reader is told"
        );
        assert_eq!(
            flags(&readings),
            [("icon".to_owned(), Value::Flag(true))],
            "seen again, the same value is sent again"
        );
        frames.handle(outside(WINDOW));
        assert_eq!(
            problems(&readings),
            ["a frame cannot be read: the region lies outside the 800x600 window"]
        );
        frames.handle(Err(io::Error::other("the copy failed")));
        frames.handle(Err(io::Error::other("the copy failed")));
        assert_eq!(
            problems(&readings),
            ["a frame cannot be read: the copy failed"]
        );
    }

    #[test]
    fn the_handler_hands_its_problems_to_the_reader_and_takes_them_back() {
        let (events, readings) = mpsc::channel::<Reading>();
        let (control, told) = mpsc::channel();
        let job = job(vec![red_at("large", 0, 0), drawn_at("small", 4, 6, SMALL)]);
        let (mut frames, said) = frames_with(&job, WINDOW, events, control).expect("values");
        let red = frame_image(2, 2, |_, _| true);
        frames.handle(Ok(frame(&red, WINDOW)));
        assert_eq!(problems(&readings), [format!("value 1 {NOT_DRAWN}")]);
        // A give-back left from an earlier size is dropped when it leaves.
        said.send(vec![None, None]).expect("the handler listens");
        frames.handle(Ok(frame(&red, SMALL)));
        let other = Size {
            width: 320,
            height: 240,
        };
        frames.handle(Ok(frame(&red, other)));
        assert_eq!(
            told.try_iter().collect::<Vec<_>>(),
            [
                Control::Resizing {
                    size: SMALL,
                    problems: Some(vec![None, Some(NOT_DRAWN.to_owned())])
                },
                Control::Resizing {
                    size: other,
                    problems: None
                }
            ],
            "handed once"
        );
        // The reader said something else about the value meanwhile, so back
        // at its size the value says its own reason again.
        said.send(vec![None, Some(OUTSIDE.to_owned())])
            .expect("the handler listens");
        frames.handle(Ok(frame(&red, WINDOW)));
        assert_eq!(problems(&readings), [format!("value 1 {NOT_DRAWN}")]);
        assert_eq!(frames.problems, [None, Some(NOT_DRAWN.to_owned())]);
    }

    #[test]
    fn a_size_settles_once_and_is_resolved_once_unless_the_capture_reads_at_it() {
        let job = job(vec![red_at("large", 0, 0), drawn_at("small", 4, 6, SMALL)]);
        let start = Instant::now();
        let later = start.checked_add(SETTLE).expect("in range");
        let mut settle = Settle::new(WINDOW);
        settle.saw(SMALL, start);
        assert_eq!(
            settled_elsewhere(&mut settle, start, &job.plan, WINDOW),
            None,
            "not settled yet"
        );
        assert_eq!(
            settled_elsewhere(&mut settle, later, &job.plan, WINDOW),
            Some(Placed::new(&job.plan, SMALL))
        );
        assert_eq!(
            settled_elsewhere(&mut settle, later, &job.plan, WINDOW),
            None,
            "resolved once"
        );
        settle.saw(WINDOW, later);
        assert_eq!(
            settled_elsewhere(
                &mut settle,
                later.checked_add(SETTLE).expect("in range"),
                &job.plan,
                WINDOW
            ),
            None,
            "back at the size the capture reads at"
        );
        assert_eq!(settle.at(), WINDOW);
    }

    #[test]
    fn values_drawn_at_another_size_are_read_when_the_window_has_it() {
        let (events, readings) = mpsc::channel::<Reading>();
        let (control, told) = mpsc::channel();
        let job = job(vec![red_at("large", 0, 0), drawn_at("small", 4, 6, SMALL)]);
        let mut frames = frames_at(&job, WINDOW, events.clone(), control.clone()).expect("a value");
        let copied = |frames: &FrameReader<Reading>| {
            frames
                .layout
                .copies()
                .iter()
                .map(|copy| copy.from)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            copied(&frames),
            [Region::new(0, 0, 2, 2).expect("not empty")]
        );
        let red = frame_image(2, 2, |_, _| true);
        frames.handle(Ok(frame(&red, WINDOW)));
        assert_eq!(flags(&readings), [("large".to_owned(), Value::Flag(true))]);
        assert_eq!(told.try_recv(), Err(TryRecvError::Empty));
        assert_eq!(frames.handle(outside(SMALL)), Duration::ZERO);
        assert_eq!(
            told.try_recv(),
            Ok(Control::Resizing {
                size: SMALL,
                problems: Some(vec![None, Some(NOT_DRAWN.to_owned())])
            })
        );
        readings.try_iter().for_each(drop);
        // Once SMALL settles, the capture for it carries the problems, so
        // the value drawn only at the old size says nothing new.
        let mut small = frames_at(&job, SMALL, events, control).expect("a value");
        assert_eq!(
            copied(&small),
            [Region::new(4, 6, 2, 2).expect("not empty")]
        );
        small.carry(&Recovery::new(), vec![Some(NOT_DRAWN.to_owned()), None]);
        small.handle(Ok(frame(&red, SMALL)));
        assert_eq!(problems(&readings), Vec::<String>::new());
        let mut fresh =
            frames_at(&job, SMALL, mpsc::channel().0, mpsc::channel().0).expect("a value");
        fresh.carry(&Recovery::new(), vec![None]);
        assert_eq!(
            fresh.problems,
            [None, None],
            "a list for other values is left out"
        );
        let (said, heard) = mpsc::channel::<Reading>();
        let mut uncarried = frames_at(&job, SMALL, said, mpsc::channel().0).expect("a value");
        uncarried.handle(Ok(frame(&red, SMALL)));
        assert_eq!(problems(&heard), [format!("value 0 {NOT_DRAWN}")]);
    }

    #[test]
    fn a_value_that_scales_reads_at_each_size_from_its_scaled_region() {
        let (events, readings) = mpsc::channel::<Reading>();
        let (control, told) = mpsc::channel();
        let mut fixed = scaling("fixed", 0, 0, WINDOW, SizeChangesDef::Rarely);
        fixed.check.fixes.push(SavedFix {
            window_size: SMALL,
            region: Region::new(10, 12, 2, 2).expect("not empty"),
            check: None,
        });
        let job = job(vec![
            scaling("scaled", 4, 6, WINDOW, SizeChangesDef::Often),
            fixed,
            // Pinned left at 798 of 800, it scales to 638 of 640 and fits.
            scaling("edge", 798, 0, WINDOW, SizeChangesDef::Often),
        ]);
        let mut frames = frames_at(&job, WINDOW, events.clone(), control.clone()).expect("values");
        let red = frame_image(16, 16, |_, _| true);
        frames.handle(Ok(frame(&red, SMALL)));
        let placed = Placed::new(&job.plan, SMALL);
        assert!(placed.reads());
        assert!(matches!(
            told.try_recv(),
            Ok(Control::Resizing { size: SMALL, .. })
        ));
        // Its edges at 4 and 6 across scale to 3.2 and 4.8, so 3 to 5, and
        // at 6 and 8 down to 4.8 and 6.4, so 5 to 6: one row. The fix is
        // read exactly where it was drawn.
        let small =
            FrameReader::new(&job, placed, events, control, mpsc::channel().1).expect("values");
        assert_eq!(
            small
                .layout
                .copies()
                .iter()
                .map(|copy| copy.from)
                .collect::<Vec<_>>(),
            [
                Region::new(3, 5, 2, 1).expect("not empty"),
                Region::new(10, 12, 2, 2).expect("not empty"),
                Region::new(638, 0, 2, 2).expect("not empty"),
            ]
        );
        readings.try_iter().for_each(drop);
    }

    #[test]
    fn a_size_where_nothing_reads_is_said_once_the_reader_resolved_it() {
        let (events, readings) = mpsc::channel::<Reading>();
        // Pinned left at the right edge, it scales past a narrower window.
        let job = job(vec![scaling("edge", 798, 0, WINDOW, SizeChangesDef::Often)]);
        let mut problems_now = vec![None];
        let narrow = Size {
            width: 400,
            height: 600,
        };
        let placed = Placed::new(&job.plan, narrow);
        assert!(!placed.reads());
        say_unread(&placed, &mut problems_now, job.session, &events);
        assert_eq!(problems(&readings), [format!("value 0 {OUTSIDE}")]);
        // Narrower still, it is not read for the same reason, and the log
        // says nothing new.
        let narrower = Size {
            width: 300,
            height: 600,
        };
        say_unread(
            &Placed::new(&job.plan, narrower),
            &mut problems_now,
            job.session,
            &events,
        );
        assert_eq!(problems(&readings), Vec::<String>::new(), "said once");
        assert_eq!(problems_now, [Some(OUTSIDE.to_owned())]);
        // A value that reads says nothing.
        say_unread(
            &Placed::new(&plan(vec![red_at("icon", 0, 0)]), WINDOW),
            &mut problems_now,
            job.session,
            &events,
        );
        assert_eq!(problems(&readings), Vec::<String>::new());
    }

    #[test]
    fn a_value_that_cannot_be_read_says_why() {
        let (events, readings) = mpsc::channel::<Reading>();
        let (control, _control) = mpsc::channel();
        let mut value = red_at("count", 0, 0);
        value.check.drawn.check = Check::Number {
            reader: DigitReader::new(RED, 10),
            whole: false,
        };
        let job = job(vec![value]);
        let mut frames = frames_at(&job, WINDOW, events, control).expect("values to read");
        let black = frame_image(2, 2, |_, _| false);
        frames.handle(Ok(frame(&black, WINDOW)));
        frames.handle(Ok(frame(&black, WINDOW)));
        assert_eq!(
            problems(&readings),
            ["value 0 cannot be read: the region's number could not be read"]
        );
    }

    #[test]
    fn a_closed_window_tells_the_reader_to_look_again() {
        let (events, _readings) = mpsc::channel::<Reading>();
        let (control, told) = mpsc::channel();
        let icon = job(vec![red_at("icon", 0, 0)]);
        let mut frames = frames_at(&icon, WINDOW, events, control).expect("values to read");
        let closed = io::Error::new(io::ErrorKind::NotConnected, "the captured window closed");
        assert_eq!(frames.handle(Err(closed)), Duration::ZERO);
        assert_eq!(
            told.try_recv(),
            Ok(Control::Lost(
                "its capture ended: the captured window closed".to_owned()
            ))
        );
        let none = job(Vec::new());
        let nothing = frames_at(
            &none,
            WINDOW,
            mpsc::channel::<Reading>().0,
            mpsc::channel().0,
        );
        assert!(nothing.is_none(), "nothing to read");
    }

    #[test]
    fn frames_that_keep_failing_restart_the_capture() {
        let (events, readings) = mpsc::channel::<Reading>();
        let (control, told) = mpsc::channel();
        let icon = job(vec![red_at("icon", 0, 0)]);
        let mut frames = frames_at(&icon, WINDOW, events, control).expect("values to read");
        let red = frame_image(2, 2, |_, _| true);
        let failed = || Err(io::Error::other("the copy failed"));
        frames.handle(Ok(frame(&red, WINDOW)));
        for _ in 1..RESTART_AFTER {
            frames.handle(failed());
        }
        frames.handle(Ok(frame(&red, WINDOW)));
        for _ in 1..RESTART_AFTER {
            frames.handle(failed());
        }
        // A saved region that does not fit its own window size fails every
        // capture alike, so it does not count.
        frames.handle(outside(WINDOW));
        assert_eq!(
            told.try_recv(),
            Err(TryRecvError::Empty),
            "a good frame starts the count again"
        );
        readings.try_iter().for_each(drop);
        assert_eq!(frames.handle(failed()), Duration::ZERO);
        assert_eq!(
            told.try_recv(),
            Ok(Control::Failing("the copy failed".to_owned()))
        );
        let restart: Vec<Reading> = readings.try_iter().collect();
        assert_eq!(
            restart,
            [
                Reading::Problem {
                    session: SessionId(3),
                    problem: "a frame cannot be read: the copy failed".to_owned()
                },
                Reading::Unseen {
                    session: SessionId(3)
                },
            ]
        );
        assert_eq!(frames.handle(Ok(frame(&red, WINDOW))), FIND_AGAIN);
        frames.handle(failed());
        assert_eq!(
            readings.try_recv(),
            Err(TryRecvError::Empty),
            "the capture ended, so what it hands over until it closes is not read"
        );
        assert_eq!(told.try_recv(), Err(TryRecvError::Empty));
    }

    #[test]
    fn a_capture_that_ended_reads_nothing_more() {
        let (events, readings) = mpsc::channel::<Reading>();
        let (control, told) = mpsc::channel();
        let values = || vec![red_at("icon", 0, 0), drawn_at("small", 0, 0, SMALL)];
        let red = frame_image(2, 2, |_, _| true);
        let closed = || Err(io::Error::new(io::ErrorKind::NotConnected, "closed"));
        // Another size does not end it: the reader ends it once the size
        // settled.
        for ending in [
            closed(),
            Err(io::Error::new(io::ErrorKind::NotConnected, "lost")),
        ] {
            let job = job(values());
            let mut frames =
                frames_at(&job, WINDOW, events.clone(), control.clone()).expect("values to read");
            frames.handle(Ok(frame(&red, WINDOW)));
            frames.handle(ending);
            assert!(told.try_recv().is_ok(), "the capture ended");
            readings.try_iter().for_each(drop);
            frames.handle(Ok(frame(&red, WINDOW)));
            frames.handle(closed());
            assert_eq!(readings.try_recv(), Err(TryRecvError::Empty));
            assert_eq!(told.try_recv(), Err(TryRecvError::Empty));
        }
    }

    #[test]
    fn a_restarted_capture_logs_the_same_failure_once_and_says_when_it_reads() {
        let (events, readings) = mpsc::channel::<Reading>();
        let (control, told) = mpsc::channel();
        let icon = job(vec![red_at("icon", 0, 0)]);
        let mut frames = frames_at(&icon, WINDOW, events, control).expect("values to read");
        let mut recovery = Recovery::new();
        recovery.failed(
            "the capture starts again".to_owned(),
            Some("the copy failed".to_owned()),
        );
        frames.carry(&recovery, Vec::new());
        frames.handle(Err(io::Error::other("the copy failed")));
        assert_eq!(problems(&readings), Vec::<String>::new(), "logged already");
        let red = frame_image(2, 2, |_, _| true);
        frames.handle(Ok(frame(&red, WINDOW)));
        frames.handle(Ok(frame(&red, WINDOW)));
        assert_eq!(told.try_iter().collect::<Vec<_>>(), [Control::Recovered]);
    }

    #[test]
    fn recovery_waits_longer_after_each_failure_until_a_frame_is_read() {
        let mut recovery = Recovery::new();
        let failed =
            |recovery: &mut Recovery, problem: &str| recovery.failed(problem.to_owned(), None);
        assert_eq!(failed(&mut recovery, "lost"), (FIND_AGAIN, true));
        assert_eq!(
            failed(&mut recovery, "lost"),
            (Duration::from_secs(2), false)
        );
        assert_eq!(
            failed(&mut recovery, "gone"),
            (Duration::from_secs(4), true)
        );
        for _ in 0..10 {
            failed(&mut recovery, "gone");
        }
        assert_eq!(failed(&mut recovery, "gone"), (RESTART_WAIT_MAX, false));
        recovery.recovered();
        assert_eq!(failed(&mut recovery, "gone"), (FIND_AGAIN, true));
    }

    /// A capture the fake system started: its window, its crop, and the
    /// handler that receives its frames.
    type Started = (u64, Crop, FrameHandler);

    /// Where the fake system's window list is answered.
    type Answer = SyncSender<io::Result<Vec<WindowEntry>>>;

    /// A request for whether a window shows, and where it is answered.
    type PlaceAsk = (u64, SyncSender<bool>);

    /// A system with set processes that asks the test for each window list,
    /// and whose captures hand their handler to the test. Each window's
    /// place cannot be read, unless the test answers where windows are.
    struct FakeSystem {
        processes: Vec<ProcessEntry>,
        lists: Sender<Answer>,
        places: Option<Sender<PlaceAsk>>,
        captures: Sender<Started>,
        /// The size every window is framed at, or `None` when it is not
        /// known.
        framed: Option<Size>,
    }

    /// The test's side of a [`FakeSystem`]. Dropping it makes every request
    /// to the test fail, so a reader waiting on one never hangs.
    struct Desk {
        lists: Receiver<Answer>,
        places: Option<Receiver<PlaceAsk>>,
        /// Dropped to make every capture fail.
        started: Option<Receiver<Started>>,
    }

    impl Desk {
        /// Answers whether the window the reader asks about shows, and
        /// returns that window.
        fn place(&self, shown: bool) -> u64 {
            let (window, answer) = self
                .places
                .as_ref()
                .expect("places are answered")
                .recv_timeout(PATIENCE)
                .expect("a request");
            answer.send(shown).expect("the reader waits");
            window
        }

        /// Answers the reader's next request for the window list.
        fn answer(&self, windows: io::Result<Vec<WindowEntry>>) {
            let answer = self.lists.recv_timeout(PATIENCE).expect("a request");
            answer.send(windows).expect("the reader waits");
        }

        /// Whether the reader asks for the window list within `QUIET`.
        fn asks_soon(&self) -> bool {
            self.lists.recv_timeout(QUIET).is_ok()
        }

        /// The reader's next capture.
        fn capture(&self) -> Started {
            self.started
                .as_ref()
                .expect("captures work")
                .recv_timeout(PATIENCE)
                .expect("a capture")
        }
    }

    impl FakeSystem {
        fn new() -> (Arc<Self>, Desk) {
            Self::start(false, None)
        }

        /// A system whose windows' places the test answers.
        fn placing() -> (Arc<Self>, Desk) {
            Self::start(true, None)
        }

        /// A system that frames every window at `size`.
        fn framing(size: Size) -> (Arc<Self>, Desk) {
            Self::start(false, Some(size))
        }

        fn start(placing: bool, framed: Option<Size>) -> (Arc<Self>, Desk) {
            let process = |id, executable: &str| ProcessEntry {
                id: ProcessId::new(id),
                parent: ProcessId::new(1),
                executable: executable.to_owned(),
                threads: 1,
            };
            let processes = vec![
                process(7, "Other.exe"),
                process(42, "Example.exe"),
                process(43, "Example.exe"),
            ];
            let (lists, requests) = mpsc::channel();
            let (captures, started) = mpsc::channel();
            let (places, asked) = if placing {
                let (places, asked) = mpsc::channel();
                (Some(places), Some(asked))
            } else {
                (None, None)
            };
            let desk = Desk {
                lists: requests,
                places: asked,
                started: Some(started),
            };
            let system = Self {
                processes,
                lists,
                places,
                captures,
                framed,
            };
            (Arc::new(system), desk)
        }
    }

    struct FakeSession;

    impl CaptureSession for FakeSession {
        fn close(self) -> io::Result<()> {
            Ok(())
        }
    }

    impl System for FakeSystem {
        fn processes(&self) -> io::Result<Vec<ProcessEntry>> {
            Ok(self.processes.clone())
        }

        fn windows(&self) -> io::Result<Vec<WindowEntry>> {
            let (answer, answered) = mpsc::sync_channel(1);
            let gone = || io::Error::from(io::ErrorKind::BrokenPipe);
            self.lists.send(answer).map_err(|_gone| gone())?;
            answered.recv().map_err(|_gone| gone())?
        }

        fn window_place(&self, window: u64) -> io::Result<Option<WindowPlace>> {
            let places = self
                .places
                .as_ref()
                .ok_or_else(|| io::Error::from(io::ErrorKind::Unsupported))?;
            let (answer, answered) = mpsc::sync_channel(1);
            let gone = || io::Error::from(io::ErrorKind::BrokenPipe);
            places.send((window, answer)).map_err(|_gone| gone())?;
            let shown = answered.recv().map_err(|_gone| gone())?;
            Ok(Some(WindowPlace {
                bounds: Bounds {
                    left: 0,
                    top: 0,
                    right: 800,
                    bottom: 600,
                },
                shown,
                front: false,
            }))
        }

        fn frame_size(&self, _window: u64) -> io::Result<Option<Size>> {
            Ok(self.framed)
        }

        fn executable_build(
            &self,
            _process: appytappy_config::anti_cheat::ProcessId,
        ) -> io::Result<appytappy_config::app_file::BuildDef> {
            Err(io::Error::from(io::ErrorKind::Unsupported))
        }

        fn screen_pixel(&self, _: i32, _: i32) -> io::Result<Rgb> {
            Err(io::Error::from(io::ErrorKind::Unsupported))
        }
    }

    impl Capture for FakeSystem {
        type Session = FakeSession;

        fn start(
            &self,
            window: u64,
            crop: Crop,
            _throttle: Duration,
            handler: FrameHandler,
        ) -> io::Result<FakeSession> {
            self.captures
                .send((window, crop, handler))
                .map_err(|_unheard| io::Error::other("no capture today"))?;
            Ok(FakeSession)
        }
    }

    fn window(handle: u64, process: u32, title: &str, visible: bool) -> WindowEntry {
        WindowEntry {
            handle,
            process: ProcessId::new(process),
            title: title.to_owned(),
            class: "Frame".to_owned(),
            visible,
            bounds: Bounds {
                left: 0,
                top: 0,
                right: 800,
                bottom: 600,
            },
        }
    }

    /// Windows of `Other.exe` and of two `Example.exe` processes; the
    /// visible main window belongs to the second.
    fn desktop() -> Vec<WindowEntry> {
        vec![
            window(1, 7, "Main window", true),
            window(2, 42, "Settings", true),
            window(3, 42, "Main window", false),
            window(4, 43, "Main window", true),
        ]
    }

    /// Stops `reader` and checks its thread ended, which drops its senders.
    /// The desk goes first, so a reader that waits on it cannot hang, and a
    /// reader that does not end within `PATIENCE` fails the test rather than
    /// hanging it.
    fn stopped(reader: Reader, desk: Desk, readings: &Receiver<Reading>) {
        drop(desk);
        let (done, ended) = mpsc::channel();
        thread::spawn(move || done.send(reader.stop().map_err(|error| error.to_string())));
        ended
            .recv_timeout(PATIENCE)
            .expect("the reader ends when the session stops")
            .expect("stop");
        let left: Vec<Reading> = readings.try_iter().collect();
        assert_eq!(left, [], "nothing more was sent");
        assert_eq!(readings.try_recv(), Err(TryRecvError::Disconnected));
    }

    #[test]
    fn the_reader_finds_the_best_window_and_captures_again_after_a_resize_or_a_close() {
        let (system, desk) = FakeSystem::new();
        let (events, readings) = mpsc::channel::<Reading>();
        let values = vec![red_at("icon", 4, 6), drawn_at("small", 2, 2, SMALL)];
        // The window is checked only every minute, so the new size is acted
        // on as soon as it settles, not at the next check, and the check
        // itself, which the desk would have to answer, does not run.
        let reader = Reader::checking_every(Duration::from_secs(60), system, job(values), events)
            .expect("start");
        desk.answer(Ok(Vec::new()));
        assert_eq!(
            readings.recv_timeout(PATIENCE),
            Ok(Reading::Problem {
                session: SessionId(3),
                problem: "no window of Example.exe fits yet".to_owned()
            })
        );
        assert!(!desk.asks_soon(), "it waits before looking again");
        desk.answer(Ok(desktop()));
        let (handle, crop, mut first) = desk.capture();
        assert_eq!(handle, 4, "the visible main window of either Example.exe");
        let copied = |crop: &Crop| match crop {
            Crop::Layout(layout) => layout.copies().iter().map(|copy| copy.from).collect(),
            Crop::Whole => Vec::new(),
        };
        assert_eq!(copied(&crop), [Region::new(4, 6, 2, 2).expect("not empty")]);
        let red = frame_image(2, 2, |_, _| true);
        first(Ok(frame(&red, WINDOW)));
        let sample = readings.recv_timeout(PATIENCE).expect("a sample");
        assert!(
            matches!(
                &sample,
                Reading::Sample {
                    session: SessionId(3),
                    value: Value::Flag(true),
                    ..
                }
            ),
            "{sample:?}"
        );
        assert_eq!(
            readings.recv_timeout(PATIENCE),
            Ok(Reading::ValueProblem {
                session: SessionId(3),
                value: 1,
                problem: NOT_DRAWN.to_owned()
            }),
            "a value drawn only at another size says so once"
        );
        first(outside(SMALL));
        assert_eq!(
            readings.recv_timeout(PATIENCE),
            Ok(Reading::Unseen {
                session: SessionId(3)
            }),
            "the values it read are out of sight"
        );
        let resizing = Instant::now();
        desk.answer(Ok(desktop()));
        let (resized, crop, mut second) = desk.capture();
        assert!(
            resizing.elapsed() < Duration::from_secs(1),
            "captured again once the size settled"
        );
        assert_eq!(resized, 4);
        assert_eq!(
            copied(&crop),
            [Region::new(2, 2, 2, 2).expect("not empty")],
            "the value drawn at the new size"
        );
        second(Err(io::Error::new(io::ErrorKind::NotConnected, "closed")));
        assert_eq!(
            readings.recv_timeout(PATIENCE),
            Ok(Reading::Problem {
                session: SessionId(3),
                problem: "its capture ended: closed".to_owned()
            })
        );
        desk.answer(Ok(desktop()));
        let (again, _, _) = desk.capture();
        assert_eq!(again, 4, "it looks for the window again");
        // A real capture drops its handler when it closes.
        drop((first, second));
        stopped(reader, desk, &readings);
    }

    #[test]
    fn a_settled_size_where_nothing_reads_keeps_the_capture_and_says_why() {
        let (system, desk) = FakeSystem::new();
        let (events, readings) = mpsc::channel::<Reading>();
        // Pinned left at the right edge, it scales past a narrower window.
        let edge = scaling("edge", 798, 0, WINDOW, SizeChangesDef::Often);
        let reader = Reader::start(system, job(vec![edge]), events).expect("start");
        desk.answer(Ok(desktop()));
        let (_, _, mut first) = desk.capture();
        let red = frame_image(2, 2, |_, _| true);
        first(Ok(frame(&red, WINDOW)));
        assert!(matches!(
            readings.recv_timeout(PATIENCE),
            Ok(Reading::Sample { .. })
        ));
        let narrow = Size {
            width: 400,
            height: 600,
        };
        first(Ok(frame(&red, narrow)));
        assert_eq!(
            readings.recv_timeout(PATIENCE),
            Ok(Reading::Unseen {
                session: SessionId(3)
            })
        );
        // Once the size settles, the reader says why the value is not read,
        // though the window draws no more frames, and says it once.
        assert_eq!(
            readings.recv_timeout(PATIENCE),
            Ok(Reading::ValueProblem {
                session: SessionId(3),
                value: 0,
                problem: OUTSIDE.to_owned()
            })
        );
        first(Ok(frame(&red, narrow)));
        assert!(!desk.asks_soon(), "the capture goes on");
        assert_eq!(readings.try_recv(), Err(TryRecvError::Empty));
        drop(first);
        stopped(reader, desk, &readings);
    }

    #[test]
    fn a_capture_starts_at_the_size_the_window_is_framed_at() {
        let (system, desk) = FakeSystem::framing(SMALL);
        let (events, readings) = mpsc::channel::<Reading>();
        let values = vec![red_at("icon", 4, 6), drawn_at("small", 2, 2, SMALL)];
        let reader = Reader::start(system, job(values), events).expect("start");
        desk.answer(Ok(desktop()));
        let (_, crop, mut first) = desk.capture();
        let copied = |crop: &Crop| match crop {
            Crop::Layout(layout) => layout.copies().iter().map(|copy| copy.from).collect(),
            Crop::Whole => Vec::new(),
        };
        assert_eq!(
            copied(&crop),
            [Region::new(2, 2, 2, 2).expect("not empty")],
            "the value drawn at the size the window has, from the first frame"
        );
        let red = frame_image(2, 2, |_, _| true);
        first(Ok(frame(&red, SMALL)));
        assert_eq!(
            readings.recv_timeout(PATIENCE),
            Ok(Reading::ValueProblem {
                session: SessionId(3),
                value: 0,
                problem: NOT_DRAWN.to_owned()
            })
        );
        assert!(matches!(
            readings.recv_timeout(PATIENCE),
            Ok(Reading::Sample { .. })
        ));
        // The system's size is a guess: when frames show another, the
        // capture follows them once, and is not sent back to the guess.
        first(Ok(frame(&red, WINDOW)));
        assert_eq!(
            readings.recv_timeout(PATIENCE),
            Ok(Reading::Unseen {
                session: SessionId(3)
            })
        );
        desk.answer(Ok(desktop()));
        let (_, crop, mut second) = desk.capture();
        assert_eq!(copied(&crop), [Region::new(4, 6, 2, 2).expect("not empty")]);
        second(Ok(frame(&red, WINDOW)));
        assert!(matches!(
            readings.recv_timeout(PATIENCE),
            Ok(Reading::Sample { .. })
        ));
        assert_eq!(
            readings.recv_timeout(PATIENCE),
            Ok(Reading::ValueProblem {
                session: SessionId(3),
                value: 1,
                problem: NOT_DRAWN.to_owned()
            })
        );
        assert!(!desk.asks_soon(), "the capture goes on");
        drop((first, second));
        stopped(reader, desk, &readings);
    }

    #[test]
    fn a_framed_size_where_nothing_reads_starts_at_the_drawn_size() {
        let narrow = Size {
            width: 400,
            height: 600,
        };
        let (system, desk) = FakeSystem::framing(narrow);
        let (events, readings) = mpsc::channel::<Reading>();
        let reader = Reader::start(system, job(vec![red_at("icon", 4, 6)]), events).expect("start");
        desk.answer(Ok(desktop()));
        let (_, crop, handler) = desk.capture();
        assert!(
            matches!(&crop, Crop::Layout(layout)
            if layout.copies().iter().map(|copy| copy.from).eq([
                Region::new(4, 6, 2, 2).expect("not empty")
            ])),
            "{crop:?}"
        );
        drop(handler);
        stopped(reader, desk, &readings);
    }

    #[test]
    fn a_reader_stopped_while_it_waits_after_a_capture_ended_ends_at_once() {
        let (system, desk) = FakeSystem::new();
        let (events, readings) = mpsc::channel::<Reading>();
        let reader = Reader::start(system, job(vec![red_at("icon", 0, 0)]), events).expect("start");
        desk.answer(Ok(desktop()));
        let (_, _, mut handler) = desk.capture();
        handler(Err(io::Error::new(io::ErrorKind::NotConnected, "closed")));
        assert_eq!(
            readings.recv_timeout(PATIENCE),
            Ok(Reading::Problem {
                session: SessionId(3),
                problem: "its capture ended: closed".to_owned()
            })
        );
        // It waits before it looks again, and the stop ends that wait.
        drop(handler);
        stopped(reader, desk, &readings);
    }

    #[test]
    fn a_capture_that_keeps_ending_is_logged_once_and_waits_longer() {
        let (system, desk) = FakeSystem::new();
        let (events, readings) = mpsc::channel::<Reading>();
        let check = Duration::from_secs(60);
        let reader = Reader::checking_every(check, system, job(vec![red_at("icon", 0, 0)]), events)
            .expect("start");
        let closed = || Err(io::Error::new(io::ErrorKind::NotConnected, "closed"));
        desk.answer(Ok(desktop()));
        let (_, _, mut first) = desk.capture();
        first(closed());
        assert_eq!(
            readings.recv_timeout(PATIENCE),
            Ok(Reading::Problem {
                session: SessionId(3),
                problem: "its capture ended: closed".to_owned()
            })
        );
        assert!(!desk.asks_soon(), "it waits before looking again");
        desk.answer(Ok(desktop()));
        let (_, _, mut second) = desk.capture();
        second(closed());
        assert_eq!(
            readings.recv_timeout(QUIET),
            Err(RecvTimeoutError::Timeout),
            "the same reason is logged once"
        );
        // The second wait is twice the first, so it outlasts FIND_AGAIN.
        assert!(
            matches!(
                desk.lists.recv_timeout(FIND_AGAIN),
                Err(RecvTimeoutError::Timeout)
            ),
            "it waits longer"
        );
        drop((first, second));
        stopped(reader, desk, &readings);
    }

    #[test]
    fn a_reader_waits_while_the_window_belongs_to_another_process() {
        let (system, desk) = FakeSystem::new();
        let (events, readings) = mpsc::channel::<Reading>();
        let reader = Reader::start(system, job(vec![red_at("icon", 0, 0)]), events).expect("start");
        let other = || Ok(vec![window(2, 42, "Main window", true)]);
        desk.answer(other());
        assert_eq!(
            readings.recv_timeout(PATIENCE),
            Ok(Reading::Problem {
                session: SessionId(3),
                problem:
                    "the window that fits best belongs to another process, so it is read once the \
                          session follows that process"
                        .to_owned()
            })
        );
        assert!(!desk.asks_soon(), "it waits before looking again");
        desk.answer(other());
        stopped(reader, desk, &readings);
    }

    #[test]
    fn a_reader_that_cannot_list_windows_says_so_once_and_waits() {
        let (system, desk) = FakeSystem::new();
        let (events, readings) = mpsc::channel::<Reading>();
        let reader = Reader::start(system, job(vec![red_at("icon", 0, 0)]), events).expect("start");
        desk.answer(Err(io::Error::other("denied")));
        assert_eq!(
            readings.recv_timeout(PATIENCE),
            Ok(Reading::Problem {
                session: SessionId(3),
                problem: "its windows cannot be listed: denied".to_owned()
            })
        );
        assert!(!desk.asks_soon(), "it waits before looking again");
        desk.answer(Err(io::Error::other("denied")));
        stopped(reader, desk, &readings);
    }

    #[test]
    fn a_window_that_cannot_be_captured_says_so_once_and_waits() {
        let (system, mut desk) = FakeSystem::new();
        drop(desk.started.take());
        let (events, readings) = mpsc::channel::<Reading>();
        let reader = Reader::start(system, job(vec![red_at("icon", 0, 0)]), events).expect("start");
        desk.answer(Ok(desktop()));
        assert_eq!(
            readings.recv_timeout(PATIENCE),
            Ok(Reading::Problem {
                session: SessionId(3),
                problem: "its window cannot be captured: no capture today".to_owned()
            })
        );
        assert!(!desk.asks_soon(), "it waits before looking again");
        desk.answer(Ok(desktop()));
        stopped(reader, desk, &readings);
    }

    #[test]
    fn a_minimized_window_is_not_captured_until_it_shows() {
        let (system, desk) = FakeSystem::placing();
        let (events, readings) = mpsc::channel::<Reading>();
        let reader = Reader::start(system, job(vec![red_at("icon", 0, 0)]), events).expect("start");
        desk.answer(Ok(desktop()));
        assert_eq!(desk.place(false), 4);
        assert_eq!(
            readings.recv_timeout(PATIENCE),
            Ok(Reading::Problem {
                session: SessionId(3),
                problem: "its window is minimized or hidden".to_owned()
            })
        );
        assert!(!desk.asks_soon(), "it waits before looking again");
        desk.answer(Ok(desktop()));
        assert_eq!(desk.place(true), 4);
        let (window, _, handler) = desk.capture();
        assert_eq!(window, 4, "the window shows again, so it is captured");
        drop(handler);
        stopped(reader, desk, &readings);
    }

    #[test]
    fn a_shown_window_is_taken_over_a_minimized_one_that_fits_as_well() {
        let (system, desk) = FakeSystem::placing();
        let (events, readings) = mpsc::channel::<Reading>();
        let reader = Reader::start(system, job(vec![red_at("icon", 0, 0)]), events).expect("start");
        let mut windows = desktop();
        windows.push(window(5, 43, "Main window", true));
        desk.answer(Ok(windows));
        assert_eq!(desk.place(false), 4);
        assert_eq!(desk.place(true), 5);
        let (window, _, handler) = desk.capture();
        assert_eq!(window, 5);
        drop(handler);
        stopped(reader, desk, &readings);
    }

    /// A system that answers from set lists, for checks on one thread.
    struct Still {
        windows: io::Result<Vec<WindowEntry>>,
        minimized: Vec<u64>,
    }

    impl System for Still {
        fn processes(&self) -> io::Result<Vec<ProcessEntry>> {
            Ok([42, 43]
                .into_iter()
                .map(|id| ProcessEntry {
                    id: ProcessId::new(id),
                    parent: ProcessId::new(1),
                    executable: "Example.exe".to_owned(),
                    threads: 1,
                })
                .collect())
        }

        fn windows(&self) -> io::Result<Vec<WindowEntry>> {
            match &self.windows {
                Ok(windows) => Ok(windows.clone()),
                Err(error) => Err(io::Error::new(error.kind(), error.to_string())),
            }
        }

        fn window_place(&self, window: u64) -> io::Result<Option<WindowPlace>> {
            Ok(Some(WindowPlace {
                bounds: Bounds {
                    left: 0,
                    top: 0,
                    right: 800,
                    bottom: 600,
                },
                shown: !self.minimized.contains(&window),
                front: false,
            }))
        }

        fn frame_size(&self, _window: u64) -> io::Result<Option<Size>> {
            Err(io::Error::from(io::ErrorKind::Unsupported))
        }

        fn executable_build(
            &self,
            _process: appytappy_config::anti_cheat::ProcessId,
        ) -> io::Result<appytappy_config::app_file::BuildDef> {
            Err(io::Error::from(io::ErrorKind::Unsupported))
        }

        fn screen_pixel(&self, _: i32, _: i32) -> io::Result<Rgb> {
            Err(io::Error::from(io::ErrorKind::Unsupported))
        }
    }

    #[test]
    fn a_capture_moves_on_when_its_window_hides_or_a_better_one_opens() {
        let icon = job(vec![red_at("icon", 0, 0)]);
        let check =
            |window, windows, minimized| moved_on(&Still { windows, minimized }, &icon, window);
        assert_eq!(
            check(4, Ok(desktop()), vec![4]).as_deref(),
            Some("its window is minimized or hidden")
        );
        assert_eq!(check(4, Ok(desktop()), Vec::new()), None, "4 fits best");
        let mut tied = desktop();
        tied.push(window(5, 43, "Main window", true));
        assert_eq!(check(5, Ok(tied), Vec::new()), None, "5 fits as well as 4");
        assert_eq!(
            check(2, Ok(desktop()), Vec::new()).as_deref(),
            Some("another window fits better, so it is read instead"),
            "a launcher's window gives way to the main window"
        );
        assert_eq!(
            check(2, Err(io::Error::other("denied")), Vec::new()),
            None,
            "a failing check keeps the capture"
        );
        assert_eq!(
            check(4, Ok(vec![window(1, 7, "Main window", true)]), Vec::new()).as_deref(),
            Some("no window of Example.exe fits yet"),
            "every window that fits closed"
        );
        let copies = || {
            Ok(vec![
                window(6, 42, "Main window", true),
                window(4, 43, "Main window", true),
            ])
        };
        assert_eq!(
            check(4, copies(), Vec::new()),
            None,
            "another copy's window fits as well"
        );
        assert_eq!(
            check(4, Ok(vec![window(6, 42, "Main window", true)]), Vec::new()).as_deref(),
            Some(
                "the window that fits best belongs to another process, so it is read once the \
                 session follows that process"
            ),
            "only another copy's window fits"
        );
    }

    #[test]
    fn the_window_read_is_the_sessions_process_even_when_minimized() {
        let icon = job(vec![red_at("icon", 0, 0)]);
        let find = |windows: Vec<WindowEntry>, minimized| {
            find_window(
                &Still {
                    windows: Ok(windows),
                    minimized,
                },
                &icon,
            )
            .expect("listed")
        };
        let copies = || {
            vec![
                window(6, 42, "Main window", true),
                window(4, 43, "Main window", true),
            ]
        };
        assert_eq!(find(copies(), Vec::new()), Found::Shown(4));
        assert_eq!(
            find(copies(), vec![4]),
            Found::Minimized,
            "detection follows process 43, so another copy's window is not read"
        );
        assert_eq!(
            find(vec![window(6, 42, "Main window", true)], Vec::new()),
            Found::Elsewhere,
            "another copy's window, until the session follows that copy"
        );
        assert_eq!(find(Vec::new(), Vec::new()), Found::Nothing);
    }

    #[test]
    fn a_capture_whose_window_no_longer_fits_best_is_left_for_the_better_one() {
        let (system, desk) = FakeSystem::new();
        let (events, readings) = mpsc::channel::<Reading>();
        let check = Duration::from_millis(50);
        let reader = Reader::checking_every(check, system, job(vec![red_at("icon", 0, 0)]), events)
            .expect("start");
        desk.answer(Ok(desktop()));
        let (first, _, handler) = desk.capture();
        assert_eq!(first, 4);
        desk.answer(Ok(desktop()));
        // A reader that moved on would say why before it looks again.
        let next = desk.lists.recv_timeout(PATIENCE).expect("the next check");
        assert_eq!(
            readings.try_recv(),
            Err(TryRecvError::Empty),
            "4 still fits, so it checks again"
        );
        let better = || {
            vec![
                window(4, 43, "Settings", true),
                window(5, 43, "Main window", true),
            ]
        };
        next.send(Ok(better())).expect("the reader waits");
        assert_eq!(
            readings.recv_timeout(PATIENCE),
            Ok(Reading::Problem {
                session: SessionId(3),
                problem: "another window fits better, so it is read instead".to_owned()
            })
        );
        assert_eq!(
            readings.recv_timeout(PATIENCE),
            Ok(Reading::Unseen {
                session: SessionId(3)
            })
        );
        desk.answer(Ok(better()));
        let (second, _, _) = desk.capture();
        assert_eq!(second, 5, "the reader captures the better window");
        drop(handler);
        stopped(reader, desk, &readings);
    }

    #[test]
    fn what_a_closed_capture_sent_is_dropped_but_a_stop_counts() {
        let (control, received) = mpsc::channel();
        let lost = || Control::Lost("closed".to_owned());
        let mut recovery = Recovery::new();
        recovery.failed("closed".to_owned(), None);
        for left in [
            lost(),
            Control::Resizing {
                size: SMALL,
                problems: None,
            },
            Control::Failing("x".to_owned()),
        ] {
            control.send(left).expect("send");
        }
        assert!(!stopped_meanwhile(&received, &mut recovery));
        assert_eq!(received.try_recv(), Err(TryRecvError::Empty));
        assert_eq!(recovery.wait, Duration::from_secs(2), "still failing");
        control.send(Control::Recovered).expect("send");
        assert!(!stopped_meanwhile(&received, &mut recovery));
        assert_eq!(recovery.wait, FIND_AGAIN, "a frame was read meanwhile");
        for left in [Control::Stop, lost(), Control::Stop] {
            control.send(left).expect("send");
        }
        assert!(stopped_meanwhile(&received, &mut recovery));
        assert_eq!(received.try_recv(), Err(TryRecvError::Empty));
    }

    #[test]
    fn a_capture_whose_frames_keep_failing_starts_again_after_a_wait() {
        let (system, desk) = FakeSystem::new();
        let (events, readings) = mpsc::channel::<Reading>();
        // No check of the captured window comes in the test, so every window
        // list it answers is one the reader asked for to capture again.
        let check = Duration::from_secs(60);
        let reader = Reader::checking_every(check, system, job(vec![red_at("icon", 0, 0)]), events)
            .expect("start");
        let fail_all = |handler: &mut FrameHandler| {
            for _ in 0..RESTART_AFTER {
                handler(Err(io::Error::other("the copy failed")));
            }
        };
        desk.answer(Ok(desktop()));
        let (_, _, mut first) = desk.capture();
        fail_all(&mut first);
        let problem = |problem: &str| Reading::Problem {
            session: SessionId(3),
            problem: problem.to_owned(),
        };
        assert_eq!(
            readings.recv_timeout(PATIENCE),
            Ok(problem("a frame cannot be read: the copy failed"))
        );
        assert_eq!(
            readings.recv_timeout(PATIENCE),
            Ok(problem(
                "5 frames in a row cannot be read, so its capture starts again: the copy failed"
            ))
        );
        assert!(!desk.asks_soon(), "it waits before capturing again");
        desk.answer(Ok(desktop()));
        let (_, _, mut second) = desk.capture();
        let red = frame_image(2, 2, |_, _| true);
        second(Ok(frame(&red, WINDOW)));
        assert!(
            matches!(readings.recv_timeout(PATIENCE), Ok(Reading::Sample { .. })),
            "the new capture reads"
        );
        assert!(!desk.asks_soon(), "a capture that reads is kept");
        fail_all(&mut second);
        let again: Vec<Reading> = (0..3)
            .filter_map(|_| readings.recv_timeout(PATIENCE).ok())
            .collect();
        assert_eq!(
            again,
            [
                problem("a frame cannot be read: the copy failed"),
                Reading::Unseen {
                    session: SessionId(3)
                },
                problem(
                    "5 frames in a row cannot be read, so its capture starts again: the copy \
                     failed"
                ),
            ],
            "after a good frame, a failure is news again"
        );
        desk.answer(Ok(desktop()));
        let (_, _, mut third) = desk.capture();
        fail_all(&mut third);
        assert_eq!(
            readings.recv_timeout(QUIET),
            Err(RecvTimeoutError::Timeout),
            "the same failure is logged once"
        );
        assert!(!desk.asks_soon(), "it waits longer");
        drop((first, second, third));
        stopped(reader, desk, &readings);
    }
}
