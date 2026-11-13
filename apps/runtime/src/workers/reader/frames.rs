//! The capture's frame handler: reads each value from a frame and sends
//! what changed.

use std::fmt;
use std::io;
use std::panic::{self, AssertUnwindSafe};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use appytappy_config::settings_file::TraceCategory;
use appytappy_config::signal::Value;
use appytappy_sources::schedule::{Activity, ReadSchedule};
use appytappy_sources::screen::check::CheckError;
use appytappy_sources::screen::saved::Unread;
use appytappy_sources::screen::{Frame, FrameMeaning, Placed, Size};

use super::{Control, Job, RESTART_AFTER, Reading, Recovery};
use crate::model::{ReaderNumber, SessionId};
use crate::plan::AppPlan;
use crate::trace::{Record, Trace, What};

/// The wait the handler asks for once it told the reader that its capture
/// ended. The reader closes the capture, and no frame it hands over until
/// then is read, so the wait only keeps the capture from handing them over
/// at once.
const AFTER_END: Duration = Duration::from_secs(1);

/// The problems said for each value since it was last read, in the order of
/// its application's values: why it is not read at the window's size, or
/// the way its check failed, as [`unreadable`] words it.
pub(super) type Problems = Vec<Vec<String>>;

/// Why a value could not be read from a frame.
#[derive(Debug)]
enum Unreadable {
    /// It does not read at the window's size.
    NotRead(Unread),
    /// Its check failed on the region's pixels.
    Check {
        /// Why.
        error: CheckError,
        /// Whether the region is one the value was drawn at, at this window
        /// size, rather than scaled to it.
        drawn: bool,
    },
}

/// That a value's check failed and why, as a sentence fragment that
/// follows the value's name: `why` is the check's error, or the way it
/// failed alone, which stays the same while what the region shows changes.
/// A region that is not `drawn` at the window's size is scaled to it and
/// can lie beside the value, such as when the application lays its
/// interface out another way at that size, so the fragment says what to do
/// then.
fn unreadable(why: &dyn fmt::Display, drawn: bool) -> String {
    if drawn {
        format!("cannot be read: {why}")
    } else {
        format!(
            "cannot be read where its region scales to at the window's size now: {why}; if \
             the region is off the value there, fix the value at this size in AppyTappy Setup"
        )
    }
}

/// `problem` when it is not among those `said`, which it then joins. A
/// value has a handful of problems at most: one for each reason it is not
/// read at a size, and one for each way its check fails.
pub(super) fn newly(said: &mut Vec<String>, problem: String) -> Option<String> {
    (!said.contains(&problem)).then(|| {
        said.push(problem.clone());
        problem
    })
}

/// Why a value is not read at the window's size, as a sentence fragment
/// that follows the value's name. It leaves the size out, so the log says
/// it once for each reason, not once for each size a window passes.
pub(super) fn not_read(unread: Unread) -> String {
    let advice = match unread {
        Unread::NotDrawn => "resize the window, or fix the value at this size in AppyTappy Setup",
        Unread::Outside | Unread::TooSmall => "fix the value at this size in AppyTappy Setup",
    };
    format!("is not read at the window's size now: {unread}; {advice}")
}

/// How far a capture that the reader started again after the last one
/// ended has got toward telling the reader that it reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Restart {
    /// The capture was not started again, or it told the reader it reads.
    Done,
    /// No frame has come yet.
    Waiting,
    /// A frame came, and once it is handled without a panic, the handler
    /// sends [`Control::Recovered`].
    Read,
}

