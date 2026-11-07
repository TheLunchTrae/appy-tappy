//! The runtime's Windows host services: the window events that drive app
//! detection, the tray icon and its menu, the single-instance guard, the
//! reload requests the setup app sends, autostart at sign-in, and the local
//! clock.
//!
//! Each service turns Windows API calls into a typed value or an error and
//! makes no decisions of its own. The Windows adapter lives in `win`; other
//! platforms get services that fail with [`io::ErrorKind::Unsupported`], so
//! the portable code that uses them still builds and tests there.
//!
//! See `docs/architecture.md` for this crate's boundaries.

use std::any::Any;
use std::ffi::OsStr;
use std::fmt;
use std::io;
use std::num::NonZeroU16;
use std::sync::mpsc::Sender;

#[cfg(not(windows))]
mod unsupported;
#[cfg(windows)]
mod win;

#[cfg(not(windows))]
use unsupported as host;
#[cfg(windows)]
use win as host;

/// A change to the set of windows on the desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowEvent {
    /// A top-level window was created.
    Created,
    /// A window was destroyed. Windows cannot tell whether a destroyed
    /// window was top-level, so this covers child windows too.
    Destroyed,
}

/// A thread that reports [`WindowEvent`]s for every process on the desktop.
///
/// The hook it installs never opens a process, so it is safe for every
/// anti-cheat class. Its thread ends when [`WindowWatch::stop`] runs, or
/// when the next event arrives after the receiver was dropped.
#[derive(Debug)]
pub struct WindowWatch(host::WindowWatch);

impl WindowWatch {
    /// Installs the hook on a new thread and returns once it is in place.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error when the thread or the hook
    /// cannot start.
    pub fn start<E>(events: Sender<E>) -> io::Result<Self>
    where
        E: From<WindowEvent> + Send + 'static,
    {
        host::WindowWatch::start(events).map(Self)
    }

    /// Removes the hook, ends the thread, and waits for it.
    ///
    /// # Errors
    ///
    /// Returns the error that ended the thread's message loop, if any.
    // Off Windows no watch exists, so no test there can reach this body; the
    // Windows adapter's tests call it.
    #[mutants::skip]
    pub fn stop(self) -> io::Result<()> {
        self.0.stop()
    }
}

/// The tray icon's tooltip and right-click menu. The caller decides what
/// they say; the tray only shows them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayMenu {
    /// The text shown when the pointer rests on the icon. Windows shows at
    /// most 127 UTF-16 units of it.
    pub tooltip: String,
    /// The menu's items, top to bottom.
    pub items: Vec<MenuItem>,
}

/// One line of the tray menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuItem {
    /// A line the user can pick. Picking it sends [`TrayEvent::Picked`]
    /// with its `id`.
    Action {
        /// What a pick reports.
        id: NonZeroU16,
        /// The line's text.
        label: String,
        /// Whether the line shows a check mark.
        checked: bool,
    },
    /// A line that shows text and does nothing when picked.
    Note(String),
    /// A grayed-out line.
    Inactive(String),
    /// A line that opens a nested menu.
    Submenu {
        /// The line's text.
        label: String,
        /// The nested menu's items.
        items: Vec<MenuItem>,
    },
    /// A dividing line.
    Separator,
}

/// What the tray reports to its owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayEvent {
    /// The user picked the [`MenuItem::Action`] with this id.
    Picked(NonZeroU16),
    /// Windows asked the program to close, as `taskkill` without `/F`, an
    /// installer that replaces the program, or signing out does.
    CloseRequested,
    /// The tray stopped before [`Tray::stop`] for this reason, and its icon
    /// is gone.
    Ended(String),
}

/// The tray icon, with its own thread and hidden window.
///
/// Right-clicking the icon opens the menu, and each [`TrayEvent`] goes to
/// the sender given to [`Tray::start`]. When Explorer restarts, the icon
/// comes back.
#[derive(Debug)]
pub struct Tray(host::Tray);

impl Tray {
    /// Starts the tray thread and returns once its window exists.
    ///
    /// The icon may appear later: when Explorer is not running yet, as at
    /// sign-in, the icon is added when the taskbar starts.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error when the thread or the window
    /// cannot start.
    pub fn start<E>(events: Sender<E>, menu: TrayMenu) -> io::Result<Self>
    where
        E: From<TrayEvent> + Send + 'static,
    {
        host::Tray::start(events, menu).map(Self)
    }

    /// Replaces the tooltip and the menu. A menu that is open keeps what it
    /// showed; the next one shows the new items.
    ///
    /// # Errors
    ///
    /// Returns an error when the tray thread has ended.
    // Off Windows no tray exists, so no test there can reach this body; the
    // Windows adapter's tests call it.
    #[mutants::skip]
    pub fn show(&self, menu: TrayMenu) -> io::Result<()> {
        self.0.show(menu)
    }

    /// Removes the icon, ends the thread, and waits for it.
    ///
    /// # Errors
    ///
    /// Returns the error that ended the tray's message loop, if any.
    // Off Windows no tray exists, so no test there can reach this body; the
    // Windows adapter's tests call it.
    #[mutants::skip]
    pub fn stop(self) -> io::Result<()> {
        self.0.stop()
    }
}

/// A menu label as Windows should show it: a lone `&` would underline the
/// next letter and a tab would split the line, so `&` is doubled and a tab
/// becomes a space.
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the Windows adapter draws menus")
)]
fn menu_label(label: &str) -> String {
    label.replace('&', "&&").replace('\t', " ")
}

