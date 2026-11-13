//! The capture's frame loop: when a captured frame is read, which one, and
//! what ends the capture.
//!
//! The system pushes a frame each time the window's content changes. Between
//! reads the loop keeps only the newest frame and copies nothing. When the
//! handler's wait ends, it finds where the window's content lies in the
//! frame, copies the crop's regions out of it, and lends the handler the
//! copy. A closed window and a lost graphics device stop frames rather than
//! fail them, so the loop checks for both every [`HEALTH_CHECK`], whatever
//! arrives meanwhile.
//!
//! The loop holds every decision, and a [`FrameSource`] makes the system
//! calls, so the loop reads the same on every platform and tests drive it
//! with a scripted source.

use std::io;
use std::time::{Duration, Instant};

use super::{Crop, Frame, FrameBuffer, OutsideWindow, Placement, Region, Size, content, in_frame};
use crate::system::Bounds;

/// How often the loop checks that its window is there and its graphics
/// device works.
const HEALTH_CHECK: Duration = Duration::from_secs(2);

/// The wait a frame handler returns when it wants no more frames, such as
/// once it has the one frame it started the capture for. The capture hands
/// it none until it is closed.
pub const NO_MORE_FRAMES: Duration = Duration::MAX;

/// The longest wait a capture keeps; a longer one, such as
/// [`NO_MORE_FRAMES`], is cut to this so the deadline stays representable.
const LONGEST_WAIT: Duration = Duration::from_hours(24);

/// The shortest wait a capture asks the system to match. Below it, the
/// capture keeps its throttle, so a value that changes on every other read
/// does not change the capture's setting on every read.
const SHORTEST_PACED_WAIT: Duration = Duration::from_millis(64);

/// A system time span's ticks in a second: each is 100 nanoseconds.
const TICKS_PER_SECOND: i128 = 10_000_000;

/// The frame handler as [`super::run_capture`] lends it to the loop.
pub type Handler<'handler> = dyn FnMut(io::Result<Frame<'_>>) -> Duration + 'handler;

/// What ended a wait for the capture's next event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wake {
    /// The system has at least one new frame.
    Frame,
    /// The session is closing.
    Stop,
    /// The wait ran out.
    Timeout,
}

/// The system calls of one running capture, each one call that decides
/// nothing: [`run_frames`] decides.
pub trait FrameSource {
    /// A frame the system lends until it is released.
    type Frame;

    /// The time now.
    fn now(&self) -> Instant;

    /// Waits up to `timeout` for the capture's next event.
    fn wait(&mut self, timeout: Duration) -> Wake;

    /// Takes the next frame the system holds for the capture, with how long
    /// ago the system composed it, or `None` when it holds no more.
    fn next_frame(&mut self) -> Option<(Self::Frame, Duration)>;

    /// Gives `frame` back to the system.
    fn release(&mut self, frame: Self::Frame);

    /// Whether the captured window is gone.
    fn window_closed(&self) -> bool;

    /// Why the graphics device can no longer be used, or `None` while it
    /// works.
    fn lost_device(&self) -> Option<String>;

    /// The size of `frame`'s picture.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn frame_size(&self, frame: &Self::Frame) -> io::Result<Size>;

    /// Makes the system's later frames `size`.
    ///
    /// # Errors
    ///
    /// Returns why the system cannot.
    fn resize_frames(&mut self, size: Size) -> Result<(), String>;

    /// Where the window's frame and its client area lie on the screen, in
    /// that order, or `None` when either cannot be read.
    fn window_bounds(&self) -> Option<(Bounds, Bounds)>;

    /// Copies each of `copies` out of `frame` into a picture of `size`, at
    /// the left edge of the row the copy names, and fills `buffer` with
    /// that picture. The loop makes sure every copy lies inside the frame's
    /// picture and lands inside `size`.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn copy(
        &mut self,
        frame: &Self::Frame,
        size: Size,
        copies: &[Placement],
        buffer: &mut FrameBuffer,
    ) -> io::Result<()>;

    /// Asks the system for at most one frame every `interval`.
    fn ask_interval(&mut self, interval: Duration);
}

