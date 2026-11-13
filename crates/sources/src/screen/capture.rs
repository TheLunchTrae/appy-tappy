//! The capture interface: what a capture copies out of each frame, the frame it
//! lends to its handler, where a window's content lies in its frame, and the
//! [`Capture`] trait that the operating system's capture implements, with a
//! stand-in for systems that cannot capture.

use std::any::Any;
use std::convert::Infallible;
use std::io;
use std::panic::{self, AssertUnwindSafe};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::{Image, Layout, Region, Size};
use crate::system::{Bounds, Unsupported};

/// What a capture copies out of each frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Crop {
    /// The window's whole content, as the setup app's preview and
    /// screenshot need.
    Whole,
    /// The regions a layout packs, as the values of a window read. The
    /// frame's image is then the packed image.
    Layout(Layout),
}

/// Where a window's content lies in a capture of its frame.
///
/// A capture of a window takes in its frame, at `frame` on the screen, title
/// bar and borders included, in a picture of `captured` size. The
/// application draws in its client area, at `client` on the screen, and only
/// that grows with the window: a title bar keeps its height between a small
/// window and a large one, and fullscreen has none. So the window's content
/// is the part of the picture its client area covers, and regions are
/// measured from it, which places them the same with and without a title
/// bar. It is `None` when the client area covers none of the picture, and
/// the whole picture is the content then.
#[must_use]
pub fn content(frame: Bounds, client: Bounds, captured: Size) -> Option<Region> {
    // How far `edge` lies from the frame's `start`, and 0 before it. Two
    // `i32`s are at most `u32::MAX` apart, so any distance fits.
    let from = |edge: i32, start: i32| {
        u32::try_from(i64::from(edge).saturating_sub(i64::from(start))).unwrap_or_default()
    };
    let left = from(client.left, frame.left);
    let top = from(client.top, frame.top);
    let right = from(client.right, frame.left).min(captured.width);
    let bottom = from(client.bottom, frame.top).min(captured.height);
    Region::new(
        left,
        top,
        right.checked_sub(left)?,
        bottom.checked_sub(top)?,
    )
}

/// Where `region`, measured from the corner of a window's `content`, lies in
/// the frame that [`content`] found it in, or `None` when it reaches past
/// the content.
#[must_use]
pub fn in_frame(region: Region, content: Region) -> Option<Region> {
    region.fits(content.size()).then_some(())?;
    let size = region.size();
    Region::new(
        region.x().checked_add(content.x())?,
        region.y().checked_add(content.y())?,
        size.width,
        size.height,
    )
}

/// Where a window's content lies on the screen.
///
/// The window's frame lies at `frame`, and its client area at `client`, or
/// it is unknown. The content is what [`content`] finds in a capture of the
/// whole frame, and where it finds none, the whole frame, as for a capture.
#[must_use]
pub fn content_on_screen(frame: Bounds, client: Option<Bounds>) -> Bounds {
    let length = |from: i32, to: i32| u32::try_from(to.saturating_sub(from)).unwrap_or_default();
    let whole = Size {
        width: length(frame.left, frame.right),
        height: length(frame.top, frame.bottom),
    };
    client
        .and_then(|client| content(frame, client, whole))
        .and_then(|area| {
            let left = frame.left.checked_add_unsigned(area.x())?;
            let top = frame.top.checked_add_unsigned(area.y())?;
            Some(Bounds {
                left,
                top,
                right: left.checked_add_unsigned(area.size().width)?,
                bottom: top.checked_add_unsigned(area.size().height)?,
            })
        })
        .unwrap_or(frame)
}

/// One captured frame's pixels, lent to the frame handler. The image is
/// reused for the next frame, so a handler copies what it keeps.
#[derive(Debug)]
pub struct Frame<'buffer> {
    /// The size of the window's content, which [`content`] finds in the
    /// frame and a region's coordinates refer to.
    pub window_size: Size,
    /// The cropped pixels: the window's whole content, or the packed image
    /// of the capture's [`Layout`].
    pub image: &'buffer Image,
    /// When AppyTappy received the frame.
    pub received: Instant,
    /// How long the frame waited between the system composing it and
    /// AppyTappy receiving it.
    pub delay: Duration,
}

impl Frame<'_> {
    /// The time from the system composing the frame until now: the
    /// capture-to-decision latency when a handler calls it after deciding.
    #[must_use]
    pub fn latency(&self) -> Duration {
        self.delay.saturating_add(self.received.elapsed())
    }
}

