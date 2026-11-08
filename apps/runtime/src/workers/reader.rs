//! Reading one attached application's values from its window.
//!
//! Each running session has a reader thread. It finds the application's
//! window with the application's
//! [`appytappy_config::window_match::WindowMatch`], among the windows of
//! every running process whose executable fits it, as the setup app does.
//! Then it captures one region of the window: the smallest rectangle that
//! holds the regions of every value drawn at the window's current size, so
//! the capture copies no more pixels than the values need. Windows pushes a
//! frame only when the window's content changes, so a still window is
//! copied no more, and the [`ReadSchedule`] backs off while the values hold
//! still. What a still window costs is the checks below: the reader lists
//! the processes and windows every [`CHECK_AGAIN`], and the capture checks
//! its graphics device on a timer while no frame comes.
//! A value is sent to the owner only when it reads differently from its last
//! read.
//!
//! A value's region means something only at the window size it was drawn
//! at. When the window takes another size at which other values were drawn,
//! the reader captures those instead; a value whose size the window does not
//! have is not read, and the reader says so once.
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
//! window after a launcher. While no window fits, or the one that fits best
//! is minimized, it looks again every [`FIND_AGAIN`]. Each time a capture
//! ends, the values it read can no longer be seen, so the reader sends
//! [`Reading::Unseen`], and the capture's handler sends nothing more.

use std::io;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use appytappy_config::signal::{SignalName, Value};
use appytappy_sources::schedule::{Activity, ReadInterval, ReadSchedule};
use appytappy_sources::screen::{
    Capture, CaptureSession, Crop, Frame, OutsideWindow, Region, Size,
};
use appytappy_sources::system::{System, WindowPlace, best_windows};

use crate::plan::AppPlan;

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
    /// The attached process's executable, such as `Example.exe`, for the
    /// log.
    pub(crate) executable: String,
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
    /// The values read so far can no longer be seen, because their capture
    /// ended: the window closed, changed size, was minimized or hidden, or
    /// gave way to a window that fits better, or the capture lost its
    /// graphics device or its frames kept failing. Rules on them should
    /// forget them rather than hold their last readings.
    Unseen {
        /// The session.
        session: SessionId,
    },
}

/// What reaches the reader thread.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Control {
    /// The session ended.
    Stop,
    /// The capture ended, such as when the window closed, and this says
    /// why, for the log.
    Lost(String),
    /// The window took a size at which other values were drawn, so the
    /// capture starts again for those.
    Resized(Size),
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
    // The window size whose values the capture reads, until a frame shows
    // the window has another. A plan always has a value to read.
    let Some(mut size) = job.plan.values.first().map(|value| value.check.window_size) else {
        return;
    };
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
                if !report(&mut reported, minimized(job)) || !wait() {
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
            Err(error) => {
                let problem = format!("its windows cannot be listed: {error}");
                if !report(&mut reported, problem) || !wait() {
                    return;
                }
                continue;
            },
        };
        let Some(mut frames) = FrameReader::new(job, size, events.clone(), lost.clone()) else {
            return;
        };
        frames.carry(&recovery);
        let crop = Crop::Region(frames.bounds);
        let throttle = ReadInterval::DEFAULT.min();
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
        let next = watch(system, job, window, check, control, &mut recovery);
        if let Err(error) = session.close() {
            report(&mut reported, format!("its capture ended badly: {error}"));
        }
        if stopped_meanwhile(control, &mut recovery) {
            return;
        }
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
            Ok(Control::Recovered) => {},
            Ok(Control::Resized(resized)) => size = resized,
            Ok(Control::Lost(problem)) => {
                let (pause_for, new) = recovery.failed(problem.clone(), None);
                if (new && !report(&mut reported, problem)) || !pause(control, pause_for) {
                    return;
                }
            },
            Ok(Control::Failing(error)) => {
                let problem = format!(
                    "{RESTART_AFTER} frames in a row cannot be read, so its capture starts \
                     again: {error}"
                );
                let (pause_for, new) = recovery.failed(problem.clone(), Some(error));
                if (new && !report(&mut reported, problem)) || !pause(control, pause_for) {
                    return;
                }
            },
            Ok(Control::Stop) => return,
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
            Control::Lost(_) | Control::Resized(_) | Control::Failing(_) => {},
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