/// The capture's frame handler: reads each value that reads at one window
/// size from a frame and sends what changed.
pub(super) struct FrameReader<E> {
    session: SessionId,
    reader: ReaderNumber,
    plan: Arc<AppPlan>,
    /// Where each value reads at the window size it reads.
    placed: Placed,
    /// The other size it last told the reader frames show, until frames
    /// show the size it reads at again.
    told: Option<Size>,
    /// Each value's last reading.
    last: Vec<Option<Value>>,
    /// The problems said for each value since it was last read. They are
    /// handed to the reader when frames leave the size it reads at, since
    /// the reader says why values are not read at another size, and taken
    /// back when they show it again.
    problems: Problems,
    /// Where the reader gives `problems` back.
    heard: Receiver<Problems>,
    /// The last problem with a frame as a whole.
    frame_problem: Option<String>,
    /// How many frames in a row failed.
    failures: u32,
    /// Whether the reader started this capture again after the last one
    /// ended, and whether it has heard that this one reads.
    restart: Restart,
    /// Whether it sent a reading since it last sent [`Reading::Unseen`].
    seen: bool,
    /// Whether it told the reader that its capture ended, or that reading a
    /// frame panicked. The capture can still hand it frames until the
    /// reader closes it, and it reads none of them, so nothing it sends
    /// follows the reader's `Unseen`.
    ended: bool,
    schedule: ReadSchedule,
    /// Where it records each reading and change, when the trace is on.
    trace: Trace,
    /// Where the reader gives it a new `trace`.
    traces: Receiver<Trace>,
    events: Sender<E>,
    control: Sender<Control>,
}

impl<E: From<Reading>> FrameReader<E> {
    /// The handler for the values of `job` as `placed` places them. `heard`
    /// gives back each value's last problem as the reader said it while
    /// frames showed another size, and `traces` gives the trace to record
    /// to, before the first frame and whenever it changes.
    pub(super) fn new(
        job: &Job,
        placed: Placed,
        events: Sender<E>,
        control: Sender<Control>,
        heard: Receiver<Problems>,
        traces: Receiver<Trace>,
    ) -> Self {
        let count = job.plan.runs.values.len();
        Self {
            session: job.session,
            reader: job.reader,
            plan: Arc::clone(&job.plan),
            placed,
            told: None,
            last: vec![None; count],
            problems: vec![Vec::new(); count],
            heard,
            frame_problem: None,
            failures: 0,
            restart: Restart::Done,
            seen: false,
            ended: false,
            schedule: ReadSchedule::new(job.interval),
            trace: Trace::OFF,
            traces,
            events,
            control,
        }
    }

    /// Acts on what `frame` means and returns how long to wait before the
    /// next one. It runs on the capture's thread, so a panic while it acts
    /// is caught here and told to the reader with [`Control::Panicked`]:
    /// unwound, it would end that thread, and the reader would hear of it
    /// only once it closed the capture.
    pub(super) fn handle(&mut self, frame: io::Result<Frame<'_>>) -> Duration {
        if self.ended {
            return AFTER_END;
        }
        // Once it panicked, it reads no more frames. What it still reads of
        // its state is what a panic cannot leave half changed: a value
        // counts as seen once its reading was sent.
        match panic::catch_unwind(AssertUnwindSafe(|| self.act(frame))) {
            Ok(wait) => {
                // Only a frame handled without a panic shows that a capture
                // started again reads.
                if self.restart == Restart::Read {
                    self.restart = Restart::Done;
                    self.tell(Control::Recovered);
                }
                wait
            },
            Err(payload) => {
                let reason = appytappy_platform::panic_reason(payload.as_ref()).to_owned();
                self.unseen();
                self.tell(Control::Panicked(reason));
                AFTER_END
            },
        }
    }

    /// Acts on what `frame` means, as [`FrameReader::handle`] says.
    fn act(&mut self, frame: io::Result<Frame<'_>>) -> Duration {
        if let Some(trace) = self.traces.try_iter().last() {
            self.trace = trace;
        }
        match self.placed.meaning(frame) {
            Ok(FrameMeaning::Readable(frame)) => {
                self.captured();
                self.read_all(&frame)
            },
            Ok(FrameMeaning::Resized(size)) => {
                self.captured();
                self.resizing(size);
                Duration::ZERO
            },
            // A new capture would fail the same way, so the frame does not
            // count toward starting one.
            Ok(FrameMeaning::Unreadable(outside)) => self.frame_failed(&outside.to_string()),
            Err(error) if error.kind() == io::ErrorKind::NotConnected => {
                self.unseen();
                self.tell(Control::Lost(format!("its capture ended: {error}")));
                Duration::ZERO
            },
            Err(error) => {
                let wait = self.frame_failed(&error.to_string());
                self.failures = self.failures.saturating_add(1);
                if self.failures < RESTART_AFTER {
                    return wait;
                }
                self.failures = 0;
                self.unseen();
                self.tell(Control::Failing(error.to_string()));
                Duration::ZERO
            },
        }
    }