/// Receives a capture's newest frame, or the error that stopped one frame.
/// A [`io::ErrorKind::NotConnected`] error says the capture ended, such as
/// when the window closed, and no frame follows it.
///
/// It returns how long to wait before handling another, such as the wait a
/// [`crate::schedule::ReadSchedule`] gives, and runs on the capture's own
/// thread. A panic in it unwinds that thread, so the capture stops and
/// tells nobody until it is closed: a handler whose owner must hear of a
/// panic catches its own. A panic elsewhere in the capture's frame loop
/// reaches the handler as the capture's end, as [`run_capture`] says.
pub type FrameHandler = Box<dyn FnMut(io::Result<Frame<'_>>) -> Duration + Send>;

/// Runs `work`, which hands a capture's frames to `handler` on the
/// capture's thread, and tells the handler when the work panics.
///
/// A panic outside the handler reaches it as a
/// [`io::ErrorKind::NotConnected`] error with the panic's message, as for
/// any capture that cannot go on. Unwound, the panic would end the thread,
/// and the handler would hear nothing until the capture closed. A panic in
/// the handler itself unwinds on, because a handler that panicked is not
/// called again.
pub fn run_capture(
    handler: &mut FrameHandler,
    work: impl FnOnce(&mut dyn FnMut(io::Result<Frame<'_>>) -> Duration),
) {
    let mut handling = false;
    // The handler is called again only when the panic did not come from it,
    // so it never sees itself half changed.
    let worked = panic::catch_unwind(AssertUnwindSafe(|| {
        work(&mut |frame| {
            handling = true;
            let wait = handler(frame);
            handling = false;
            wait
        });
    }));
    let Err(payload) = worked else {
        return;
    };
    if handling {
        panic::resume_unwind(payload);
    }
    let ended = io::Error::new(
        io::ErrorKind::NotConnected,
        format!("the capture panicked: {}", panic_reason(payload.as_ref())),
    );
    // The wait is moot: nothing more will arrive.
    let _wait = handler(Err(ended));
}

/// The message a panic left.
fn panic_reason(payload: &(dyn Any + Send)) -> &str {
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("an unknown reason")
}

/// Captures one window's frames.
///
/// A capture is pushed frames as the window's content changes and never
/// polls: a window that does not change costs nothing. Frames that arrive
/// during the wait the handler returned are released without being copied;
/// when the wait ends, the newest frame since then is handled.
pub trait Capture {
    /// A running capture.
    type Session: CaptureSession;

    /// Starts capturing `window`, a handle from [`crate::system::System::windows`],
    /// and calls `handler` with `crop` of new frames. Where the system
    /// supports it, it also asks the system for at most one frame every
    /// `throttle`, or every wait the handler last returned when that is
    /// longer, which saves the system's own work as well as ours.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error when the capture cannot start.
    fn start(
        &self,
        window: u64,
        crop: Crop,
        throttle: Duration,
        handler: FrameHandler,
    ) -> io::Result<Self::Session>;
}

/// A running capture.
pub trait CaptureSession {
    /// Stops the capture and waits for its thread to finish.
    ///
    /// # Errors
    ///
    /// Returns an error when the capture's thread failed.
    fn close(self) -> io::Result<()>;
}

/// How long [`grab`]'s capture waits after its frame: longer than any grab
/// takes, so the system is asked for no more frames before it closes.
const AFTER_GRAB: Duration = Duration::from_secs(3600);

/// Copies `region` of one frame of `window`, with the window's size.
///
/// `window` is a handle from [`crate::system::System::windows`], and the
/// region's coordinates refer to the window's size. It starts a capture,
/// waits up to `wait` for its first frame, and closes it.
///
/// # Errors
///
/// Returns the operating system's error when the capture cannot start or
/// the frame cannot be copied, such as an [`super::OutsideWindow`] error
/// when the region does not fit the window, or
/// [`io::ErrorKind::TimedOut`] when no frame came in time.
pub fn grab<C: Capture>(
    capture: &C,
    window: u64,
    region: Region,
    wait: Duration,
) -> io::Result<(Size, Image)> {
    let (sender, frames) = mpsc::sync_channel(1);
    let handler: FrameHandler = Box::new(move |frame| {
        let copied = frame.map(|frame| (frame.window_size, frame.image.clone()));
        // Only the first frame is wanted: the channel holds one, and once
        // the grab has it, nothing reads the rest.
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a frame after the first, or after the grab stopped waiting, is not wanted"
        )]
        let _ = sender.try_send(copied);
        AFTER_GRAB
    });
    let session = capture.start(
        window,
        Crop::Layout(Layout::one(region)),
        Duration::ZERO,
        handler,
    )?;
    let frame = frames.recv_timeout(wait);
    session.close()?;
    frame.unwrap_or_else(|_timeout| {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "the window sent no frame in time",
        ))
    })
}