/// A frame held until the handler's wait ends.
struct Held<F> {
    frame: F,
    received: Instant,
    delay: Duration,
}

/// What one capture's loop keeps between frames.
struct FrameLoop {
    /// The size the system makes its frames at.
    frames: Size,
    /// The least time between two frames that the capture may ask for.
    throttle: Duration,
    /// The time between frames that the capture asks the system for now.
    asked: Duration,
    /// When the handler's last wait ends.
    next_read: Instant,
    /// When the window and the device were last checked.
    checked: Instant,
    /// The copies of the frame being read, in the frame's own coordinates.
    copies: Vec<Placement>,
    buffer: FrameBuffer,
}

/// Hands `crop` of `source`'s newest frame to `handler` whenever its last
/// wait has ended, until the session stops or the capture ends.
///
/// An ending reaches the handler as a [`io::ErrorKind::NotConnected`] error
/// that says why. The system makes frames of `frames` size and was asked for at most one
/// every `throttle` when the capture started.
pub fn run_frames<S: FrameSource>(
    source: &mut S,
    frames: Size,
    throttle: Duration,
    crop: &Crop,
    handler: &mut Handler<'_>,
) {
    let started = source.now();
    let mut state = FrameLoop {
        frames,
        throttle,
        asked: throttle,
        next_read: started,
        checked: started,
        copies: Vec::new(),
        buffer: FrameBuffer::default(),
    };
    let mut held: Option<Held<S::Frame>> = None;
    loop {
        let wait = state.wait(source.now(), held.is_some());
        match source.wait(wait) {
            Wake::Stop => break,
            Wake::Frame => take_newest(source, &mut held),
            Wake::Timeout => {},
        }
        // Checked on a deadline, so neither steady wakes nor a handler
        // that waits long put the check off.
        if state.check_due(source.now())
            && let Some(ending) = ending(source)
        {
            end(handler, ending);
            break;
        }
        if let Some(frame) = held.take_if(|_| source.now() >= state.next_read) {
            let Some(wait) = state.read(source, frame, crop, handler) else {
                break;
            };
            state.after_read(source, wait);
        }
    }
    if let Some(frame) = held {
        source.release(frame.frame);
    }
}

/// Takes every frame the system holds, keeping only the newest.
fn take_newest<S: FrameSource>(source: &mut S, held: &mut Option<Held<S::Frame>>) {
    while let Some((frame, delay)) = source.next_frame() {
        let newest = Held {
            frame,
            received: source.now(),
            delay,
        };
        if let Some(older) = held.replace(newest) {
            source.release(older.frame);
        }
    }
}

/// Why the capture cannot go on, or `None` while its window is there and
/// its device works.
fn ending<S: FrameSource>(source: &S) -> Option<io::Error> {
    if source.window_closed() {
        return Some(ended("the captured window closed"));
    }
    lost_device(source)
}

/// Why the capture cannot go on because its graphics device is lost, such
/// as after a graphics driver reset.
fn lost_device<S: FrameSource>(source: &S) -> Option<io::Error> {
    source
        .lost_device()
        .map(|error| ended(&format!("the graphics device was lost: {error}")))
}

/// The error that tells a handler its capture ended.
fn ended(why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::NotConnected, why)
}

/// Tells `handler` that its capture ended. Its wait is moot: nothing more
/// will arrive.
fn end(handler: &mut Handler<'_>, error: io::Error) {
    let _wait = handler(Err(error));
}

impl FrameLoop {
    /// How long to wait for the next event at `now`: until the check is
    /// due, or until the read is while a frame is held.
    fn wait(&self, now: Instant, holding: bool) -> Duration {
        let check_in = HEALTH_CHECK.saturating_sub(now.saturating_duration_since(self.checked));
        if holding {
            self.next_read.saturating_duration_since(now).min(check_in)
        } else {
            check_in
        }
    }

    /// Whether the window and the device are to be checked at `now`, which
    /// then counts as checked.
    fn check_due(&mut self, now: Instant) -> bool {
        let due = now.saturating_duration_since(self.checked) >= HEALTH_CHECK;
        if due {
            self.checked = now;
        }
        due
    }