    /// Notes that the capture handed over a frame: no frame problem
    /// stands, and a capture started again has recovered once the frame is
    /// handled.
    fn captured(&mut self) {
        self.frame_problem = None;
        self.failures = 0;
        if self.restart == Restart::Waiting {
            self.restart = Restart::Read;
        }
    }

    /// Reads nothing while frames show the window at `size`, another than
    /// the capture reads at, and tells the reader each such size once. The
    /// reader acts on it once it settled, and says why values are not read
    /// there.
    fn resizing(&mut self, size: Size) {
        self.unseen();
        if self.told != Some(size) {
            // Leaving its size, it hands the reader its problems.
            let problems = self.told.is_none().then(|| {
                self.heard.try_iter().for_each(drop);
                self.problems.clone()
            });
            self.told = Some(size);
            self.tell(Control::Resizing { size, problems });
        }
    }

    /// Reads each value from `frame`, which shows the window at the size
    /// the capture reads at, and returns how long to wait before the next
    /// frame.
    fn read_all(&mut self, frame: &Frame<'_>) -> Duration {
        if self.told.take().is_some() {
            if let Some(problems) = self.heard.try_iter().last()
                && problems.len() == self.problems.len()
            {
                self.problems = problems;
            }
            self.tell(Control::Resizing {
                size: self.placed.size(),
                problems: None,
            });
        }
        let mut changed = false;
        // The clock is read only when each reading is traced.
        let timed = self.trace.wants(TraceCategory::Readings);
        let session = self.session;
        for position in 0..self.plan.runs.values.len() {
            let started = timed.then(Instant::now);
            let read = self.read(position, frame);
            if let (Some(started), Some(Ok(value))) = (started, &read) {
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
                Some(Ok(reading)) => changed |= self.record(position, reading, frame),
                Some(Err(problem)) => self.report(position, &problem),
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
    pub(super) fn carry(&mut self, recovery: &Recovery, problems: Problems) {
        self.restart = if recovery.problem.is_some() {
            Restart::Waiting
        } else {
            Restart::Done
        };
        self.frame_problem = recovery.frame_error.as_deref().map(frame_problem);
        if problems.len() == self.problems.len() {
            self.problems = problems;
        }
    }

    /// Says once that frames cannot be read and why, until another problem
    /// or a good frame replaces it, and returns the wait before the next
    /// frame.
    fn frame_failed(&mut self, error: &str) -> Duration {
        let problem = frame_problem(error);
        if self.frame_problem.as_ref() != Some(&problem) {
            self.frame_problem = Some(problem.clone());
            self.problem(problem);
        }
        self.schedule.after_read(Activity::Steady)
    }

    /// Reads the value at `position` from `frame`, or says why it cannot
    /// be read.
    fn read(&self, position: usize, frame: &Frame<'_>) -> Option<Result<Value, Unreadable>> {
        let value = self.plan.runs.values.get(position)?;
        let at = match self.placed.values().get(position)? {
            Ok(at) => at,
            Err(unread) => return Some(Err(Unreadable::NotRead(*unread))),
        };
        let image = self.placed.cropped(position, frame.image)?;
        Some(value.check.check(at).read(&image).map_err(|error| {
            Unreadable::Check {
                error,
                drawn: value
                    .check
                    .drawn_sizes()
                    .any(|size| size == self.placed.size()),
            }
        }))
    }

    /// Keeps a good reading and sends it when it changed; returns whether it
    /// did.
    fn record(&mut self, position: usize, reading: Value, frame: &Frame<'_>) -> bool {
        if let Some(said) = self.problems.get_mut(position) {
            said.clear();
        }
        let Some(last) = self.last.get_mut(position) else {
            return false;
        };
        if last.as_ref() == Some(&reading) {
            return false;
        }
        *last = Some(reading.clone());
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
            reader: self.reader,
            position,
            value: reading,
            at: frame.received,
        }));
        // Only once it was sent, so a panic that kept it from going leaves
        // nothing to say `Unseen` for.
        self.seen = true;
        true
    }

