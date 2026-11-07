//! Reading one attached application's values from its window.
//!
//! Each running session has a reader thread. It finds the application's
//! window with the application's
//! [`appytappy_config::window_match::WindowMatch`], among the windows of
//! every running process whose executable fits it, as the setup app does.
//! Then it captures one region of the window: the smallest rectangle that
//! holds the regions of every value drawn at the window's current size, so
//! the capture copies no more pixels than the values need. Windows pushes a
//! frame only when the window's content changes, so a still window costs
//! nothing, and the [`ReadSchedule`] backs off while the values hold still.
//! A value is sent to the owner only when it reads differently from its last
//! read.
//!
//! A value's region means something only at the window size it was drawn
//! at. When the window takes another size at which other values were drawn,
//! the reader captures those instead; a value whose size the window does not
//! have is not read, and the reader says so once.
//!
//! When the window closes, the reader looks for the window again, because
//! many applications replace their window as they run, such as when they
//! switch to full screen. While no window fits, it looks again every
//! [`FIND_AGAIN`].

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
use appytappy_sources::system::{System, best_window};

use crate::plan::AppPlan;

/// How often a reader looks for a window while none fits.
pub(crate) const FIND_AGAIN: Duration = Duration::from_secs(1);

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
    /// The values read so far can no longer be seen: the window closed or
    /// changed size. Rules on them should forget them rather than hold their
    /// last readings.
    Unseen {
        /// The session.
        session: SessionId,
    },
}

/// What reaches the reader thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Control {
    /// The session ended.
    Stop,
    /// The captured window closed.
    Lost,
    /// The window took a size at which other values were drawn, so the
    /// capture starts again for those.
    Resized(Size),
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
        let (control, received) = mpsc::channel();
        let lost = control.clone();
        let thread = thread::Builder::new()
            .name(format!("reader {}", job.session.0))
            .spawn(move || read(system.as_ref(), &job, &received, &lost, &events))?;
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
        crate::join(self.thread)
    }
}

/// Sends `reading`, returning whether the owner still listens.
fn send<E: From<Reading>>(events: &Sender<E>, reading: Reading) -> bool {
    events.send(E::from(reading)).is_ok()
}

/// The reader thread: finds the window, captures it until it closes, takes
/// another size, or the session stops, and looks again after it closes.
fn read<S: System + Capture, E: From<Reading> + Send + 'static>(
    system: &S,
    job: &Job,
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
    let wait = || match control.recv_timeout(FIND_AGAIN) {
        Ok(Control::Lost | Control::Resized(_)) | Err(RecvTimeoutError::Timeout) => true,
        Ok(Control::Stop) | Err(RecvTimeoutError::Disconnected) => false,
    };
    loop {
        let window = match find_window(system, job) {
            Ok(Some(window)) => window,
            Ok(None) => {
                let problem = format!(
                    "no window of {} fits {}'s window yet",
                    job.executable, job.plan.name
                );
                if !report(&mut reported, problem) || !wait() {
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
        let next = control.recv();
        if let Err(error) = session.close() {
            report(&mut reported, format!("its capture ended badly: {error}"));
        }
        match next {
            Ok(Control::Lost) => {},
            Ok(Control::Resized(resized)) => size = resized,
            Ok(Control::Stop) | Err(_) => return,
        }
    }
}

/// The handle of the visible window that fits the app best.
fn find_window<S: System>(system: &S, job: &Job) -> io::Result<Option<u64>> {
    let processes = system.processes()?;
    Ok(best_window(&job.plan.window, processes, system.windows()?))
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
    /// Whether it sent a reading since it last sent [`Reading::Unseen`].
    seen: bool,
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
            seen: false,
            schedule: ReadSchedule::new(ReadInterval::DEFAULT),
            events,
            control,
        })
    }

    /// Reads `frame` and returns how long to wait before the next one.
    fn handle(&mut self, frame: io::Result<Frame<'_>>) -> Duration {
        let (window_size, frame) = match frame {
            Ok(frame) => (frame.window_size, Some(frame)),
            Err(error) if error.kind() == io::ErrorKind::NotConnected => {
                self.unseen();
                self.tell(Control::Lost);
                return Duration::ZERO;
            },
            Err(error) => {
                // The region lies outside a window of this reader's own size
                // only when the saved region does not fit the size it was
                // drawn at, which is a frame problem like any other.
                if let Some(window_size) =
                    OutsideWindow::in_error(&error).filter(|&window_size| window_size != self.size)
                {
                    (window_size, None)
                } else {
                    let problem = format!("a frame cannot be read: {error}");
                    if self.frame_problem.as_ref() != Some(&problem) {
                        self.frame_problem = Some(problem.clone());
                        self.problem(problem);
                    }
                    return self.schedule.after_read(Activity::Steady);
                }
            },
        };
        self.frame_problem = None;
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
        if *last == Some(reading) {
            return false;
        }
        *last = Some(reading);
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

    fn tell(&self, control: Control) {
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
    use appytappy_config::window_match::{MatchPriority, Pattern, WindowMatch};
    use appytappy_sources::screen::check::{Check, ColorMatch};
    use appytappy_sources::screen::digits::DigitReader;
    use appytappy_sources::screen::saved::SavedCheck;
    use appytappy_sources::screen::{FrameHandler, Image};
    use appytappy_sources::system::{
        AccessAnswer, Bounds, NoProcess, ProcessEntry, Rgb, WindowEntry,
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
    const PATIENCE: Duration = Duration::from_secs(10);
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
                priority: MatchPriority::Title,
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
        value.check.check = Check::Number(DigitReader::new(RED, 10));
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
        assert_eq!(told.try_recv(), Ok(Control::Lost));
        let nothing = FrameReader::new(
            &job(Vec::new()),
            WINDOW,
            mpsc::channel::<Reading>().0,
            mpsc::channel().0,
        );
        assert!(nothing.is_none(), "nothing to read");
    }

    /// A capture the fake system started: its window, its crop, and the
    /// handler that receives its frames.
    type Started = (u64, Crop, FrameHandler);

    /// Where the fake system's window list is answered.
    type Answer = SyncSender<io::Result<Vec<WindowEntry>>>;

    /// A system with set processes that asks the test for each window list,
    /// and whose captures hand their handler to the test.
    struct FakeSystem {
        processes: Vec<ProcessEntry>,
        lists: Sender<Answer>,
        captures: Sender<Started>,
    }

    /// The test's side of a [`FakeSystem`].
    struct Desk {
        lists: Receiver<Answer>,
        /// Dropped to make every capture fail.
        started: Option<Receiver<Started>>,
    }

    impl Desk {
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
            let desk = Desk {
                lists: requests,
                started: Some(started),
            };
            let system = Self {
                processes,
                lists,
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

        fn foreground_window(&self) -> io::Result<Option<u64>> {
            Err(io::Error::from(io::ErrorKind::Unsupported))
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
    fn stopped(reader: Reader, readings: &Receiver<Reading>) {
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
        desk.answer(Ok(desktop()));
        let (again, _, _) = desk.capture();
        assert_eq!(again, 4, "it looks for the window again");
        // A real capture drops its handler when it closes.
        drop((first, second));
        stopped(reader, &readings);
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
        stopped(reader, &readings);
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
        stopped(reader, &readings);
    }
}