    /// Copies `crop` of one frame and passes it to `handler`, returning the
    /// handler's wait, or `None` when the capture ended.
    fn read<S: FrameSource>(
        &mut self,
        source: &mut S,
        held: Held<S::Frame>,
        crop: &Crop,
        handler: &mut Handler<'_>,
    ) -> Option<Duration> {
        let copied = self.copy(source, &held.frame, crop);
        source.release(held.frame);
        match copied {
            Ok(Some(window_size)) => Some(match self.buffer.image() {
                Some(image) => handler(Ok(Frame {
                    window_size,
                    image,
                    received: held.received,
                    delay: held.delay,
                })),
                None => Duration::ZERO,
            }),
            // The system makes its frames at the window's new size from
            // here on; the next frame has it.
            Ok(None) => Some(Duration::ZERO),
            Err(error) => match lost_device(source) {
                Some(lost) => {
                    end(handler, lost);
                    None
                },
                None if error.kind() == io::ErrorKind::NotConnected => {
                    end(handler, error);
                    None
                },
                None => Some(handler(Err(error))),
            },
        }
    }

    /// Copies `crop` of `frame`'s content into the buffer and returns the
    /// content's size, or `None` when the frame was dropped because the
    /// window changed size.
    fn copy<S: FrameSource>(
        &mut self,
        source: &mut S,
        frame: &S::Frame,
        crop: &Crop,
    ) -> io::Result<Option<Size>> {
        let captured = source.frame_size(frame)?;
        if captured != self.frames {
            source.resize_frames(captured).map_err(|error| {
                ended(&format!(
                    "the capture cannot follow the window's new size: {error}"
                ))
            })?;
            self.frames = captured;
            return Ok(None);
        }
        // Without a client area that covers some of the frame, the whole
        // frame is the content.
        let area = source
            .window_bounds()
            .and_then(|(frame, client)| content(frame, client, captured))
            .or_else(|| Region::whole(captured))
            .ok_or(OutsideWindow {
                window_size: captured,
            })?;
        let window_size = area.size();
        let outside = OutsideWindow { window_size };
        let whole = [Placement {
            from: Region::whole(window_size).ok_or(outside)?,
            y: 0,
        }];
        let (size, placements) = match crop {
            Crop::Whole => (window_size, whole.as_slice()),
            Crop::Layout(layout) => (layout.size(), layout.copies()),
        };
        self.copies.clear();
        for placement in placements {
            self.copies.push(Placement {
                from: in_frame(placement.from, area).ok_or(outside)?,
                y: placement.y,
            });
        }
        source.copy(frame, size, &self.copies, &mut self.buffer)?;
        Ok(Some(window_size))
    }

    /// Records a read whose handler asked to wait `wait`, and asks the
    /// system for frames no more often than the handler reads them, as
    /// [`frame_interval`] works out, so a window whose values hold still
    /// costs the system fewer copies too. The system is expected to hold
    /// back a change drawn inside that time and deliver it once the time
    /// has passed, which is when the handler would have read again anyway;
    /// the footprint check's settle check tests that on Windows.
    fn after_read<S: FrameSource>(&mut self, source: &mut S, wait: Duration) {
        let now = source.now();
        self.next_read = now.checked_add(wait.min(LONGEST_WAIT)).unwrap_or(now);
        let interval = frame_interval(wait, self.throttle);
        if interval != self.asked {
            self.asked = interval;
            source.ask_interval(interval);
        }
    }
}

/// The least time between frames a capture asks the system for.
///
/// After a read that asked to wait `wait`, it is the wait itself, cut to
/// [`LONGEST_WAIT`] and never below `throttle`, or just `throttle` for a wait
/// under [`SHORTEST_PACED_WAIT`].
fn frame_interval(wait: Duration, throttle: Duration) -> Duration {
    if wait < SHORTEST_PACED_WAIT {
        return throttle;
    }
    wait.min(LONGEST_WAIT).max(throttle)
}

