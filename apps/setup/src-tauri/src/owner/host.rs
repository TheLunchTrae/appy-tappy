//! What the owner needs from the operating system and the app around it: the
//! system's windows and processes, a capture of one window, a click sent on
//! request, the runtime, and the overlays' windows, so the tests can stand in
//! for them.

use std::io;
use std::time::{Duration, Instant};

use appytappy_actions::{HostInput, PlacedClick, Watched};
use appytappy_config::app_file::BuildDef;
use appytappy_config::click::{ClickButton, ClickDelivery};
use appytappy_config::process::ProcessId;
use appytappy_config::window_match::WindowMatch;
use appytappy_platform::{REQUESTS, Request};
use appytappy_sources::HostSystem;
use appytappy_sources::screen::place::{Target, place};
use appytappy_sources::screen::{Capture, CaptureSession, Crop, FrameHandler};
use appytappy_sources::system::{
    Pick, ProcessEntry, SessionWindow, System, WindowEntry, WindowPlace, session_window, shows,
};
use tauri::{AppHandle, Runtime, Wry};

use crate::overlay::{self, OverlayWindow};
use crate::settings::RUNTIME_PROGRAM;

/// What the setup app calls itself where the runtime names the watched
/// application, such as in why a click did not click.
const SETUP_APP: &str = "AppyTappy Setup";

/// A click to place on the screen and send now.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ClickNow<'click> {
    /// The name of the application it clicks in, for messages.
    pub(crate) app: &'click str,
    /// How to find that application's window.
    pub(crate) window: &'click WindowMatch,
    /// What it looks for in the window.
    pub(crate) target: Target<'click>,
    /// Which button, and how many times.
    pub(crate) button: ClickButton,
    /// How it reaches the window.
    pub(crate) delivery: ClickDelivery,
}

/// The window a click sent on request went to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClickedWindow {
    /// The window's handle.
    pub(crate) handle: u64,
    /// Its title.
    pub(crate) title: String,
}

/// What the setup app needs from the operating system: its windows and
/// processes, a capture of one window, a click sent on request, a way to
/// tell the runtime that a save changed what it reads or that a request to
/// run an automation once waits, and the overlays' windows. It lives as long as the owner, which sends itself work that
/// names it.
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

    /// Places `click` on the screen and sends it, both the way the runtime
    /// does. The setup app stands where the runtime has the watched
    /// application: a click with the cursor clicks only while the setup
    /// app's window or the window it clicks in is in front, and gives the
    /// focus back to the one that was. Answers the window it went to, which
    /// is the one the application's window rules find now.
    ///
    /// # Errors
    ///
    /// Returns why the click was not placed or not sent, as a clause.
    fn click(&self, click: ClickNow<'_>) -> Result<ClickedWindow, String>;

    /// The time now. The tests set it, to settle a window size without
    /// waiting.
    fn now(&self) -> Instant;

    /// Asks the runtime to read the saved files again. A runtime that is not
    /// running reads them when it starts, and one that cannot be reached
    /// keeps what it read, so the save stands either way.
    fn reload_runtime(&self);

    /// Tells the runtime that a request to run an automation once waits in
    /// its file, and answers whether a runtime heard it: one that is not
    /// running hears nothing, so the request has to be taken away again.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn run_once(&self) -> io::Result<bool>;

    /// Whether the runtime is running, as the list of running processes
    /// tells by its program's name. The check opens neither the runtime nor
    /// its single-instance guard, so a runtime that starts meanwhile still
    /// takes the guard.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn runtime_runs(&self) -> io::Result<bool>;

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

    fn click(&self, click: ClickNow<'_>) -> Result<ClickedWindow, String> {
        let found = place(&self.system, click.app, click.window, click.target)?;
        let placed = PlacedClick {
            window: found.window,
            process: found.process,
            app: click.app.to_owned(),
            at: found.at,
            button: click.button,
            delivery: click.delivery,
        };
        let asking = Watched {
            process: ProcessId::new(std::process::id()),
            executable: SETUP_APP.to_owned(),
        };
        appytappy_actions::click(&mut HostInput::default(), &placed, &asking)
            .map_err(|error| error.to_string())?;
        Ok(ClickedWindow {
            handle: found.window,
            title: found.title,
        })
    }

    fn now(&self) -> Instant {
        Instant::now()
    }

    #[mutants::skip] // Sets a named Windows event, which no test can observe.
    fn reload_runtime(&self) {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the save already happened, and a runtime that missed the signal reads it when it starts; the main pages ask whether it runs"
        )]
        let _ = appytappy_platform::request(REQUESTS, Request::Reload);
    }

    #[mutants::skip] // Sets a named Windows event, which no test can observe.
    fn run_once(&self) -> io::Result<bool> {
        appytappy_platform::request(REQUESTS, Request::RunOnce)
    }

    fn runtime_runs(&self) -> io::Result<bool> {
        self.system
            .processes()
            .map(|processes| runtime_among(&processes))
    }

    #[mutants::skip] // Opens and closes windows, which needs a running app.
    fn sync_overlay_windows(&self, wanted: &[OverlayWindow]) -> Result<(), String> {
        overlay::window::sync(&self.app, wanted)
    }
}

/// Whether the runtime's program is among `processes`. Its name is the
/// runtime's whoever started it, but the list holds every user's
/// processes, so another user's runtime on the same PC counts too.
fn runtime_among(processes: &[ProcessEntry]) -> bool {
    processes
        .iter()
        .any(|process| process.executable.eq_ignore_ascii_case(RUNTIME_PROGRAM))
}

#[cfg(test)]
mod tests {
    use appytappy_sources::screen::NextFrame;

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
        assert!(desktop.runtime_runs().is_err());
        // Run requests go through the platform's own named event, which
        // works on Windows whatever `system` says.
        #[cfg(not(windows))]
        assert!(desktop.run_once().is_err());
        let handler: FrameHandler = Box::new(|_frame| NextFrame::After(Duration::ZERO));
        assert!(
            desktop
                .capture(7, Crop::Whole, PREVIEW_INTERVAL, handler)
                .is_err()
        );
        let window = WindowMatch::exact(appytappy_config::window_match::WindowFacts {
            title: "Example App",
            class: "ExampleWindow",
            executable: "Example.exe",
        });
        let spot =
            appytappy_config::click::Spot::new(appytappy_config::click::Corner::TopLeft, 1, 1)
                .expect("valid");
        let click = ClickNow {
            app: "Example App",
            window: &window,
            target: Target::Spot(spot),
            button: ClickButton::Left,
            delivery: ClickDelivery::Message,
        };
        assert!(
            desktop
                .click(click)
                .expect_err("no windows here")
                .starts_with("Example App's window cannot be found: "),
            "a click is placed before it is sent"
        );
    }

    #[test]
    fn the_runtime_runs_while_its_program_is_listed() {
        let process = |executable: &str| ProcessEntry {
            id: ProcessId::new(7),
            parent: ProcessId::new(1),
            executable: executable.to_owned(),
            threads: 1,
        };
        assert!(!runtime_among(&[]));
        assert!(!runtime_among(&[
            process("Example.exe"),
            process("AppyTappy Setup.exe")
        ]));
        assert!(runtime_among(&[
            process("Example.exe"),
            process("AppyTappy-Runtime.EXE")
        ]));
    }
}
