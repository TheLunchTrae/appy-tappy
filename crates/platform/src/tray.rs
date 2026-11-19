//! The tray icon, its tooltip, and its right-click menu.

use std::io;
use std::num::NonZeroU16;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

use crate::host;

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
    /// installer that replaces the program, or signing out does. When the
    /// session ends, the tray holds it until [`Tray::stop`], for at most 10
    /// seconds and only as long as Windows waits, so the owner stops the
    /// tray only once it has undone what it changed.
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

    /// Lets the end of the session go on if the tray holds it, then
    /// removes the icon, ends the thread, and waits for it.
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
pub(crate) fn menu_label(label: &str) -> String {
    label.replace('&', "&&").replace('\t', " ")
}

/// The UTF-16 units of the longest start of `text` that fits a tooltip,
/// without splitting a character.
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the Windows adapter shows tooltips")
)]
pub(crate) fn tooltip_units(text: &str) -> Vec<u16> {
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

/// Holds the end of the session: sends the owner thread the close request
/// with `close`, which answers whether anyone received it, and then waits
/// at most `wait` until `done` disconnects, which the owner thread does
/// once it has undone what the automations changed. With nobody to receive
/// the request, nothing would disconnect `done`, so it does not wait.
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the Windows adapter hears the session end")
)]
pub(crate) fn hold_end_of_session(
    close: impl FnOnce() -> bool,
    done: &Receiver<()>,
    wait: Duration,
) {
    if close() {
        // Every outcome, the owner thread's word included, ends the wait; it
        // never sends, it only drops the sender.
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a disconnect is the owner thread's word and a timeout the bound; both end the wait the same way"
        )]
        let _ = done.recv_timeout(wait);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Instant;

    use super::*;

    #[test]
    fn the_end_of_the_session_waits_only_for_an_owner_that_heard_it() {
        let long = Duration::from_secs(60);
        let (_kept, done) = mpsc::channel();
        let start = Instant::now();
        hold_end_of_session(|| false, &done, long);
        assert!(start.elapsed() < long, "nobody heard it, so nothing waits");

        let (finished, done) = mpsc::channel::<()>();
        drop(finished);
        let mut heard = false;
        hold_end_of_session(
            || {
                heard = true;
                true
            },
            &done,
            long,
        );
        assert!(heard, "the close request is sent");
        assert!(
            start.elapsed() < long,
            "the owner's disconnect ends the wait"
        );

        let (_kept, done) = mpsc::channel();
        let short = Duration::from_millis(20);
        let held = Instant::now();
        hold_end_of_session(|| true, &done, short);
        assert!(
            held.elapsed() >= short,
            "an owner that heard it is waited for"
        );
    }

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
}
