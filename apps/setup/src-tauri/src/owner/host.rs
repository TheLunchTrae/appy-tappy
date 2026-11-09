//! What the owner needs from the operating system: its windows, its processes
//! and a capture of one window, so the tests can stand in for it.

use std::io;
use std::time::{Duration, Instant};

use appytappy_config::anti_cheat::ProcessId;
use appytappy_config::app_file::BuildDef;
use appytappy_config::window_match::WindowMatch;
use appytappy_sources::screen::{Capture, CaptureSession, Crop, FrameHandler};
use appytappy_sources::system::{
    Pick, ProcessEntry, SessionWindow, System, WindowEntry, WindowPlace, session_window, shows,
};

/// What the setup app needs from the operating system: its windows and
/// processes, a capture of one window, and a way to tell the runtime that a
/// save changed what it reads. It lives as long as the owner, which sends
/// itself work that names it.
pub(crate) trait Host: 'static {
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

    /// The window among `processes` and `windows` that a new session would
    /// read for `window`, the one [`session_window`] picks.
    fn read_window(
        &self,
        window: &WindowMatch,
        processes: Vec<ProcessEntry>,
        windows: Vec<WindowEntry>,
    ) -> Option<SessionWindow> {
        session_window(window, Pick::Any, processes, windows, |handle| {
            shows(self.window_place(handle))
        })
    }

    /// The build of `process`'s executable, read without opening it.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn executable_build(&self, process: ProcessId) -> io::Result<BuildDef>;

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

    /// The time now. The tests set it, to settle a window size without
    /// waiting.
    fn now(&self) -> Instant;

    /// Asks the runtime to read the saved files again. A runtime that is not
    /// running reads them when it starts, and one that cannot be reached
    /// keeps what it read, so the save stands either way.
    fn reload_runtime(&self);
}

impl<T: System + Capture + 'static> Host for T {
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

    fn executable_build(&self, process: ProcessId) -> io::Result<BuildDef> {
        System::executable_build(self, process)
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

    #[mutants::skip] // Sets a named Windows event, which no test can observe.
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn reload_runtime(&self) {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the save already happened; the runtime picks it up when it next starts"
        )]
        let _ = appytappy_platform::request_reload(appytappy_platform::RELOAD);
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
        assert!(Host::executable_build(&unsupported, ProcessId::new(7)).is_err());
        let handler: FrameHandler = Box::new(|_frame| Duration::ZERO);
        assert!(Host::capture(&unsupported, 7, Crop::Whole, PREVIEW_INTERVAL, handler).is_err());
    }
}
