//! What key presses and clicks act on: the keyboard, the mouse, and the
//! windows that [`Input`] reads and sends to.

use std::io;
use std::num::NonZeroU32;
use std::time::Duration;

use appytappy_config::action::Modifier;
use appytappy_config::anti_cheat::ProcessId;

use crate::{MouseMessage, Press, Stroke};

/// How Windows scales a window.
///
/// The window draws at one number of dots per inch, and its monitor shows
/// another when the window's program does not follow the monitor's scale.
/// Windows then stretches the window, and the window counts in its own
/// pixels, not the screen's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scale {
    /// The dots per inch the window draws at.
    pub window: NonZeroU32,
    /// The dots per inch of the monitor it shows on.
    pub monitor: NonZeroU32,
}

/// The operating system's keyboard, mouse, and windows, as key presses and
/// clicks use them. Each method reads one thing or sends one thing, and
/// makes no choices.
pub trait Input {
    /// The process whose window has the keyboard focus, or `None` when no
    /// window has it.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn focused_process(&mut self) -> io::Result<Option<ProcessId>>;

    /// The modifiers the user is holding down now.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn held_modifiers(&mut self) -> io::Result<Vec<Modifier>>;

    /// Sends `strokes`, in order, to whichever window has the keyboard
    /// focus. Answers how many of them the operating system took, from the
    /// first; it takes fewer when another program blocks input.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn send_strokes(&mut self, strokes: &[Stroke]) -> io::Result<usize>;

    /// The window in front, the one with the keyboard focus, or `None` when
    /// no window has it.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn front_window(&mut self) -> io::Result<Option<u64>>;

    /// The top-level window that shows at (`x`, `y`) in screen pixels, the
    /// one a click there would land in, or `None` where none shows.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn window_at(&mut self, x: i32, y: i32) -> io::Result<Option<u64>>;

    /// Asks for `window` to come to the front with the keyboard focus.
    /// Returns whether it is in front after.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn bring_to_front(&mut self, window: u64) -> io::Result<bool>;

    /// Where the cursor is, in screen pixels.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn cursor(&mut self) -> io::Result<(i32, i32)>;

    /// Moves the cursor to (`x`, `y`) in screen pixels.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error, such as when another program
    /// blocks input.
    fn move_cursor(&mut self, x: i32, y: i32) -> io::Result<()>;

    /// Whether a mouse button is held down now.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn buttons_held(&mut self) -> io::Result<bool>;

    /// Sends `presses`, in order, where the cursor is. Answers how many of
    /// them the operating system took, from the first; it takes fewer when
    /// another program blocks input.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn send_presses(&mut self, presses: &[Press]) -> io::Result<usize>;

    /// Waits for `time`, while pressed keys or buttons are held down.
    fn hold(&mut self, time: Duration);

    /// The screen place (`x`, `y`) as `window` counts it: from the top-left
    /// corner of its client area, in screen pixels. A place outside the
    /// client area has a coordinate below zero or past the client area's
    /// size.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error, including for a window that
    /// closed.
    fn client_point(&mut self, window: u64, x: i32, y: i32) -> io::Result<(i32, i32)>;

    /// The width and height of `window`'s client area, in screen pixels.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error, including for a window that
    /// closed.
    fn client_size(&mut self, window: u64) -> io::Result<(i32, i32)>;

    /// The child window of `window` at (`x`, `y`) in `window`'s client
    /// area, the topmost one that is visible, enabled, and not transparent
    /// to the mouse; `window` itself where it has none; or `None` for a
    /// place outside `window`. It looks only at `window`'s own children,
    /// not at theirs.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn child_at(&mut self, window: u64, x: i32, y: i32) -> io::Result<Option<u64>>;

    /// How Windows scales `window`.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error, including for a window that
    /// closed.
    fn scale(&mut self, window: u64) -> io::Result<Scale>;

    /// Puts `message` in `window`'s message queue and returns without
    /// waiting for the window to read it.
    ///
    /// # Errors
    ///
    /// Returns [`io::ErrorKind::PermissionDenied`] when Windows blocks
    /// messages to the window, because its program runs with more rights
    /// than this one, such as as administrator, and the operating system's
    /// error otherwise, including for a window that closed.
    fn post(&mut self, window: u64, message: MouseMessage) -> io::Result<()>;
}

/// The input of a platform AppyTappy does not support: every call fails
/// with [`io::ErrorKind::Unsupported`], and [`Input::hold`] does not wait.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Unsupported;

fn unsupported<T>() -> io::Result<T> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "AppyTappy presses keys and clicks on Windows only",
    ))
}

impl Input for Unsupported {
    fn focused_process(&mut self) -> io::Result<Option<ProcessId>> {
        unsupported()
    }

    fn held_modifiers(&mut self) -> io::Result<Vec<Modifier>> {
        unsupported()
    }

    fn send_strokes(&mut self, _strokes: &[Stroke]) -> io::Result<usize> {
        unsupported()
    }

    fn front_window(&mut self) -> io::Result<Option<u64>> {
        unsupported()
    }

    fn window_at(&mut self, _x: i32, _y: i32) -> io::Result<Option<u64>> {
        unsupported()
    }

    fn bring_to_front(&mut self, _window: u64) -> io::Result<bool> {
        unsupported()
    }

    fn cursor(&mut self) -> io::Result<(i32, i32)> {
        unsupported()
    }

    fn move_cursor(&mut self, _x: i32, _y: i32) -> io::Result<()> {
        unsupported()
    }

    fn buttons_held(&mut self) -> io::Result<bool> {
        unsupported()
    }

    fn send_presses(&mut self, _presses: &[Press]) -> io::Result<usize> {
        unsupported()
    }

    fn hold(&mut self, _time: Duration) {}

    fn client_point(&mut self, _window: u64, _x: i32, _y: i32) -> io::Result<(i32, i32)> {
        unsupported()
    }

    fn client_size(&mut self, _window: u64) -> io::Result<(i32, i32)> {
        unsupported()
    }

    fn child_at(&mut self, _window: u64, _x: i32, _y: i32) -> io::Result<Option<u64>> {
        unsupported()
    }

    fn scale(&mut self, _window: u64) -> io::Result<Scale> {
        unsupported()
    }

    fn post(&mut self, _window: u64, _message: MouseMessage) -> io::Result<()> {
        unsupported()
    }
}

#[cfg(test)]
mod tests {
    use appytappy_config::action::Key;

    use super::*;
    use crate::{MouseButton, MouseEvent};

    #[test]
    fn unsupported_input_fails_every_call() {
        let mut input = Unsupported;
        let message = MouseMessage {
            event: MouseEvent::Move,
            at: (0, 0),
        };
        let press = Press {
            button: MouseButton::Left,
            down: true,
        };
        let kinds = [
            input.focused_process().map(drop),
            input.held_modifiers().map(drop),
            input
                .send_strokes(&[Stroke::Key(Key::Enter, true)])
                .map(drop),
            input.front_window().map(drop),
            input.window_at(0, 0).map(drop),
            input.bring_to_front(7).map(drop),
            input.cursor().map(drop),
            input.move_cursor(0, 0),
            input.buttons_held().map(drop),
            input.send_presses(&[press]).map(drop),
            input.client_point(7, 0, 0).map(drop),
            input.client_size(7).map(drop),
            input.child_at(7, 0, 0).map(drop),
            input.scale(7).map(drop),
            input.post(7, message),
        ]
        .map(|result| result.expect_err("unsupported").kind());
        assert_eq!(kinds, [io::ErrorKind::Unsupported; 15]);
    }
}