/// The UTF-16 units of the longest start of `text` that fits a tooltip,
/// without splitting a character.
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the Windows adapter shows tooltips")
)]
fn tooltip_units(text: &str) -> Vec<u16> {
    /// Windows shows at most this many UTF-16 units of a tooltip.
    const LIMIT: usize = 127;
    let mut units = Vec::with_capacity(LIMIT);
    let mut buffer = [0; 2];
    for character in text.chars() {
        let encoded = character.encode_utf16(&mut buffer);
        if units.len().saturating_add(encoded.len()) > LIMIT {
            break;
        }
        units.extend_from_slice(encoded);
    }
    units
}

/// Proof that this is the only running copy of a program in the user's
/// session. The guard lasts until the value is dropped or the process ends.
#[derive(Debug)]
pub struct InstanceGuard {
    _held: host::InstanceGuard,
}

impl InstanceGuard {
    /// The name of the runtime's guard. The setup app can check for it to
    /// learn whether the runtime is running.
    pub const RUNTIME: &str = r"Local\AppyTappy.Runtime";

    /// Takes the guard called `name`, or returns `None` when another holder
    /// has it.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    pub fn acquire(name: &str) -> io::Result<Option<Self>> {
        host::InstanceGuard::acquire(name).map(|guard| guard.map(|held| Self { _held: held }))
    }
}

/// The name of the event the runtime waits on for reload requests. The
/// setup app sets it after each save, so the runtime reads the saved files
/// again.
pub const RELOAD: &str = r"Local\AppyTappy.Reload";

/// A request to read the saved files again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReloadRequest;

/// A thread that sends a [`ReloadRequest`] each time another program calls
/// [`request_reload`] with the same name.
///
/// Requests that arrive while one waits to be sent become one request. It
/// never polls: the thread sleeps until a request or [`ReloadWatch::stop`].
#[derive(Debug)]
pub struct ReloadWatch(host::ReloadWatch);

impl ReloadWatch {
    /// Starts waiting for requests to `name`, such as [`RELOAD`], and returns
    /// once the thread waits.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error when the thread or its event
    /// cannot start.
    pub fn start<E>(name: &str, events: Sender<E>) -> io::Result<Self>
    where
        E: From<ReloadRequest> + Send + 'static,
    {
        host::ReloadWatch::start(name, events).map(Self)
    }

    /// Ends the thread and waits for it.
    ///
    /// # Errors
    ///
    /// Returns the error that ended the thread's wait, if any.
    // Off Windows no watch exists, so no test there can reach this body; the
    // Windows adapter's tests call it.
    #[mutants::skip]
    pub fn stop(self) -> io::Result<()> {
        self.0.stop()
    }
}

/// Asks the program waiting on `name`, such as [`RELOAD`], to reload.
/// Returns whether one was waiting; when none is, nothing happens.
///
/// # Errors
///
/// Returns the operating system's error.
pub fn request_reload(name: &str) -> io::Result<bool> {
    host::request_reload(name)
}

/// The value under the user's `Run` registry key that starts the runtime at
/// sign-in.
pub const AUTOSTART_VALUE: &str = "AppyTappy";

/// Makes Windows run `command` when the user signs in, through the `Run`
/// key value `value`. It replaces any command the value held. The command
/// is a whole command line, with the program's path quoted.
///
/// # Errors
///
/// Returns the operating system's error.
pub fn enable_autostart(value: &str, command: &OsStr) -> io::Result<()> {
    host::enable_autostart(value, command)
}

/// Removes the `Run` key value `value`. A value that is already absent is
/// not an error.
///
/// # Errors
///
/// Returns the operating system's error.
pub fn disable_autostart(value: &str) -> io::Result<()> {
    host::disable_autostart(value)
}

/// The message a panicking thread left, for the error that reports it.
#[must_use]
pub fn panic_reason(payload: &(dyn Any + Send)) -> &str {
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("an unknown reason")
}

/// A wall-clock time in the user's time zone, to the second.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    /// The year, such as 2026.
    pub year: u16,
    /// The month, from 1.
    pub month: u16,
    /// The day of the month, from 1.
    pub day: u16,
    /// The hour, from 0 to 23.
    pub hour: u16,
    /// The minute.
    pub minute: u16,
    /// The second.
    pub second: u16,
}

impl LocalTime {
    /// The current local time, or `None` where AppyTappy does not run.
    // Off Windows this body already returns `None`, so the mutation testing
    // there cannot tell a mutant from it.
    #[mutants::skip]
    #[must_use]
    pub fn now() -> Option<Self> {
        host::local_time()
    }
}

impl fmt::Display for LocalTime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_labels_show_ampersands_and_tabs_literally() {
        assert_eq!(menu_label("Rock & Roll\tnow"), "Rock && Roll now");
    }

    #[test]
    fn a_tooltip_keeps_whole_characters_within_the_limit() {
        let short = tooltip_units("AppyTappy: idle");
        assert_eq!(short, "AppyTappy: idle".encode_utf16().collect::<Vec<_>>());
        let long = format!("{}\u{1F3AE}", "a".repeat(126));
        let cut = tooltip_units(&long);
        assert_eq!(cut.len(), 126, "the two-unit emoji would pass 127");
        let exact = "a".repeat(127);
        assert_eq!(tooltip_units(&exact).len(), 127);
        assert_eq!(tooltip_units(&"a".repeat(200)).len(), 127);
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
    fn local_time_reads_year_first_with_padded_fields() {
        let time = LocalTime {
            year: 2026,
            month: 11,
            day: 7,
            hour: 3,
            minute: 5,
            second: 9,
        };
        assert_eq!(time.to_string(), "2026-11-07 03:05:09");
    }
}
