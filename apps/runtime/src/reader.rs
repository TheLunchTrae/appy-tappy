//! Reading one attached application's values from its window.
//!
//! Each running session has a reader thread. It finds the application's
//! window among the attached process's windows with the application's
//! [`appytappy_config::window_match::WindowMatch`], then captures one region
//! of it: the smallest rectangle that holds every value's region, so the
//! capture copies no more pixels than the values need. Windows pushes a frame
//! only when the window's content changes, so a still window costs nothing,
//! and the [`ReadSchedule`] backs off while the values hold still. A value is
//! sent to the owner only when it reads differently from its last read.
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

use appytappy_config::anti_cheat::ProcessId;
use appytappy_config::signal::{SignalName, Value};
use appytappy_config::window_match::WindowFacts;
use appytappy_sources::schedule::{Activity, ReadInterval, ReadSchedule};
use appytappy_sources::screen::{Capture, CaptureSession, Crop, Frame, Region};
use appytappy_sources::system::System;

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
    /// The attached process, whose windows are searched.
    pub(crate) process: ProcessId,
    /// The process's executable, such as `Example.exe`.
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
}

/// What reaches the reader thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Control {
    /// The session ended.
    Stop,
    /// The captured window closed.
    Lost,
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
        self.thread.join().map_err(|payload| {
            io::Error::other(format!(
                "a reader thread panicked: {}",
                appytappy_platform::panic_reason(payload.as_ref())
            ))
        })
    }
}

/// Sends `reading`, returning whether the owner still listens.
fn send<E: From<Reading>>(events: &Sender<E>, reading: Reading) -> bool {
    events.send(E::from(reading)).is_ok()
}

