//! The capture interface: what a capture copies out of each frame, the frame it
//! lends to its handler, and the [`Capture`] trait that the operating system's
//! capture implements, with a stand-in for systems that cannot capture.

use std::convert::Infallible;
use std::io;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::{Image, Layout, Region, Size};
use crate::system::Unsupported;

/// What a capture copies out of each frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Crop {
    /// The whole frame, as the setup app's preview and screenshot need.
    Whole,
    /// The regions a layout packs, as the values of a window read. The
    /// frame's image is then the packed image.
    Layout(Layout),
}

/// One captured frame's pixels, lent to the frame handler. The image is
/// reused for the next frame, so a handler copies what it keeps.
#[derive(Debug)]
pub struct Frame<'buffer> {
    /// The size of the window's whole frame, which a region's coordinates
    /// refer to.
    pub window_size: Size,
    /// The cropped pixels: the whole frame, or the packed image of the
    /// capture's [`Layout`].
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
/// thread.
pub type FrameHandler = Box<dyn FnMut(io::Result<Frame<'_>>) -> Duration + Send>;

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
