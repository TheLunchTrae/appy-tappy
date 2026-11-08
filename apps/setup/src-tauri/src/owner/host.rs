//! What the owner needs from the operating system: its windows, its processes
//! and a capture of one window, so the tests can stand in for it.

use std::io;
use std::time::Duration;

use appytappy_sources::screen::{Capture, CaptureSession, Crop, FrameHandler};
use appytappy_sources::system::{ProcessEntry, System, WindowEntry, WindowPlace};

/// What the setup app needs from the operating system: its windows and
/// processes, and a capture of one window.
pub(crate) trait Host {
    /// A running capture.
    type Session: CaptureSession;

    /// Every top-level window.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn windows(&self) -> io::Result<Vec<WindowEntry>>;

    /// Every running process.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn processes(&self) -> io::Result<Vec<ProcessEntry>>;

    /// Where `window` is and whether it shows, or `None` once it closed.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn window_place(&self, window: u64) -> io::Result<Option<WindowPlace>>;

    /// Starts capturing `crop` of `window`, calling `handler` with frames at
    /// most every `interval`.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error when the capture cannot start.
    fn capture(
        &self,
        window: u64,
        crop: Crop,
        interval: Duration,
        handler: FrameHandler,
    ) -> io::Result<Self::Session>;
}

impl<T: System + Capture> Host for T {
    type Session = T::Session;

    fn windows(&self) -> io::Result<Vec<WindowEntry>> {
        System::windows(self)
    }

    fn processes(&self) -> io::Result<Vec<ProcessEntry>> {
        System::processes(self)
    }

    fn window_place(&self, window: u64) -> io::Result<Option<WindowPlace>> {
        System::window_place(self, window)
    }

    fn capture(
        &self,
        window: u64,
        crop: Crop,
        interval: Duration,
        handler: FrameHandler,
    ) -> io::Result<Self::Session> {
        self.start(window, crop, interval, handler)
    }
}

#[cfg(test)]
mod tests {
    use crate::owner::PREVIEW_INTERVAL;

    use super::*;

    #[test]
    fn the_host_system_lists_and_captures_through_the_platform() {
        let unsupported = appytappy_sources::system::Unsupported;
        assert!(Host::windows(&unsupported).is_err());
        assert!(Host::processes(&unsupported).is_err());
        assert!(Host::window_place(&unsupported, 7).is_err());
        let handler: FrameHandler = Box::new(|_frame| Duration::ZERO);
        assert!(Host::capture(&unsupported, 7, Crop::Whole, PREVIEW_INTERVAL, handler).is_err());
    }
}