/// How long ago the system composed a frame.
///
/// The system stamps a frame with `composed`, in 100-nanosecond ticks of
/// its performance counter, which reads `counter` now and counts
/// `frequency` a second. It is `None` when the numbers give no time, such
/// as a frame stamped after now.
#[must_use]
pub fn composed_ago(composed: i64, counter: i64, frequency: i64) -> Option<Duration> {
    let now = i128::from(counter)
        .checked_mul(TICKS_PER_SECOND)?
        .checked_div(i128::from(frequency).max(1))?;
    let ticks = u64::try_from(now.checked_sub(i128::from(composed))?).ok()?;
    Some(Duration::from_nanos(ticks.saturating_mul(100)))
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;
    use crate::screen::Layout;

    const fn millis(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    const fn size(width: u32, height: u32) -> Size {
        Size { width, height }
    }

    /// The size the scripted system makes its frames at.
    const FRAMES: Size = size(800, 600);

    /// The least time between frames the scripted capture started with.
    const THROTTLE: Duration = millis(16);

    /// What happens to a scripted capture.
    #[derive(Debug)]
    enum Happens {
        /// Frames arrive, each with its number and size, and wake the loop.
        Frames(Vec<(u8, Size)>),
        /// The session closes, which wakes the loop.
        Stop,
        /// The window closes, which wakes nothing.
        WindowCloses,
        /// The graphics device is lost, which wakes nothing.
        DeviceLost,
    }

    /// A system call the loop made that changed something.
    #[derive(Debug, PartialEq, Eq)]
    enum Call {
        Waited(Duration),
        Released(u8),
        Resized(Size),
        Copied(u8, Size, Vec<Placement>),
        Asked(Duration),
    }

    /// A capture whose events are scripted: each happens some time after
    /// the one before it, and time passes only while the loop waits.
    struct Script {
        started: Instant,
        now: Instant,
        events: VecDeque<(Duration, Happens)>,
        pool: VecDeque<(u8, Size)>,
        closed: bool,
        lost: bool,
        bounds: Option<(Bounds, Bounds)>,
        /// The error each copy fails with, in order; `None` copies.
        copy_errors: VecDeque<Option<io::ErrorKind>>,
        resizes: bool,
        calls: Vec<Call>,
    }

    impl Script {
        fn new(events: impl IntoIterator<Item = (u64, Happens)>) -> Self {
            let started = Instant::now();
            Self {
                started,
                now: started,
                events: events
                    .into_iter()
                    .map(|(after, happens)| (millis(after), happens))
                    .collect(),
                pool: VecDeque::new(),
                closed: false,
                lost: false,
                bounds: None,
                copy_errors: VecDeque::new(),
                resizes: true,
                calls: Vec::new(),
            }
        }

        /// Lets `time` pass.
        fn pass(&mut self, time: Duration) {
            self.now = self
                .now
                .checked_add(time)
                .expect("a time the clock can tell");
        }

        fn waits(&self) -> Vec<Duration> {
            self.calls
                .iter()
                .filter_map(|call| match call {
                    Call::Waited(wait) => Some(*wait),
                    _ => None,
                })
                .collect()
        }

        /// The calls other than waits.
        fn did(&self) -> Vec<&Call> {
            self.calls
                .iter()
                .filter(|call| !matches!(call, Call::Waited(_)))
                .collect()
        }
    }

    impl FrameSource for Script {
        type Frame = (u8, Size);

        fn now(&self) -> Instant {
            self.now
        }

        fn wait(&mut self, timeout: Duration) -> Wake {
            self.calls.push(Call::Waited(timeout));
            assert!(self.calls.len() < 1_000, "the loop never ended");
            let mut left = timeout;
            loop {
                let Some((after, happens)) = self.events.pop_front() else {
                    self.pass(left);
                    return Wake::Timeout;
                };
                let Some(rest) = left.checked_sub(after) else {
                    let later = after.checked_sub(left).expect("after is longer");
                    self.events.push_front((later, happens));
                    self.pass(left);
                    return Wake::Timeout;
                };
                self.pass(after);
                left = rest;
                match happens {
                    Happens::Frames(frames) => {
                        self.pool.extend(frames);
                        return Wake::Frame;
                    },
                    Happens::Stop => return Wake::Stop,
                    Happens::WindowCloses => self.closed = true,
                    Happens::DeviceLost => self.lost = true,
                }
            }
        }

        fn next_frame(&mut self) -> Option<((u8, Size), Duration)> {
            let frame = self.pool.pop_front()?;
            Some((frame, millis(frame.0.into())))
        }

        fn release(&mut self, frame: (u8, Size)) {
            self.calls.push(Call::Released(frame.0));
        }

        fn window_closed(&self) -> bool {
            self.closed
        }

        fn lost_device(&self) -> Option<String> {
            self.lost.then(|| "the driver was reset".to_owned())
        }

        fn frame_size(&self, frame: &(u8, Size)) -> io::Result<Size> {
            Ok(frame.1)
        }

        fn resize_frames(&mut self, size: Size) -> Result<(), String> {
            self.calls.push(Call::Resized(size));
            if self.resizes {
                Ok(())
            } else {
                Err("out of memory".to_owned())
            }
        }

        fn window_bounds(&self) -> Option<(Bounds, Bounds)> {
            self.bounds
        }

        fn copy(
            &mut self,
            frame: &(u8, Size),
            size: Size,
            copies: &[Placement],
            buffer: &mut FrameBuffer,
        ) -> io::Result<()> {
            self.calls
                .push(Call::Copied(frame.0, size, copies.to_vec()));
            if let Some(kind) = self.copy_errors.pop_front().flatten() {
                return Err(io::Error::new(kind, "the copy failed"));
            }
            buffer
                .prepare(size)
                .map_err(io::Error::other)?
                .fill(frame.0);
            Ok(())
        }

        fn ask_interval(&mut self, interval: Duration) {
            self.calls.push(Call::Asked(interval));
        }
    }

    /// A frame as the handler saw it.
    #[derive(Debug, PartialEq, Eq)]
    struct Seen {
        frame: u8,
        window_size: Size,
        image: Size,
        delay: Duration,
        /// How long after the capture started the frame was received.
        received: Duration,
    }

    /// Runs the loop over `script`, whose frames start at `frames` size,
    /// with a handler that asks for each of `waits` in turn and then the
    /// last again, and returns what the handler saw: each frame, or each
    /// error's kind and text.
    fn run_at(
        script: &mut Script,
        frames: Size,
        crop: &Crop,
        waits: &[Duration],
    ) -> Vec<Result<Seen, (io::ErrorKind, String)>> {
        let started = script.started;
        let mut seen = Vec::new();
        let mut waits = waits.iter().copied();
        let mut wait = Duration::ZERO;
        let mut handler = |frame: io::Result<Frame<'_>>| {
            seen.push(match frame {
                Ok(frame) => Ok(Seen {
                    frame: frame.image.as_bgra()[0],
                    window_size: frame.window_size,
                    image: frame.image.size(),
                    delay: frame.delay,
                    received: frame.received.duration_since(started),
                }),
                Err(error) => Err((error.kind(), error.to_string())),
            });
            wait = waits.next().unwrap_or(wait);
            wait
        };
        run_frames(script, frames, THROTTLE, crop, &mut handler);
        seen
    }

    fn run(
        script: &mut Script,
        crop: &Crop,
        waits: &[Duration],
    ) -> Vec<Result<Seen, (io::ErrorKind, String)>> {
        run_at(script, FRAMES, crop, waits)
    }

    fn frames(numbers: impl IntoIterator<Item = u8>) -> Happens {
        Happens::Frames(numbers.into_iter().map(|number| (number, FRAMES)).collect())
    }

    fn whole(frame: u8) -> Call {
        Call::Copied(
            frame,
            FRAMES,
            vec![Placement {
                from: Region::whole(FRAMES).unwrap(),
                y: 0,
            }],
        )
    }

    fn seen(frame: u8, received: u64) -> Seen {
        Seen {
            frame,
            window_size: FRAMES,
            image: FRAMES,
            delay: millis(frame.into()),
            received: millis(received),
        }
    }

    fn ending(why: &str) -> Result<Seen, (io::ErrorKind, String)> {
        Err((io::ErrorKind::NotConnected, why.to_owned()))
    }

    #[test]
    fn only_the_newest_frame_is_read_and_the_others_are_given_back() {
        let mut script = Script::new([(5, frames([1, 2, 3])), (0, Happens::Stop)]);
        let saw = run(&mut script, &Crop::Whole, &[millis(10)]);
        assert_eq!(saw, [Ok(seen(3, 5))]);
        assert_eq!(
            script.calls,
            [
                Call::Waited(HEALTH_CHECK),
                Call::Released(1),
                Call::Released(2),
                whole(3),
                Call::Released(3),
                Call::Waited(millis(1_995)),
            ]
        );
    }

    #[test]
    fn a_frame_that_arrives_during_the_wait_is_held_until_the_wait_ends() {
        let mut script = Script::new([
            (0, frames([1])),
            (30, frames([2])),
            (30, frames([3])),
            (1_000, Happens::Stop),
        ]);
        let saw = run(&mut script, &Crop::Whole, &[millis(100)]);
        assert_eq!(saw, [Ok(seen(1, 0)), Ok(seen(3, 60))]);
        assert_eq!(
            script.waits(),
            [
                HEALTH_CHECK,
                HEALTH_CHECK,
                millis(70),
                millis(40),
                millis(1_900)
            ],
            "a held frame waits for its read, and nothing held waits for the check"
        );
        assert_eq!(
            script.did(),
            [
                &whole(1),
                &Call::Released(1),
                &Call::Asked(millis(100)),
                &Call::Released(2),
                &whole(3),
                &Call::Released(3),
            ],
            "the system is asked once for frames as often as they are read"
        );
    }

    #[test]
    fn a_held_frame_is_given_back_when_the_session_stops() {
        let mut script = Script::new([(0, frames([1])), (10, frames([2])), (10, Happens::Stop)]);
        let saw = run(&mut script, &Crop::Whole, &[millis(1_000)]);
        assert_eq!(saw, [Ok(seen(1, 0))]);
        assert_eq!(script.did().last(), Some(&&Call::Released(2)));
    }

    #[test]
    fn a_closed_window_or_a_lost_device_ends_the_capture_at_the_next_check() {
        for (happens, why) in [
            (Happens::WindowCloses, "the captured window closed"),
            (
                Happens::DeviceLost,
                "the graphics device was lost: the driver was reset",
            ),
        ] {
            let mut script = Script::new([(500, happens)]);
            let saw = run(&mut script, &Crop::Whole, &[Duration::ZERO]);
            assert_eq!(saw, [ending(why)]);
            assert_eq!(script.waits(), [HEALTH_CHECK], "{why}");
        }
    }

    #[test]
    fn steady_frames_do_not_put_the_check_off() {
        let mut script = Script::new([
            (0, Happens::WindowCloses),
            (600, frames([1])),
            (600, frames([2])),
            (600, frames([3])),
            (600, frames([4])),
        ]);
        let saw = run(&mut script, &Crop::Whole, &[Duration::ZERO]);
        assert_eq!(
            saw,
            [
                Ok(seen(1, 600)),
                Ok(seen(2, 1_200)),
                Ok(seen(3, 1_800)),
                ending("the captured window closed")
            ]
        );
        assert_eq!(
            script.waits(),
            [HEALTH_CHECK, millis(1_400), millis(800), millis(200)]
        );
    }

    #[test]
    fn a_handler_that_waits_long_does_not_put_the_check_off() {
        let mut script = Script::new([
            (0, frames([1])),
            (100, frames([2])),
            (2_900, Happens::WindowCloses),
        ]);
        let saw = run(&mut script, &Crop::Whole, &[millis(10_000)]);
        assert_eq!(saw, [Ok(seen(1, 0)), ending("the captured window closed")]);
        assert_eq!(
            script.waits(),
            [HEALTH_CHECK, HEALTH_CHECK, millis(1_900), HEALTH_CHECK],
            "the window is checked every two seconds while a frame is held"
        );
        assert_eq!(
            script.did().last(),
            Some(&&Call::Released(2)),
            "the held frame is given back"
        );
    }

    #[test]
    fn a_frame_of_a_new_size_resizes_the_frames_and_is_dropped() {
        let larger = size(1_024, 768);
        let mut script = Script::new([
            (0, Happens::Frames(vec![(1, larger)])),
            (16, Happens::Frames(vec![(2, larger)])),
            (16, Happens::Frames(vec![(3, larger)])),
            (0, Happens::Stop),
        ]);
        let saw = run(&mut script, &Crop::Whole, &[millis(500)]);
        assert_eq!(
            saw,
            [Ok(Seen {
                frame: 2,
                window_size: larger,
                image: larger,
                delay: millis(2),
                received: millis(16),
            })],
            "the frame after the dropped one is read at once"
        );
        assert_eq!(
            script.did().get(..3),
            Some(
                [
                    &Call::Resized(larger),
                    &Call::Released(1),
                    &Call::Copied(
                        2,
                        larger,
                        vec![Placement {
                            from: Region::whole(larger).unwrap(),
                            y: 0
                        }]
                    ),
                ]
                .as_slice()
            ),
            "the frames are resized once"
        );
    }

    #[test]
    fn frames_that_cannot_follow_the_window_s_size_end_the_capture() {
        let mut script = Script::new([(0, Happens::Frames(vec![(1, size(1_024, 768))]))]);
        script.resizes = false;
        let saw = run(&mut script, &Crop::Whole, &[Duration::ZERO]);
        assert_eq!(
            saw,
            [ending(
                "the capture cannot follow the window's new size: out of memory"
            )]
        );
        assert_eq!(script.waits(), [HEALTH_CHECK], "the loop ended");
    }

    #[test]
    fn a_frame_that_cannot_be_copied_fails_alone() {
        let mut script = Script::new([(0, frames([1])), (50, frames([2])), (0, Happens::Stop)]);
        script.copy_errors = [Some(io::ErrorKind::Other)].into();
        let saw = run(&mut script, &Crop::Whole, &[millis(50)]);
        assert_eq!(
            saw,
            [
                Err((io::ErrorKind::Other, "the copy failed".to_owned())),
                Ok(seen(2, 50))
            ]
        );
        assert_eq!(
            script.waits(),
            [HEALTH_CHECK, HEALTH_CHECK, millis(1_950)],
            "the failed frame's handler set the wait"
        );
    }

    #[test]
    fn a_copy_that_fails_with_the_device_lost_or_the_capture_ended_ends_the_capture() {
        let mut lost = Script::new([(0, Happens::DeviceLost), (0, frames([1]))]);
        lost.copy_errors = [Some(io::ErrorKind::Other)].into();
        assert_eq!(
            run(&mut lost, &Crop::Whole, &[Duration::ZERO]),
            [ending("the graphics device was lost: the driver was reset")]
        );
        assert_eq!(lost.waits(), [HEALTH_CHECK], "the loop ended");

        let mut over = Script::new([(0, frames([1]))]);
        over.copy_errors = [Some(io::ErrorKind::NotConnected)].into();
        assert_eq!(
            run(&mut over, &Crop::Whole, &[Duration::ZERO]),
            [ending("the copy failed")]
        );
        assert_eq!(over.waits(), [HEALTH_CHECK], "the loop ended");
    }

    /// A window at 800x631 with a title bar of 30 pixels and borders of 1.
    fn titled() -> (Bounds, Bounds) {
        let bounds = |left, top, right, bottom| Bounds {
            left,
            top,
            right,
            bottom,
        };
        (bounds(100, 50, 900, 681), bounds(101, 81, 899, 680))
    }

    #[test]
    fn regions_are_copied_from_the_window_s_content_in_the_frame() {
        let captured = size(800, 631);
        let content = size(798, 599);
        let region = Region::new(10, 20, 30, 40).unwrap();
        let in_frame = Region::new(11, 51, 30, 40).unwrap();
        for (crop, copied, from) in [
            (Crop::Layout(Layout::one(region)), region.size(), in_frame),
            (Crop::Whole, content, Region::new(1, 31, 798, 599).unwrap()),
        ] {
            let mut script = Script::new([
                (0, Happens::Frames(vec![(7, captured)])),
                (0, Happens::Stop),
            ]);
            script.bounds = Some(titled());
            let saw = run_at(&mut script, captured, &crop, &[]);
            assert_eq!(
                saw,
                [Ok(Seen {
                    frame: 7,
                    window_size: content,
                    image: copied,
                    delay: millis(7),
                    received: Duration::ZERO,
                })]
            );
            assert_eq!(
                script.did().first(),
                Some(&&Call::Copied(7, copied, vec![Placement { from, y: 0 }]))
            );
        }
    }

    #[test]
    fn a_layout_that_reaches_past_the_content_fails_with_the_content_s_size() {
        let captured = size(800, 631);
        // Inside the frame, but past the content's right edge.
        let crop = Crop::Layout(Layout::one(Region::new(790, 0, 10, 10).unwrap()));
        let mut script = Script::new([
            (0, Happens::Frames(vec![(1, captured)])),
            (0, Happens::Stop),
        ]);
        script.bounds = Some(titled());
        let mut errors = Vec::new();
        let mut handler = |frame: io::Result<Frame<'_>>| {
            errors.push(frame.map(|_frame| ()).unwrap_err());
            Duration::ZERO
        };
        run_frames(&mut script, captured, THROTTLE, &crop, &mut handler);
        assert_eq!(
            errors
                .iter()
                .map(OutsideWindow::in_error)
                .collect::<Vec<_>>(),
            [Some(size(798, 599))]
        );
        assert_eq!(script.did(), [&Call::Released(1)], "nothing was copied");
    }

    #[test]
    fn the_system_is_asked_for_frames_as_often_as_they_are_read() {
        let mut script = Script::new([
            (0, frames([1])),
            (500, frames([2])),
            (500, frames([3])),
            (10, frames([4])),
            (2_000, frames([5])),
            (0, Happens::Stop),
        ]);
        let saw = run(
            &mut script,
            &Crop::Whole,
            &[millis(500), millis(500), millis(10), NO_MORE_FRAMES],
        );
        assert_eq!(saw.len(), 4, "{saw:?}");
        let asked: Vec<_> = script
            .calls
            .iter()
            .filter_map(|call| match call {
                Call::Asked(interval) => Some(*interval),
                _ => None,
            })
            .collect();
        assert_eq!(
            asked,
            [millis(500), THROTTLE, LONGEST_WAIT],
            "only when the interval changes"
        );
        assert_eq!(
            script.did().last(),
            Some(&&Call::Released(5)),
            "a handler that wants no more frames is handed none"
        );
    }

    #[test]
    fn a_capture_asks_for_frames_as_often_as_it_reads_them() {
        let throttle = millis(16);
        assert_eq!(frame_interval(millis(500), throttle), millis(500));
        assert_eq!(
            frame_interval(SHORTEST_PACED_WAIT, throttle),
            SHORTEST_PACED_WAIT
        );
        assert_eq!(frame_interval(Duration::MAX, throttle), LONGEST_WAIT);
    }

    #[test]
    fn a_short_wait_keeps_the_throttle() {
        let throttle = millis(16);
        assert_eq!(frame_interval(Duration::ZERO, throttle), throttle);
        assert_eq!(frame_interval(millis(32), throttle), throttle);
        assert_eq!(
            frame_interval(
                SHORTEST_PACED_WAIT.checked_sub(millis(1)).unwrap(),
                throttle
            ),
            throttle
        );
        assert_eq!(frame_interval(millis(32), Duration::ZERO), Duration::ZERO);
    }

    #[test]
    fn a_capture_never_asks_for_frames_faster_than_its_throttle() {
        assert_eq!(frame_interval(millis(100), millis(200)), millis(200));
        let slow = LONGEST_WAIT.checked_mul(2).unwrap();
        assert_eq!(frame_interval(Duration::MAX, slow), slow);
    }

    #[test]
    fn a_frame_s_delay_is_the_time_since_the_system_composed_it() {
        // Composed one second in, read three seconds in.
        assert_eq!(
            composed_ago(10_000_000, 3_000, 1_000),
            Some(Duration::from_secs(2))
        );
        assert_eq!(
            composed_ago(5, 7, 0),
            Some(Duration::from_nanos(6_999_999_500)),
            "a counter that says no frequency counts seconds"
        );
        assert_eq!(composed_ago(10_000_001, 1_000, 1_000), None, "after now");
    }
}
