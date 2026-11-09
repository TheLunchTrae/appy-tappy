//! What the owner needs from the operating system and the app around it: the
//! system's windows and processes, a capture of one window, the runtime, and
//! the overlays' windows, so the tests can stand in for them.

use std::io;
use std::time::{Duration, Instant};

use appytappy_config::anti_cheat::ProcessId;
use appytappy_config::app_file::BuildDef;
use appytappy_config::window_match::WindowMatch;
use appytappy_sources::HostSystem;
use appytappy_sources::screen::{Capture, CaptureSession, Crop, FrameHandler};
use appytappy_sources::system::{
    Pick, ProcessEntry, SessionWindow, System, WindowEntry, WindowPlace, session_window, shows,
};
use tauri::{AppHandle, Runtime, Wry};

use crate::overlay::{self, OverlayWindow};

/// What the setup app needs from the operating system: its windows and
/// processes, a capture of one window, a way to tell the runtime that a
/// save changed what it reads, and the overlays' windows. It lives as long
/// as the owner, which sends itself work that names it.
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

    /// Opens a window for each overlay in `wanted` that has none, and
    /// closes the overlay windows nothing wants any more.
    ///
    /// # Errors
    ///
    /// Returns why each window that could not open or close could not.
    fn sync_overlay_windows(&self, wanted: &[OverlayWindow]) -> Result<(), String>;
}

/// The setup app's host: the operating system, and the running app whose
/// windows show the overlays.
#[derive(Debug)]
pub(crate) struct Desktop<S = HostSystem, R: Runtime = Wry> {
    /// The operating system.
    pub(crate) system: S,
    /// The running app.
    pub(crate) app: AppHandle<R>,
}

impl<S: System + Capture + 'static, R: Runtime> Host for Desktop<S, R> {
    type Session = S::Session;

    fn windows(&self) -> io::Result<Vec<WindowEntry>> {
        self.system.windows()
    }

    fn processes(&self) -> io::Result<Vec<ProcessEntry>> {
        self.system.processes()
    }

    fn window_place(&self, window: u64) -> io::Result<Option<WindowPlace>> {
        self.system.window_place(window)
    }

    fn executable_build(&self, process: ProcessId) -> io::Result<BuildDef> {
        self.system.executable_build(process)
    }

    fn capture(
        &self,
        window: u64,
        crop: Crop,
        interval: Duration,
        handler: FrameHandler,
    ) -> io::Result<Self::Session> {
        self.system.start(window, crop, interval, handler)
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

    #[mutants::skip] // Opens and closes windows, which needs a running app.
    fn sync_overlay_windows(&self, wanted: &[OverlayWindow]) -> Result<(), String> {
        overlay::window::sync(&self.app, wanted)
    }
}

#[cfg(test)]
mod tests {
    use crate::owner::PREVIEW_INTERVAL;

    use super::*;

    #[test]
    fn the_desktop_lists_and_captures_through_the_platform() {
        let desktop = Desktop {
            system: appytappy_sources::system::Unsupported,
            app: tauri::test::mock_app().handle().clone(),
        };
        assert!(desktop.windows().is_err());
        assert!(desktop.processes().is_err());
        assert!(desktop.window_place(7).is_err());
        assert!(desktop.executable_build(ProcessId::new(7)).is_err());
        let handler: FrameHandler = Box::new(|_frame| Duration::ZERO);
        assert!(
            desktop
                .capture(7, Crop::Whole, PREVIEW_INTERVAL, handler)
                .is_err()
        );
    }
}
