//! The capture interface: what a capture copies out of each frame, the frame it
//! lends to its handler, and the [`Capture`] trait that the operating system's
//! capture implements, with a stand-in for systems that cannot capture.

use std::convert::Infallible;
use std::io;
use std::time::{Duration, Instant};

use super::{Image, Region, Size};
use crate::system::Unsupported;

/// What a capture copies out of each frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Crop {
    /// The whole frame, as the setup app's preview and screenshot need.
    Whole,
    /// One region, as a screen binding reads.
    Region(Region),
}

/// One captured frame's pixels, lent to the frame handler. The image is
/// reused for the next frame, so a handler copies what it keeps.
#[derive(Debug)]
pub struct Frame<'buffer> {
    /// The size of the window's whole frame, which a region's coordinates
    /// refer to.
    pub window_size: Size,
    /// The cropped pixels.
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
    /// `throttle`, which saves the system's own work as well as ours.
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