/// Waits while `window` is captured, checking it every `check`, and returns
/// what ended the capture, or why the reader should look for the window
/// again.
fn watch<S: System>(
    system: &S,
    job: &Job,
    window: u64,
    check: Duration,
    control: &Receiver<Control>,
    recovery: &mut Recovery,
) -> Result<Control, String> {
    loop {
        match control.recv_timeout(check) {
            Ok(Control::Recovered) => recovery.recovered(),
            Ok(next) => return Ok(next),
            Err(RecvTimeoutError::Timeout) => {
                if let Some(problem) = moved_on(system, job, window) {
                    return Err(problem);
                }
            },
            Err(RecvTimeoutError::Disconnected) => return Ok(Control::Stop),
        }
    }
}

/// What the reader found to capture.
#[derive(Debug, PartialEq, Eq)]
enum Found {
    /// The first window that fits the app best and shows.
    Shown(u64),
    /// Windows fit, but every one that fits best is minimized.
    Minimized,
    /// No window fits.
    Nothing,
}

/// The first shown window among the visible windows that fit the app best.
fn find_window<S: System>(system: &S, job: &Job) -> io::Result<Found> {
    let processes = system.processes()?;
    let best = best_windows(&job.plan.window, processes, system.windows()?);
    if best.is_empty() {
        return Ok(Found::Nothing);
    }
    Ok(best
        .iter()
        .find(|entry| shown(system.window_place(entry.handle)))
        .map_or(Found::Minimized, |entry| Found::Shown(entry.handle)))
}

/// Whether a window shows. A window whose place cannot be read counts as
/// shown, so a failing check never stops a capture that works.
fn shown(place: io::Result<Option<WindowPlace>>) -> bool {
    !matches!(place, Ok(None | Some(WindowPlace { shown: false, .. })))
}

fn minimized(job: &Job) -> String {
    format!("{}'s window is minimized or hidden", job.plan.name)
}

fn no_window(job: &Job) -> String {
    format!(
        "no window of {} fits {}'s window yet",
        job.executable, job.plan.name
    )
}

/// Why the reader should stop capturing `window` and look again, or `None`
/// while it still shows and fits the app as well as any window. A check
/// that fails keeps the capture.
fn moved_on<S: System>(system: &S, job: &Job, window: u64) -> Option<String> {
    if !shown(system.window_place(window)) {
        return Some(minimized(job));
    }
    let processes = system.processes().ok()?;
    let best = best_windows(&job.plan.window, processes, system.windows().ok()?);
    if best.is_empty() {
        return Some(no_window(job));
    }
    (!best.iter().any(|entry| entry.handle == window)).then(|| {
        format!(
            "another window fits {}'s window better, so it is read instead",
            job.plan.name
        )
    })
}

/// The capture's frame handler: reads each value drawn at one window size
/// from a frame and sends what changed.
struct FrameReader<E> {
    session: SessionId,
    plan: Arc<AppPlan>,
    /// The window size whose values it reads.
    size: Size,
    /// The region the capture copies: the smallest that holds every value
    /// drawn at `size`.
    bounds: Region,
    /// Each value's region within `bounds`, or `None` for a value drawn at
    /// another size.
    regions: Vec<Option<Region>>,
    /// Each value's last reading.
    last: Vec<Option<Value>>,
    /// Each value's last problem.
    problems: Vec<Option<String>>,
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
    events: Sender<E>,
    control: Sender<Control>,
}