    /// Sends a value's problem unless it was sent since the value was last
    /// read. A check's problem is the way it fails, not what the region
    /// shows, so a number that stays unreadable while it counts is said
    /// once, and so is each of two problems a region flickers between.
    fn report(&mut self, position: usize, problem: &Unreadable) {
        let Some(said) = self.problems.get_mut(position) else {
            return;
        };
        let (kind, check) = match problem {
            Unreadable::NotRead(unread) => (not_read(*unread), None),
            Unreadable::Check { error, drawn } => {
                (unreadable(&error.stage(), *drawn), Some((error, *drawn)))
            },
        };
        if let Some(kind) = newly(said, kind) {
            let problem = check.map_or(kind, |(error, drawn)| unreadable(error, drawn));
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
            self.last.fill(None);
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a failed send means the owner stopped this session"
            )]
            let _ = self.events.send(E::from(Reading::Unseen {
                session: self.session,
                reader: self.reader,
            }));
            self.seen = false;
        }
    }

    /// Tells the reader `control`; once that ends the capture, it reads no
    /// more frames.
    fn tell(&mut self, control: Control) {
        if matches!(
            control,
            Control::Lost(_) | Control::Failing(_) | Control::Panicked(_)
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

#[cfg(test)]
mod tests {
    use std::sync::mpsc::TryRecvError;

    use appytappy_config::app_file::SizeChangesDef;
    use appytappy_config::signal::Number;
    use appytappy_sources::screen::check::Check;
    use appytappy_sources::screen::digits::DigitReader;
    use appytappy_sources::screen::saved::SavedFix;
    use appytappy_sources::screen::settle::SETTLE;
    use appytappy_sources::screen::{Placement, Region};

    use super::*;

    use super::super::examples::*;
    use super::super::*;

    /// The frame handler for `job`'s values placed at `size`.
    fn frames_at(
        job: &Job,
        size: Size,
        events: Sender<Reading>,
        control: Sender<Control>,
    ) -> FrameReader<Reading> {
        frames_with(job, size, events, control).0
    }

    /// The frame handler for `job`'s values placed at `size`, and where the
    /// reader gives it back each value's last problem.
    fn frames_with(
        job: &Job,
        size: Size,
        events: Sender<Reading>,
        control: Sender<Control>,
    ) -> (FrameReader<Reading>, Sender<Problems>) {
        let (said, heard) = mpsc::channel();
        let placed = place(&job.plan, size);
        let frames = FrameReader::new(job, placed, events, control, heard, mpsc::channel().1);
        (frames, said)
    }

    /// The frame handler for `job`'s values placed at [`WINDOW`], and where
    /// the reader gives it a new trace.
    fn traced(job: &Job, events: Sender<Reading>) -> (FrameReader<Reading>, Sender<Trace>) {
        let (retrace, traces) = mpsc::channel();
        let placed = place(&job.plan, WINDOW);
        let frames = FrameReader::new(
            job,
            placed,
            events,
            mpsc::channel().0,
            mpsc::channel().1,
            traces,
        );
        (frames, retrace)
    }

    /// Each value sent, by its position.
    fn flags(readings: &Receiver<Reading>) -> Vec<(usize, Value)> {
        readings
            .try_iter()
            .filter_map(|reading| match reading {
                Reading::Sample {
                    position, value, ..
                } => Some((position, value)),
                Reading::Problem { .. }
                | Reading::ValueProblem { .. }
                | Reading::Unseen { .. }
                | Reading::Ended { .. } => None,
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
                Reading::Sample { .. } | Reading::Unseen { .. } | Reading::Ended { .. } => None,
            })
            .collect()
    }

    #[test]
    fn the_capture_packs_every_value_and_reads_each_from_its_place() {
        let (events, readings) = mpsc::channel::<Reading>();
        let (control, _control) = mpsc::channel();
        let job = job(vec![red_at("left", 10, 20), red_at("right", 16, 22)]);
        let mut frames = frames_at(&job, WINDOW, events, control);
        let region = |x, y| Region::new(x, y, 2, 2).expect("not empty");
        assert_eq!(
            frames.placed.layout().expect("values to read").copies(),
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
            [(0, Value::Flag(true)), (1, Value::Flag(false))]
        );
        let steady = frames.handle(Ok(frame(&left_red, WINDOW)));
        assert!(steady > wait, "a steady read backs off");
        assert_eq!(flags(&readings), [], "nothing changed, so nothing is sent");
        let right_red = frame_image(2, 4, |_, y| y >= 2);
        frames.handle(Ok(frame(&right_red, WINDOW)));
        assert_eq!(
            flags(&readings),
            [(0, Value::Flag(false)), (1, Value::Flag(true))]
        );
    }

    #[test]
    fn each_read_and_each_change_is_traced_with_its_value_and_its_frame() {
        let (events, _readings) = mpsc::channel::<Reading>();
        let (trace, entries) =
            Trace::for_test(1, &[TraceCategory::Readings, TraceCategory::Changes]);
        let job = job(vec![red_at("left", 10, 20), red_at("right", 16, 22)]);
        let (mut frames, retrace) = traced(&job, events);
        retrace.send(trace).expect("the handler listens");
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
        retrace.send(Trace::OFF).expect("the handler listens");
        frames.handle(Ok(frame(&frame_image(8, 4, |_, _| false), WINDOW)));
        assert_eq!(
            crate::trace::sent(&entries),
            [],
            "the handler records to the trace it was given last"
        );
    }

    #[test]
    fn a_window_of_another_size_is_read_only_once_it_settled_and_says_why_once() {
        let (events, readings) = mpsc::channel::<Reading>();
        let (control, told) = mpsc::channel();
        let job = job(vec![red_at("icon", 0, 0)]);
        let (mut frames, said) = frames_with(&job, WINDOW, events.clone(), control);
        let image = frame_image(2, 2, |_, _| true);
        frames.handle(Ok(frame(&image, WINDOW)));
        assert_eq!(flags(&readings), [(0, Value::Flag(true))]);
        assert_eq!(frames.handle(Ok(frame(&image, SMALL))), Duration::ZERO);
        frames.handle(outside(SMALL));
        frames.handle(Ok(frame(&image, SMALL)));
        assert_eq!(
            told.try_iter().collect::<Vec<_>>(),
            [Control::Resizing {
                size: SMALL,
                problems: Some(vec![Vec::new()])
            }],
            "the reader is told once, and gets the problems"
        );
        assert_eq!(
            readings.try_iter().collect::<Vec<_>>(),
            [Reading::Unseen {
                session: SessionId(3),
                reader: ReaderNumber(2),
            }],
            "nothing is read before the size settles"
        );
        // The reader found that nothing reads at the settled size and says
        // why, once, with no frame needed.
        let small = place(&job.plan, SMALL);
        let mut problems_now = vec![Vec::new()];
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
            [(0, Value::Flag(true))],
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
        let (mut frames, said) = frames_with(&job, WINDOW, events, control);
        let red = frame_image(2, 2, |_, _| true);
        frames.handle(Ok(frame(&red, WINDOW)));
        assert_eq!(problems(&readings), [format!("value 1 {NOT_DRAWN}")]);
        // A give-back left from an earlier size is dropped when it leaves.
        said.send(vec![Vec::new(), Vec::new()])
            .expect("the handler listens");
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
                    problems: Some(vec![Vec::new(), vec![NOT_DRAWN.to_owned()]])
                },
                Control::Resizing {
                    size: other,
                    problems: None
                }
            ],
            "handed once"
        );
        // The reader said something else about the value meanwhile. Back at
        // its size the value keeps both as said, so its own reason is not
        // said again.
        let both = vec![NOT_DRAWN.to_owned(), OUTSIDE.to_owned()];
        said.send(vec![Vec::new(), both.clone()])
            .expect("the handler listens");
        frames.handle(Ok(frame(&red, WINDOW)));
        assert_eq!(problems(&readings), Vec::<String>::new());
        assert_eq!(frames.problems, [Vec::new(), both]);
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
            Some(place(&job.plan, SMALL))
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
        let mut frames = frames_at(&job, WINDOW, events.clone(), control.clone());
        let copied = |frames: &FrameReader<Reading>| {
            frames
                .placed
                .layout()
                .expect("a value to read")
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
        assert_eq!(flags(&readings), [(0, Value::Flag(true))]);
        assert_eq!(told.try_recv(), Err(TryRecvError::Empty));
        assert_eq!(frames.handle(outside(SMALL)), Duration::ZERO);
        assert_eq!(
            told.try_recv(),
            Ok(Control::Resizing {
                size: SMALL,
                problems: Some(vec![Vec::new(), vec![NOT_DRAWN.to_owned()]])
            })
        );
        readings.try_iter().for_each(drop);
        // Once SMALL settles, the capture for it carries the problems, so
        // the value drawn only at the old size says nothing new.
        let mut small = frames_at(&job, SMALL, events, control);
        assert_eq!(
            copied(&small),
            [Region::new(4, 6, 2, 2).expect("not empty")]
        );
        small.carry(
            &Recovery::new(),
            vec![vec![NOT_DRAWN.to_owned()], Vec::new()],
        );
        small.handle(Ok(frame(&red, SMALL)));
        assert_eq!(problems(&readings), Vec::<String>::new());
        let mut fresh = frames_at(&job, SMALL, mpsc::channel().0, mpsc::channel().0);
        fresh.carry(&Recovery::new(), vec![Vec::new()]);
        assert_eq!(
            fresh.problems,
            [Vec::<String>::new(), Vec::new()],
            "a list for other values is left out"
        );
        let (said, heard) = mpsc::channel::<Reading>();
        let mut uncarried = frames_at(&job, SMALL, said, mpsc::channel().0);
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
        let mut frames = frames_at(&job, WINDOW, events.clone(), control.clone());
        let red = frame_image(16, 16, |_, _| true);
        frames.handle(Ok(frame(&red, SMALL)));
        let placed = place(&job.plan, SMALL);
        assert!(placed.layout().is_some());
        assert!(matches!(
            told.try_recv(),
            Ok(Control::Resizing { size: SMALL, .. })
        ));
        // Its edges at 4 and 6 across scale to 3.2 and 4.8, so 3 to 5, and
        // at 6 and 8 down to 4.8 and 6.4, so 5 to 6: one row. The fix is
        // read exactly where it was drawn.
        let small = FrameReader::new(
            &job,
            placed,
            events,
            control,
            mpsc::channel().1,
            mpsc::channel().1,
        );
        assert_eq!(
            small
                .placed
                .layout()
                .expect("values to read")
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
        let mut problems_now = vec![Vec::new()];
        let narrow = Size {
            width: 400,
            height: 600,
        };
        let placed = place(&job.plan, narrow);
        assert!(placed.layout().is_none());
        say_unread(&placed, &mut problems_now, job.session, &events);
        assert_eq!(problems(&readings), [format!("value 0 {OUTSIDE}")]);
        // Narrower still, it is not read for the same reason, and the log
        // says nothing new.
        let narrower = Size {
            width: 300,
            height: 600,
        };
        say_unread(
            &place(&job.plan, narrower),
            &mut problems_now,
            job.session,
            &events,
        );
        assert_eq!(problems(&readings), Vec::<String>::new(), "said once");
        assert_eq!(problems_now, [vec![OUTSIDE.to_owned()]]);
        // A value that reads says nothing.
        say_unread(
            &place(&plan(vec![red_at("icon", 0, 0)]), WINDOW),
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
        let mut frames = frames_at(&job, WINDOW, events, control);
        let black = frame_image(2, 2, |_, _| false);
        frames.handle(Ok(frame(&black, WINDOW)));
        frames.handle(Ok(frame(&black, WINDOW)));
        assert_eq!(
            problems(&readings),
            ["value 0 cannot be read: no pixel in the region has the characters' color"]
        );
    }

    #[test]
    fn a_value_unreadable_where_its_region_scales_to_says_to_fix_it_there() {
        let (events, readings) = mpsc::channel::<Reading>();
        let (control, _control) = mpsc::channel();
        let mut value = scaling("count", 4, 6, WINDOW, SizeChangesDef::Often);
        value.check.drawn.check = Check::Number {
            reader: DigitReader::new(RED, 10),
            whole: false,
        };
        let job = job(vec![value]);
        let mut frames = frames_at(&job, SMALL, events, control);
        let black = frame_image(16, 16, |_, _| false);
        frames.handle(Ok(frame(&black, SMALL)));
        assert_eq!(
            problems(&readings),
            [
                "value 0 cannot be read where its region scales to at the window's size now: \
                 no pixel in the region has the characters' color; if the region is off the \
                 value there, fix the value at this size in AppyTappy Setup"
            ]
        );
    }

    #[test]
    fn a_number_that_stays_unreadable_one_way_is_said_once_with_what_it_first_read() {
        let (events, readings) = mpsc::channel::<Reading>();
        let (control, _control) = mpsc::channel();
        let mut value = red_at("count", 0, 0);
        value.check.drawn.region = Region::new(0, 0, 3, 2).expect("not empty");
        value.check.drawn.check = Check::Number {
            reader: DigitReader::new(RED, 10),
            whole: false,
        };
        let job = job(vec![value]);
        let mut frames = frames_at(&job, WINDOW, events, control);
        // One character, then two, of a reference that learned none.
        let one = frame_image(3, 2, |x, _| x == 0);
        let two = frame_image(3, 2, |x, _| x != 1);
        let none = frame_image(3, 2, |_, _| false);
        frames.handle(Ok(frame(&one, WINDOW)));
        frames.handle(Ok(frame(&two, WINDOW)));
        assert_eq!(
            problems(&readings),
            [
                "value 0 cannot be read: a character has a shape the reference has not \
                 learned: it read `?`, and character 1 is unlike any, since it has learned \
                 none"
            ]
        );
        // Another way to fail is another problem, said once however the
        // region flickers between the two.
        for _ in 0..2 {
            frames.handle(Ok(frame(&none, WINDOW)));
            frames.handle(Ok(frame(&two, WINDOW)));
        }
        assert_eq!(
            problems(&readings),
            ["value 0 cannot be read: no pixel in the region has the characters' color"]
        );
    }

    #[test]
    fn a_value_says_a_problem_again_once_it_was_read_in_between() {
        let (events, readings) = mpsc::channel::<Reading>();
        let (control, _control) = mpsc::channel();
        let mut value = red_at("count", 0, 0);
        value.check.drawn.region = Region::new(0, 0, 2, 1).expect("not empty");
        // A reader that knows a red pixel as a 1.
        let mut reader = DigitReader::new(RED, 10);
        reader
            .learn(&frame_image(1, 1, |_, _| true), "1")
            .expect("one character");
        value.check.drawn.check = Check::Number {
            reader,
            whole: false,
        };
        let job = job(vec![value]);
        let mut frames = frames_at(&job, WINDOW, events, control);
        let none = frame_image(2, 1, |_, _| false);
        let one = frame_image(2, 1, |x, _| x == 0);
        let no_ink = "value 0 cannot be read: no pixel in the region has the characters' color";
        frames.handle(Ok(frame(&none, WINDOW)));
        frames.handle(Ok(frame(&none, WINDOW)));
        assert_eq!(problems(&readings), [no_ink]);
        frames.handle(Ok(frame(&one, WINDOW)));
        assert_eq!(
            flags(&readings),
            [(0, Value::Number(Number::new(1.0).expect("finite")))]
        );
        frames.handle(Ok(frame(&none, WINDOW)));
        assert_eq!(problems(&readings), [no_ink]);
    }

    #[test]
    fn a_closed_window_tells_the_reader_to_look_again() {
        let (events, _readings) = mpsc::channel::<Reading>();
        let (control, told) = mpsc::channel();
        let icon = job(vec![red_at("icon", 0, 0)]);
        let mut frames = frames_at(&icon, WINDOW, events, control);
        let closed = io::Error::new(io::ErrorKind::NotConnected, "the captured window closed");
        assert_eq!(frames.handle(Err(closed)), Duration::ZERO);
        assert_eq!(
            told.try_recv(),
            Ok(Control::Lost(
                "its capture ended: the captured window closed".to_owned()
            ))
        );
    }

    #[test]
    fn frames_that_keep_failing_restart_the_capture() {
        let (events, readings) = mpsc::channel::<Reading>();
        let (control, told) = mpsc::channel();
        let icon = job(vec![red_at("icon", 0, 0)]);
        let mut frames = frames_at(&icon, WINDOW, events, control);
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
                    session: SessionId(3),
                    reader: ReaderNumber(2),
                },
            ]
        );
        assert_eq!(frames.handle(Ok(frame(&red, WINDOW))), AFTER_END);
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
            let mut frames = frames_at(&job, WINDOW, events.clone(), control.clone());
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
    fn a_frame_whose_reading_panics_tells_the_reader_and_reads_no_more() {
        let (events, readings) = mpsc::channel::<Tripwire>();
        let (control, told) = mpsc::channel();
        let job = job(vec![red_at("icon", 0, 0)]);
        let placed = place(&job.plan, WINDOW);
        let mut frames = FrameReader::new(
            &job,
            placed,
            events,
            control,
            mpsc::channel().1,
            mpsc::channel().1,
        );
        let (red, black) = (
            frame_image(2, 2, |_, _| true),
            frame_image(2, 2, |_, _| false),
        );
        frames.handle(Ok(frame(&red, WINDOW)));
        assert_eq!(told.try_recv(), Ok(Control::Panicked("broke".to_owned())));
        frames.handle(Ok(frame(&black, WINDOW)));
        assert_eq!(readings.try_recv(), Err(TryRecvError::Empty));
        assert_eq!(told.try_recv(), Err(TryRecvError::Empty));
    }

    #[test]
    fn a_restarted_capture_whose_first_frame_panics_has_not_recovered() {
        let (events, _readings) = mpsc::channel::<Tripwire>();
        let (control, told) = mpsc::channel();
        let job = job(vec![red_at("icon", 0, 0)]);
        let placed = place(&job.plan, WINDOW);
        let mut frames = FrameReader::new(
            &job,
            placed,
            events,
            control,
            mpsc::channel().1,
            mpsc::channel().1,
        );
        let mut recovery = Recovery::new();
        recovery.failed("reading a frame panicked".to_owned(), None);
        frames.carry(&recovery, Vec::new());
        let red = frame_image(2, 2, |_, _| true);
        frames.handle(Ok(frame(&red, WINDOW)));
        assert_eq!(
            told.try_iter().collect::<Vec<_>>(),
            [Control::Panicked("broke".to_owned())]
        );
    }

    #[test]
    fn a_restarted_capture_logs_the_same_failure_once_and_says_when_it_reads() {
        let (events, readings) = mpsc::channel::<Reading>();
        let (control, told) = mpsc::channel();
        let icon = job(vec![red_at("icon", 0, 0)]);
        let mut frames = frames_at(&icon, WINDOW, events, control);
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
}