/// The session type of [`Unsupported`], which cannot be constructed.
#[derive(Debug)]
pub struct NoSession(Infallible);

impl CaptureSession for NoSession {
    // `NoSession` holds `Infallible`, so no value exists and this body never
    // runs. No test can call it, so mutating it cannot be caught.
    #[mutants::skip]
    fn close(self) -> io::Result<()> {
        match self.0 {}
    }
}

impl Capture for Unsupported {
    type Session = NoSession;

    fn start(
        &self,
        _window: u64,
        _crop: Crop,
        _throttle: Duration,
        _handler: FrameHandler,
    ) -> io::Result<NoSession> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "AppyTappy captures windows on Windows only",
        ))
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    /// What the fake capture was asked to do.
    #[derive(Debug, PartialEq)]
    enum Asked {
        Start(u64, Crop),
        Close,
    }

    /// A capture that sends `frames` to its handler as it starts, and says
    /// what it was asked to do on `asked`.
    struct Fake {
        frames: Vec<Result<Image, io::ErrorKind>>,
        asked: mpsc::Sender<Asked>,
        refuse: bool,
    }

    struct FakeSession(mpsc::Sender<Asked>);

    impl CaptureSession for FakeSession {
        fn close(self) -> io::Result<()> {
            self.0.send(Asked::Close).map_err(io::Error::other)
        }
    }

    impl Capture for Fake {
        type Session = FakeSession;

        fn start(
            &self,
            window: u64,
            crop: Crop,
            _throttle: Duration,
            mut handler: FrameHandler,
        ) -> io::Result<FakeSession> {
            if self.refuse {
                return Err(io::Error::other("no capture"));
            }
            self.asked
                .send(Asked::Start(window, crop))
                .map_err(io::Error::other)?;
            let window_size = Size {
                width: 640,
                height: 480,
            };
            for frame in &self.frames {
                handler(match frame {
                    Ok(image) => Ok(Frame {
                        window_size,
                        image,
                        received: Instant::now(),
                        delay: Duration::ZERO,
                    }),
                    Err(kind) => Err(io::Error::from(*kind)),
                });
            }
            Ok(FakeSession(self.asked.clone()))
        }
    }

    fn fake(frames: Vec<Result<Image, io::ErrorKind>>) -> (Fake, mpsc::Receiver<Asked>) {
        let (asked, requests) = mpsc::channel();
        (
            Fake {
                frames,
                asked,
                refuse: false,
            },
            requests,
        )
    }

    fn pixel(gray: u8) -> Image {
        Image::new(
            Size {
                width: 1,
                height: 1,
            },
            vec![gray, gray, gray, u8::MAX],
        )
        .unwrap()
    }

    #[test]
    fn a_grab_takes_the_first_frame_of_its_region_and_closes() {
        let (capture, asked) = fake(vec![Ok(pixel(10)), Ok(pixel(20))]);
        let region = Region::new(5, 6, 1, 1).unwrap();
        let (size, image) = grab(&capture, 7, region, Duration::from_secs(5)).unwrap();
        assert_eq!(
            size,
            Size {
                width: 640,
                height: 480
            }
        );
        assert_eq!(image, pixel(10));
        assert_eq!(
            asked.try_iter().collect::<Vec<_>>(),
            [
                Asked::Start(7, Crop::Layout(Layout::one(region))),
                Asked::Close
            ]
        );
    }

    #[test]
    fn a_grab_fails_when_the_frame_or_the_capture_does() {
        let region = Region::new(0, 0, 1, 1).unwrap();
        let wait = Duration::from_millis(10);
        for (frames, kind) in [
            (
                vec![Err(io::ErrorKind::InvalidInput)],
                io::ErrorKind::InvalidInput,
            ),
            (Vec::new(), io::ErrorKind::TimedOut),
        ] {
            let (capture, asked) = fake(frames);
            assert_eq!(grab(&capture, 7, region, wait).unwrap_err().kind(), kind);
            assert_eq!(asked.try_iter().last(), Some(Asked::Close), "{kind:?}");
        }
        let (mut refusing, asked) = fake(Vec::new());
        refusing.refuse = true;
        assert_eq!(
            grab(&refusing, 7, region, wait).unwrap_err().to_string(),
            "no capture"
        );
        assert_eq!(asked.try_iter().count(), 0);
    }

    fn bounds(left: i32, top: i32, right: i32, bottom: i32) -> Bounds {
        Bounds {
            left,
            top,
            right,
            bottom,
        }
    }

    const fn size(width: u32, height: u32) -> Size {
        Size { width, height }
    }

    #[test]
    fn a_window_s_content_leaves_out_its_title_bar_and_borders() {
        let frame = bounds(100, 50, 900, 681);
        let client = bounds(101, 81, 899, 680);
        assert_eq!(
            content(frame, client, size(800, 631)),
            Region::new(1, 31, 798, 599)
        );
    }

    #[test]
    fn a_window_drawn_to_its_edges_is_all_content() {
        let screen = bounds(-1920, 0, 0, 1080);
        assert_eq!(
            content(screen, screen, size(1920, 1080)),
            Region::new(0, 0, 1920, 1080)
        );
        // A client area larger than the frame, as when a maximized window's
        // reaches past the screen, still gives only the capture.
        assert_eq!(
            content(screen, bounds(-1928, -8, 8, 1088), size(1920, 1080)),
            Region::new(0, 0, 1920, 1080)
        );
    }

    #[test]
    fn a_window_s_content_ends_where_its_capture_does() {
        // The window grew after the system composed the frame.
        let frame = bounds(0, 0, 820, 650);
        let client = bounds(1, 31, 819, 649);
        assert_eq!(
            content(frame, client, size(800, 631)),
            Region::new(1, 31, 799, 600)
        );
    }

    #[test]
    fn a_client_area_outside_the_capture_gives_no_content() {
        let frame = bounds(0, 0, 800, 600);
        for client in [
            bounds(10, 10, 10, 10),
            bounds(900, 0, 1000, 600),
            bounds(0, -100, 800, -10),
        ] {
            assert_eq!(content(frame, client, size(800, 600)), None, "{client:?}");
        }
    }

    #[test]
    fn a_window_s_content_on_the_screen_is_its_client_area_inside_its_frame() {
        let frame = bounds(-800, 50, 0, 681);
        assert_eq!(
            content_on_screen(frame, Some(bounds(-799, 81, -1, 680))),
            bounds(-799, 81, -1, 680)
        );
        assert_eq!(
            content_on_screen(frame, Some(bounds(-900, 0, 100, 700))),
            frame
        );
        for client in [None, Some(bounds(10, 10, 20, 20))] {
            assert_eq!(content_on_screen(frame, client), frame, "{client:?}");
        }
    }

    #[test]
    fn a_region_of_the_content_lies_past_the_title_bar_in_the_frame() {
        let content = Region::new(1, 31, 798, 599).unwrap();
        assert_eq!(
            in_frame(Region::new(0, 0, 10, 4).unwrap(), content),
            Region::new(1, 31, 10, 4)
        );
        assert_eq!(
            in_frame(Region::new(788, 595, 10, 4).unwrap(), content),
            Region::new(789, 626, 10, 4)
        );
        assert_eq!(
            in_frame(Region::new(789, 595, 10, 4).unwrap(), content),
            None
        );
    }

    proptest! {
        /// The capture copies a window's content out of its frame's
        /// texture, so the content never reaches past the frame.
        #[test]
        fn a_window_s_content_lies_inside_its_capture(
            corner in (-5_000_i32..5_000, -5_000_i32..5_000),
            width in 1_i32..5_000,
            height in 1_i32..5_000,
            // How far each client edge lies right of or below the frame's.
            shifts in (-50_i32..100, -50_i32..100, -100_i32..50, -100_i32..50),
            captured in (1_u32..5_100, 1_u32..5_100),
        ) {
            let add = |one: i32, other: i32| one.checked_add(other).expect("the ranges are small");
            let (left, top) = corner;
            let (right, bottom) = (add(left, width), add(top, height));
            let frame = bounds(left, top, right, bottom);
            let client = bounds(
                add(left, shifts.0),
                add(top, shifts.1),
                add(right, shifts.2),
                add(bottom, shifts.3),
            );
            let captured = size(captured.0, captured.1);
            if let Some(area) = content(frame, client, captured) {
                prop_assert!(area.fits(captured), "{area:?}");
            }
        }

        /// A region that fits the content lies inside it in the frame.
        #[test]
        fn a_region_of_the_content_stays_inside_it_in_the_frame(
            content_at in (0_u32..100, 0_u32..100, 1_u32..500, 1_u32..500),
            region_at in (0_u32..600, 0_u32..600, 1_u32..600, 1_u32..600),
        ) {
            let content = Region::new(content_at.0, content_at.1, content_at.2, content_at.3).unwrap();
            let region = Region::new(region_at.0, region_at.1, region_at.2, region_at.3).unwrap();
            let placed = in_frame(region, content);
            prop_assert_eq!(placed.is_some(), region.fits(content.size()));
            if let Some(placed) = placed {
                prop_assert!(placed.x() >= content.x() && placed.right() <= content.right());
                prop_assert!(placed.y() >= content.y() && placed.bottom() <= content.bottom());
                prop_assert_eq!(placed.size(), region.size());
            }
        }
    }

    /// A handler that records what it was handed: each frame's gray, or
    /// each error's kind and message. It panics on a frame of gray 0.
    fn recording() -> (FrameHandler, mpsc::Receiver<Result<u8, String>>) {
        let (handed, received) = mpsc::channel();
        let handler: FrameHandler = Box::new(move |frame| {
            let what = match frame {
                Ok(frame) => {
                    let gray = frame.image.pixels()[0][0];
                    assert_ne!(gray, 0, "the handler broke");
                    Ok(gray)
                },
                Err(error) => Err(format!("{:?}: {error}", error.kind())),
            };
            handed.send(what).unwrap();
            Duration::ZERO
        });
        (handler, received)
    }

    fn hand(handler: &mut dyn FnMut(io::Result<Frame<'_>>) -> Duration, image: &Image) {
        handler(Ok(Frame {
            window_size: image.size(),
            image,
            received: Instant::now(),
            delay: Duration::ZERO,
        }));
    }

    #[test]
    fn a_capture_that_panics_after_a_frame_tells_its_handler_it_ended() {
        let (mut handler, handed) = recording();
        run_capture(&mut handler, |handler| {
            hand(handler, &pixel(10));
            panic!("the copy broke");
        });
        assert_eq!(
            handed.try_iter().collect::<Vec<_>>(),
            [
                Ok(10),
                Err("NotConnected: the capture panicked: the copy broke".to_owned())
            ]
        );
    }

    #[test]
    fn a_capture_that_ends_without_a_panic_tells_its_handler_nothing_more() {
        let (mut handler, handed) = recording();
        run_capture(&mut handler, |handler| hand(handler, &pixel(10)));
        assert_eq!(handed.try_iter().collect::<Vec<_>>(), [Ok(10)]);
    }

    #[test]
    fn a_handler_that_panics_is_not_called_again() {
        let (mut handler, handed) = recording();
        let unwound = panic::catch_unwind(AssertUnwindSafe(|| {
            run_capture(&mut handler, |handler| {
                hand(handler, &pixel(10));
                hand(handler, &pixel(0));
            });
        }));
        let payload = unwound.expect_err("the handler's panic unwinds on");
        let reason = panic_reason(payload.as_ref());
        assert!(reason.contains("the handler broke"), "{reason}");
        assert_eq!(handed.try_iter().collect::<Vec<_>>(), [Ok(10)]);
    }

    #[test]
    fn a_panic_reason_is_its_message() {
        let fixed: Box<dyn Any + Send> = Box::new("fixed");
        let formatted: Box<dyn Any + Send> = Box::new(format!("formatted {}", 1));
        let other: Box<dyn Any + Send> = Box::new(1_u8);
        let reasons =
            [fixed, formatted, other].map(|payload| panic_reason(payload.as_ref()).to_owned());
        assert_eq!(reasons, ["fixed", "formatted 1", "an unknown reason"]);
    }

    #[test]
    fn a_grab_s_capture_waits_long_after_its_frame() {
        assert!(AFTER_GRAB >= Duration::from_secs(60));
    }

    #[test]
    fn a_frame_s_latency_adds_its_delay_to_the_time_since_it_arrived() {
        let size = Size {
            width: 1,
            height: 1,
        };
        let picture = Image::new(size, vec![0, 0, 0, u8::MAX]).unwrap();
        let frame = Frame {
            window_size: picture.size(),
            image: &picture,
            received: Instant::now(),
            delay: Duration::from_secs(60),
        };
        let latency = frame.latency();
        assert!(latency >= Duration::from_secs(60), "{latency:?}");
        assert!(latency < Duration::from_secs(61), "{latency:?}");
    }
}