impl<E: From<Reading>> FrameReader<E> {
    /// The handler for the values of `job` drawn at `size`, or `None` when
    /// none was.
    fn new(job: &Job, size: Size, events: Sender<E>, control: Sender<Control>) -> Option<Self> {
        let drawn: Vec<Option<Region>> = job
            .plan
            .values
            .iter()
            .map(|value| (value.check.window_size == size).then_some(value.check.region))
            .collect();
        let bounds = drawn.iter().flatten().copied().reduce(union)?;
        let regions = drawn
            .into_iter()
            .map(|region| region.and_then(|region| within(region, bounds)))
            .collect();
        let count = job.plan.values.len();
        Some(Self {
            session: job.session,
            plan: Arc::clone(&job.plan),
            size,
            bounds,
            regions,
            last: vec![None; count],
            problems: vec![None; count],
            frame_problem: None,
            failures: 0,
            restarted: false,
            seen: false,
            ended: false,
            schedule: ReadSchedule::new(ReadInterval::DEFAULT),
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
                Some(window_size) if window_size != self.size => (window_size, None),
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
        let other_values = self
            .plan
            .values
            .iter()
            .any(|value| value.check.window_size == window_size);
        if window_size != self.size {
            self.unseen();
            if other_values {
                self.tell(Control::Resized(window_size));
                return Duration::ZERO;
            }
        }
        let mut changed = false;
        for position in 0..self.plan.values.len() {
            let read = self.read(position, window_size, frame.as_ref());
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
    /// one failed, and the frame error it logged already.
    fn carry(&mut self, recovery: &Recovery) {
        self.restarted = recovery.problem.is_some();
        self.frame_problem = recovery.frame_error.as_deref().map(frame_problem);
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
    fn read(
        &self,
        position: usize,
        window_size: Size,
        frame: Option<&Frame<'_>>,
    ) -> Option<Result<Value, String>> {
        let value = self.plan.values.get(position)?;
        // A frame of another size reaches here only when no value was drawn
        // at it. Values drawn at another size than this reader's are read
        // by the reader for that size, so they wait quietly.
        if window_size != self.size {
            // The message leaves out the window's size, so dragging a
            // window's corner logs it once rather than once for each size.
            return Some(Err(format!(
                "{} was drawn when the window was {}, and the window is now another size; \
                 resize the window or draw the value again",
                value.name, value.check.window_size
            )));
        }
        let region = (*self.regions.get(position)?)?;
        let image = frame?.image.crop(region)?;
        Some(
            value
                .check
                .check
                .read(&image)
                .map_err(|error| format!("{} cannot be read: {error}", value.name)),
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
            self.plan.values.get(position),
            frame,
        ) else {
            return false;
        };
        if last.as_ref() == Some(&reading) {
            return false;
        }
        *last = Some(reading.clone());
        self.seen = true;
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
        if slot.as_ref() != Some(&problem) {
            *slot = Some(problem.clone());
            self.problem(problem);
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
        if matches!(
            control,
            Control::Lost(_) | Control::Resized(_) | Control::Failing(_)
        ) {
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

/// The smallest region that holds both.
fn union(one: Region, other: Region) -> Region {
    let left = one.x().min(other.x());
    let top = one.y().min(other.y());
    let right = one.right().max(other.right());
    let bottom = one.bottom().max(other.bottom());
    Region::new(
        left,
        top,
        right.saturating_sub(left),
        bottom.saturating_sub(top),
    )
    .unwrap_or(one)
}

/// `region`, measured from the corner of `bounds`.
fn within(region: Region, bounds: Region) -> Option<Region> {
    let size = region.size();
    Region::new(
        region.x().checked_sub(bounds.x())?,
        region.y().checked_sub(bounds.y())?,
        size.width,
        size.height,
    )
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{SyncSender, TryRecvError};

    use appytappy_config::anti_cheat::{ProcessAccessPermit, ProcessId};
    use appytappy_config::rule::Rule;
    use appytappy_config::window_match::{MustMatch, Pattern, WindowMatch};
    use appytappy_sources::screen::check::{Check, ColorMatch};
    use appytappy_sources::screen::digits::DigitReader;
    use appytappy_sources::screen::saved::SavedCheck;
    use appytappy_sources::screen::{FrameHandler, Image};
    use appytappy_sources::system::{
        AccessAnswer, Bounds, NoProcess, ProcessEntry, Rgb, WindowEntry, WindowPlace,
    };

    use super::*;
    use crate::plan::ValuePlan;

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

    fn drawn_at(name: &str, x: u32, y: u32, window_size: Size) -> ValuePlan {
        ValuePlan {
            signal: SignalName::new(name).expect("valid"),
            name: name.to_owned(),
            check: SavedCheck {
                window_size,
                region: Region::new(x, y, 2, 2).expect("not empty"),
                check: Check::Color(ColorMatch {
                    color: RED,
                    tolerance: 10,
                }),
            },
        }
    }

    fn plan(values: Vec<ValuePlan>) -> Arc<AppPlan> {
        Arc::new(AppPlan {
            name: "Example App".to_owned(),
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
            automations: Vec::new(),
        })
    }

    fn job(values: Vec<ValuePlan>) -> Job {
        Job {
            session: SessionId(3),
            plan: plan(values),
            executable: "Example.exe".to_owned(),
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
                Reading::Problem { .. } | Reading::Unseen { .. } => None,
            })
            .collect()
    }

    fn problems(readings: &Receiver<Reading>) -> Vec<String> {
        readings
            .try_iter()
            .filter_map(|reading| match reading {
                Reading::Problem { problem, .. } => Some(problem),
                Reading::Sample { .. } | Reading::Unseen { .. } => None,
            })
            .collect()
    }

    #[test]
    fn the_capture_covers_every_value_and_reads_each_from_its_place() {
        let (events, readings) = mpsc::channel::<Reading>();
        let (control, _control) = mpsc::channel();
        let job = job(vec![red_at("left", 10, 20), red_at("right", 16, 22)]);
        let mut frames = FrameReader::new(&job, WINDOW, events, control).expect("values to read");
        assert_eq!(frames.bounds, Region::new(10, 20, 8, 4).expect("not empty"));
        // Only the left value's square, at the bounds' corner, is red.
        let left_red = frame_image(8, 4, |x, y| x < 2 && y < 2);
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
        let right_red = frame_image(8, 4, |x, y| x >= 6 && y >= 2);
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
    fn a_window_of_another_size_is_not_read_and_says_so_once() {
        let (events, readings) = mpsc::channel::<Reading>();
        let (control, told) = mpsc::channel();
        let job = job(vec![red_at("icon", 0, 0)]);
        let mut frames = FrameReader::new(&job, WINDOW, events, control).expect("values to read");
        let image = frame_image(2, 2, |_, _| true);
        frames.handle(Ok(frame(&image, SMALL)));
        frames.handle(outside(SMALL));
        assert_eq!(
            problems(&readings),
            [
                "icon was drawn when the window was 800x600, and the window is now another size; \
             resize the window or draw the value again"
            ]
        );
        assert_eq!(
            told.try_recv(),
            Err(TryRecvError::Empty),
            "no value fits 640x480"
        );
        frames.handle(Ok(frame(&image, WINDOW)));
        assert_eq!(flags(&readings), [("icon".to_owned(), Value::Flag(true))]);
        frames.handle(outside(SMALL));
        frames.handle(outside(SMALL));
        let unseen: Vec<Reading> = readings.try_iter().collect();
        assert_eq!(
            unseen,
            [
                Reading::Unseen {
                    session: SessionId(3)
                },
                Reading::Problem {
                    session: SessionId(3),
                    problem: "icon was drawn when the window was 800x600, and the window is \
                              now another size; resize the window or draw the value again"
                        .to_owned()
                }
            ],
            "each is said once"
        );
        frames.handle(Ok(frame(&image, WINDOW)));
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
    fn values_drawn_at_another_size_are_read_when_the_window_has_it() {
        let (events, readings) = mpsc::channel::<Reading>();
        let (control, told) = mpsc::channel();
        let job = job(vec![red_at("large", 0, 0), drawn_at("small", 4, 6, SMALL)]);
        let mut frames =
            FrameReader::new(&job, WINDOW, events.clone(), control.clone()).expect("a value");
        assert_eq!(frames.bounds, Region::new(0, 0, 2, 2).expect("not empty"));
        let red = frame_image(2, 2, |_, _| true);
        frames.handle(Ok(frame(&red, WINDOW)));
        assert_eq!(flags(&readings), [("large".to_owned(), Value::Flag(true))]);
        assert_eq!(told.try_recv(), Err(TryRecvError::Empty));
        assert_eq!(frames.handle(outside(SMALL)), Duration::ZERO);
        assert_eq!(told.try_recv(), Ok(Control::Resized(SMALL)));
        let small = FrameReader::new(&job, SMALL, events, control).expect("a value");
        assert_eq!(small.bounds, Region::new(4, 6, 2, 2).expect("not empty"));
    }

    #[test]
    fn a_value_that_cannot_be_read_says_why() {
        let (events, readings) = mpsc::channel::<Reading>();
        let (control, _control) = mpsc::channel();
        let mut value = red_at("count", 0, 0);
        value.check.check = Check::Number {
            reader: DigitReader::new(RED, 10),
            whole: false,
        };
        let job = job(vec![value]);
        let mut frames = FrameReader::new(&job, WINDOW, events, control).expect("values to read");
        let black = frame_image(2, 2, |_, _| false);
        frames.handle(Ok(frame(&black, WINDOW)));
        frames.handle(Ok(frame(&black, WINDOW)));
        assert_eq!(
            problems(&readings),
            ["count cannot be read: the region's number could not be read"]
        );
    }

    #[test]
    fn a_closed_window_tells_the_reader_to_look_again() {
        let (events, _readings) = mpsc::channel::<Reading>();
        let (control, told) = mpsc::channel();
        let icon = job(vec![red_at("icon", 0, 0)]);
        let mut frames = FrameReader::new(&icon, WINDOW, events, control).expect("values to read");
        let closed = io::Error::new(io::ErrorKind::NotConnected, "the captured window closed");
        assert_eq!(frames.handle(Err(closed)), Duration::ZERO);
        assert_eq!(
            told.try_recv(),
            Ok(Control::Lost(
                "its capture ended: the captured window closed".to_owned()
            ))
        );
        let nothing = FrameReader::new(
            &job(Vec::new()),
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
        let mut frames = FrameReader::new(&icon, WINDOW, events, control).expect("values to read");
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
        for ending in [closed(), outside(SMALL)] {
            let mut frames =
                FrameReader::new(&job(values()), WINDOW, events.clone(), control.clone())
                    .expect("values to read");
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
        let mut frames = FrameReader::new(&icon, WINDOW, events, control).expect("values to read");
        let mut recovery = Recovery::new();
        recovery.failed(
            "the capture starts again".to_owned(),
            Some("the copy failed".to_owned()),
        );
        frames.carry(&recovery);
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
            Self::start(false)
        }

        /// A system whose windows' places the test answers.
        fn placing() -> (Arc<Self>, Desk) {
            Self::start(true)
        }

        fn start(placing: bool) -> (Arc<Self>, Desk) {
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
        type Process = NoProcess;

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

        fn ask_memory_access(&self, _: &ProcessEntry, _: &[&str]) -> io::Result<AccessAnswer> {
            Err(io::Error::from(io::ErrorKind::Unsupported))
        }

        fn open(&self, _: &ProcessAccessPermit) -> io::Result<NoProcess> {
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
    /// The desk goes first, so a reader that waits on it cannot hang.
    fn stopped(reader: Reader, desk: Desk, readings: &Receiver<Reading>) {
        drop(desk);
        reader.stop().expect("stop");
        let left: Vec<Reading> = readings.try_iter().collect();
        assert_eq!(left, [], "nothing more was sent");
        assert_eq!(readings.try_recv(), Err(TryRecvError::Disconnected));
    }

    #[test]
    fn the_reader_finds_the_best_window_and_captures_again_after_a_resize_or_a_close() {
        let (system, desk) = FakeSystem::new();
        let (events, readings) = mpsc::channel::<Reading>();
        let values = vec![red_at("icon", 4, 6), drawn_at("small", 2, 2, SMALL)];
        let reader = Reader::start(system, job(values), events).expect("start");
        desk.answer(Ok(Vec::new()));
        assert_eq!(
            readings.recv_timeout(PATIENCE),
            Ok(Reading::Problem {
                session: SessionId(3),
                problem: "no window of Example.exe fits Example App's window yet".to_owned()
            })
        );
        assert!(!desk.asks_soon(), "it waits before looking again");
        desk.answer(Ok(desktop()));
        let (handle, crop, mut first) = desk.capture();
        assert_eq!(handle, 4, "the visible main window of either Example.exe");
        assert_eq!(
            crop,
            Crop::Region(Region::new(4, 6, 2, 2).expect("not empty"))
        );
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
        first(outside(SMALL));
        assert_eq!(
            readings.recv_timeout(PATIENCE),
            Ok(Reading::Unseen {
                session: SessionId(3)
            }),
            "the values it read are out of sight"
        );
        desk.answer(Ok(desktop()));
        let (resized, crop, mut second) = desk.capture();
        assert_eq!(resized, 4);
        assert_eq!(
            crop,
            Crop::Region(Region::new(2, 2, 2, 2).expect("not empty")),
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
                problem: "Example App's window is minimized or hidden".to_owned()
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
        type Process = NoProcess;

        fn processes(&self) -> io::Result<Vec<ProcessEntry>> {
            Ok(vec![ProcessEntry {
                id: ProcessId::new(43),
                parent: ProcessId::new(1),
                executable: "Example.exe".to_owned(),
                threads: 1,
            }])
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

        fn ask_memory_access(&self, _: &ProcessEntry, _: &[&str]) -> io::Result<AccessAnswer> {
            Err(io::Error::from(io::ErrorKind::Unsupported))
        }

        fn open(&self, _: &ProcessAccessPermit) -> io::Result<NoProcess> {
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
            Some("Example App's window is minimized or hidden")
        );
        assert_eq!(check(4, Ok(desktop()), Vec::new()), None, "4 fits best");
        let mut tied = desktop();
        tied.push(window(5, 43, "Main window", true));
        assert_eq!(check(5, Ok(tied), Vec::new()), None, "5 fits as well as 4");
        assert_eq!(
            check(2, Ok(desktop()), Vec::new()).as_deref(),
            Some("another window fits Example App's window better, so it is read instead"),
            "a launcher's window gives way to the main window"
        );
        assert_eq!(
            check(2, Err(io::Error::other("denied")), Vec::new()),
            None,
            "a failing check keeps the capture"
        );
        assert_eq!(
            check(4, Ok(vec![window(1, 7, "Main window", true)]), Vec::new()).as_deref(),
            Some("no window of Example.exe fits Example App's window yet"),
            "every window that fits closed"
        );
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
                problem: "another window fits Example App's window better, so it is read \
                          instead"
                    .to_owned()
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
            Control::Resized(SMALL),
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