/// The reader thread: finds the window, captures it until it closes or the
/// session stops, and looks again after it closes.
fn read<S: System + Capture, E: From<Reading> + Send + 'static>(
    system: &S,
    job: &Job,
    control: &Receiver<Control>,
    lost: &Sender<Control>,
    events: &Sender<E>,
) {
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
        Ok(Control::Lost) | Err(RecvTimeoutError::Timeout) => true,
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
                if !report(
                    &mut reported,
                    format!("its windows cannot be listed: {error}"),
                ) || !wait()
                {
                    return;
                }
                continue;
            },
        };
        let Some(frames) = FrameReader::new(job, events.clone(), lost.clone()) else {
            // A plan always has a value to read; without one, wait to stop.
            #[expect(
                clippy::let_underscore_must_use,
                reason = "whatever arrives, there is nothing to read"
            )]
            let _ = control.recv();
            return;
        };
        let crop = Crop::Region(frames.bounds);
        let throttle = ReadInterval::DEFAULT.min();
        let mut frames = frames;
        let handler = Box::new(move |frame: io::Result<Frame<'_>>| frames.handle(frame));
        let session = match system.start(window, crop, throttle, handler) {
            Ok(session) => session,
            Err(error) => {
                if !report(
                    &mut reported,
                    format!("its window cannot be captured: {error}"),
                ) || !wait()
                {
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
            Ok(Control::Stop) | Err(_) => return,
        }
    }
}

/// The handle of the attached process's window that fits the app best.
fn find_window<S: System>(system: &S, job: &Job) -> io::Result<Option<u64>> {
    let windows: Vec<_> = system
        .windows()?
        .into_iter()
        .filter(|window| window.process == job.process && window.visible)
        .collect();
    let ranked = job
        .plan
        .window
        .rank(windows.iter().map(|window| WindowFacts {
            title: &window.title,
            class: &window.class,
            executable: &job.executable,
        }));
    Ok(ranked
        .first()
        .and_then(|&position| windows.get(position))
        .map(|window| window.handle))
}

/// The capture's frame handler: reads each value from a frame and sends
/// what changed.
struct FrameReader<E> {
    session: SessionId,
    plan: Arc<AppPlan>,
    /// The region the capture copies: the smallest that holds every value.
    bounds: Region,
    /// Each value's region within `bounds`.
    regions: Vec<Region>,
    /// Each value's last reading.
    last: Vec<Option<Value>>,
    /// Each value's last problem.
    problems: Vec<Option<String>>,
    /// The last problem with a frame as a whole.
    frame_problem: Option<String>,
    schedule: ReadSchedule,
    events: Sender<E>,
    lost: Sender<Control>,
}

impl<E: From<Reading>> FrameReader<E> {
    /// The handler for `job`, or `None` when it has no value to read.
    fn new(job: &Job, events: Sender<E>, lost: Sender<Control>) -> Option<Self> {
        let regions: Vec<Region> = job
            .plan
            .values
            .iter()
            .map(|value| value.check.region)
            .collect();
        let left = regions.iter().map(|region| region.x()).min()?;
        let top = regions.iter().map(|region| region.y()).min()?;
        let right = regions.iter().map(|region| region.right()).max()?;
        let bottom = regions.iter().map(|region| region.bottom()).max()?;
        let bounds = Region::new(
            left,
            top,
            right.checked_sub(left)?,
            bottom.checked_sub(top)?,
        )?;
        let regions = regions
            .iter()
            .map(|region| {
                let size = region.size();
                Region::new(
                    region.x().checked_sub(left)?,
                    region.y().checked_sub(top)?,
                    size.width,
                    size.height,
                )
            })
            .collect::<Option<Vec<_>>>()?;
        let count = regions.len();
        Some(Self {
            session: job.session,
            plan: Arc::clone(&job.plan),
            bounds,
            regions,
            last: vec![None; count],
            problems: vec![None; count],
            frame_problem: None,
            schedule: ReadSchedule::new(ReadInterval::DEFAULT),
            events,
            lost,
        })
    }

    /// Reads `frame` and returns how long to wait before the next one.
    fn handle(&mut self, frame: io::Result<Frame<'_>>) -> Duration {
        let frame = match frame {
            Ok(frame) => frame,
            Err(error) if error.kind() == io::ErrorKind::NotConnected => {
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "a failed send means the reader already stopped"
                )]
                let _ = self.lost.send(Control::Lost);
                return Duration::ZERO;
            },
            Err(error) => {
                let problem = format!("a frame cannot be read: {error}");
                if self.frame_problem.as_ref() != Some(&problem) {
                    self.frame_problem = Some(problem.clone());
                    self.problem(problem);
                }
                return self.schedule.after_read(Activity::Steady);
            },
        };
        self.frame_problem = None;
        let mut changed = false;
        for (position, value) in self.plan.values.iter().enumerate() {
            let read = if frame.window_size == value.check.window_size {
                self.regions
                    .get(position)
                    .and_then(|&region| frame.image.crop(region))
                    .ok_or_else(|| format!("{} is outside the captured frame", value.name))
                    .and_then(|image| {
                        value
                            .check
                            .check
                            .read(&image)
                            .map_err(|error| format!("{} cannot be read: {error}", value.name))
                    })
            } else {
                Err(format!(
                    "the window is {} but {} was drawn when it was {}; resize the window or \
                     draw the value again",
                    frame.window_size, value.name, value.check.window_size
                ))
            };
            match read {
                Ok(reading) => {
                    if let Some(slot) = self.problems.get_mut(position) {
                        *slot = None;
                    }
                    let Some(last) = self.last.get_mut(position) else {
                        continue;
                    };
                    if *last != Some(reading) {
                        *last = Some(reading);
                        changed = true;
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
                    }
                },
                Err(problem) => {
                    let Some(slot) = self.problems.get_mut(position) else {
                        continue;
                    };
                    if slot.as_ref() != Some(&problem) {
                        *slot = Some(problem.clone());
                        self.problem(problem);
                    }
                },
            }
        }
        self.schedule.after_read(if changed {
            Activity::Changed
        } else {
            Activity::Steady
        })
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

#[cfg(test)]
mod tests {
    use std::sync::mpsc::SyncSender;

    use appytappy_config::anti_cheat::ProcessAccessPermit;
    use appytappy_config::rule::Rule;
    use appytappy_config::window_match::{MatchPriority, Pattern, WindowMatch};
    use appytappy_sources::screen::check::{Check, ColorMatch};
    use appytappy_sources::screen::saved::SavedCheck;
    use appytappy_sources::screen::{FrameHandler, Image, Size};
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

    fn red_at(name: &str, x: u32, y: u32) -> ValuePlan {
        ValuePlan {
            signal: SignalName::new(name).expect("valid"),
            name: name.to_owned(),
            check: SavedCheck {
                window_size: WINDOW,
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
            process: ProcessId::new(42),
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

    fn flags(readings: &Receiver<Reading>) -> Vec<(String, Value)> {
        readings
            .try_iter()
            .filter_map(|reading| match reading {
                Reading::Sample { signal, value, .. } => Some((signal.to_string(), value)),
                Reading::Problem { .. } => None,
            })
            .collect()
    }

    fn problems(readings: &Receiver<Reading>) -> Vec<String> {
        readings
            .try_iter()
            .filter_map(|reading| match reading {
                Reading::Problem { problem, .. } => Some(problem),
                Reading::Sample { .. } => None,
            })
            .collect()
    }

    #[test]
    fn the_capture_covers_every_value_and_reads_each_from_its_place() {
        let (events, readings) = mpsc::channel::<Reading>();
        let (lost, _lost) = mpsc::channel();
        let job = job(vec![red_at("left", 10, 20), red_at("right", 16, 22)]);
        let mut frames = FrameReader::new(&job, events, lost).expect("values to read");
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
        let (lost, _lost) = mpsc::channel();
        let job = job(vec![red_at("icon", 0, 0)]);
        let mut frames = FrameReader::new(&job, events, lost).expect("values to read");
        let image = frame_image(2, 2, |_, _| true);
        let small = Size {
            width: 640,
            height: 480,
        };
        frames.handle(Ok(frame(&image, small)));
        frames.handle(Ok(frame(&image, small)));
        assert_eq!(
            problems(&readings),
            [
                "the window is 640x480 but icon was drawn when it was 800x600; resize the window \
              or draw the value again"
            ]
        );
        frames.handle(Ok(frame(&image, WINDOW)));
        assert_eq!(flags(&readings), [("icon".to_owned(), Value::Flag(true))]);
        frames.handle(Err(io::Error::other("the copy failed")));
        frames.handle(Err(io::Error::other("the copy failed")));
        assert_eq!(
            problems(&readings),
            ["a frame cannot be read: the copy failed"]
        );
    }

    #[test]
    fn a_value_that_cannot_be_read_says_why() {
        let (events, readings) = mpsc::channel::<Reading>();
        let (lost, _lost) = mpsc::channel();
        let mut value = red_at("count", 0, 0);
        value.check.check =
            Check::Number(appytappy_sources::screen::digits::DigitReader::new(RED, 10));
        let job = job(vec![value]);
        let mut frames = FrameReader::new(&job, events, lost).expect("values to read");
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
        let (lost, told) = mpsc::channel();
        let job = job(vec![red_at("icon", 0, 0)]);
        let mut frames = FrameReader::new(&job, events, lost).expect("values to read");
        let closed = io::Error::new(io::ErrorKind::NotConnected, "the captured window closed");
        assert_eq!(frames.handle(Err(closed)), Duration::ZERO);
        assert_eq!(told.try_recv(), Ok(Control::Lost));
        assert!(
            FrameReader::new(
                &job_without_values(),
                mpsc::channel::<Reading>().0,
                mpsc::channel().0
            )
            .is_none(),
            "nothing to read"
        );
    }

    fn job_without_values() -> Job {
        job(Vec::new())
    }

    /// A capture the fake system started: its window, its crop, and the
    /// handler that receives its frames.
    type Started = (u64, Crop, FrameHandler);

    /// Where the fake system's window list is answered.
    type Answer = SyncSender<io::Result<Vec<WindowEntry>>>;

    /// A system that asks the test for each window list, and whose captures
    /// hand their handler to the test.
    struct FakeSystem {
        lists: Sender<Answer>,
        captures: Sender<Started>,
    }

    /// The test's side of a [`FakeSystem`].
    struct Desk {
        lists: Receiver<Answer>,
        started: Receiver<Started>,
    }

    impl Desk {
        /// Answers the reader's next request for the window list.
        fn answer(&self, windows: io::Result<Vec<WindowEntry>>) {
            let answer = self.lists.recv_timeout(PATIENCE).expect("a request");
            answer.send(windows).expect("the reader waits");
        }

        /// The reader's next capture.
        fn capture(&self) -> Started {
            self.started.recv_timeout(PATIENCE).expect("a capture")
        }
    }

    impl FakeSystem {
        fn new() -> (Arc<Self>, Desk) {
            let (lists, requests) = mpsc::channel();
            let (captures, started) = mpsc::channel();
            let desk = Desk {
                lists: requests,
                started,
            };
            (Arc::new(Self { lists, captures }), desk)
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
            Ok(Vec::new())
        }

        fn windows(&self) -> io::Result<Vec<WindowEntry>> {
            let (answer, answered) = mpsc::sync_channel(1);
            let gone = || io::Error::from(io::ErrorKind::BrokenPipe);
            self.lists.send(answer).map_err(|_gone| gone())?;
            answered.recv().map_err(|_gone| gone())?
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
                .map_err(|_unheard| io::Error::from(io::ErrorKind::BrokenPipe))?;
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

    const PATIENCE: Duration = Duration::from_secs(10);

    #[test]
    fn the_reader_finds_the_best_window_captures_it_and_looks_again_after_it_closes() {
        let desktop = vec![
            window(1, 7, "Main window", true),
            window(2, 42, "Settings", true),
            window(3, 42, "Main window", false),
            window(4, 42, "Main window", true),
        ];
        let (system, desk) = FakeSystem::new();
        let (events, readings) = mpsc::channel::<Reading>();
        let reader = Reader::start(system, job(vec![red_at("icon", 4, 6)]), events).expect("start");
        desk.answer(Ok(Vec::new()));
        let first = readings.recv_timeout(PATIENCE).expect("a problem");
        assert_eq!(
            first,
            Reading::Problem {
                session: SessionId(3),
                problem: "no window of Example.exe fits Example App's window yet".to_owned()
            }
        );
        desk.answer(Ok(desktop.clone()));
        let (handle, crop, mut handler) = desk.capture();
        assert_eq!(handle, 4, "the visible main window of process 42");
        assert_eq!(
            crop,
            Crop::Region(Region::new(4, 6, 2, 2).expect("not empty"))
        );
        let red = frame_image(2, 2, |_, _| true);
        handler(Ok(frame(&red, WINDOW)));
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
        handler(Err(io::Error::new(io::ErrorKind::NotConnected, "closed")));
        desk.answer(Ok(desktop));
        let (again, _, _) = desk.capture();
        assert_eq!(again, 4, "it looks for the window again");
        reader.stop().expect("stop");
    }

    #[test]
    fn a_reader_that_cannot_list_windows_says_so_and_stops_on_request() {
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
        reader.stop().expect("stop");
        assert!(
            readings.try_iter().next().is_none(),
            "the problem was sent once"
        );
    }
}
