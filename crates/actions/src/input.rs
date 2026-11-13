//! What key presses and clicks act on: the keyboard, the mouse, and the
//! windows that [`Input`] reads and sends to.

use std::io;
use std::time::{Duration, Instant};

use appytappy_config::action::Modifier;
use appytappy_config::process::ProcessId;

use crate::{MouseMessage, Press, Stroke};

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

    /// The time now, by a clock that never goes back.
    fn now(&mut self) -> Instant;

    /// The screen place (`x`, `y`), in screen pixels, in the pixels that
    /// `window`'s program counts the screen in. They differ for a window
    /// that Windows stretches, because its program does not follow its
    /// monitor's scale. Every other place and size of a window here is in
    /// that window's own pixels. The place has to lie on the window.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error, including for a window that
    /// closed.
    fn own_place(&mut self, window: u64, x: i32, y: i32) -> io::Result<(i32, i32)>;

    /// The screen place (`x`, `y`), as [`Input::own_place`] answers it, from
    /// the top-left corner of `window`'s client area. A place outside the
    /// client area has a coordinate below zero or past the client area's
    /// size.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error, including for a window that
    /// closed.
    fn client_point(&mut self, window: u64, x: i32, y: i32) -> io::Result<(i32, i32)>;

    /// The width and height of `window`'s client area.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error, including for a window that
    /// closed.
    fn client_size(&mut self, window: u64) -> io::Result<(i32, i32)>;

    /// The topmost child window of `window`, or `None` when it has none.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn top_child(&mut self, window: u64) -> io::Result<Option<u64>>;

    /// The window just below `window` among the children of its parent, or
    /// `None` for the lowest.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn below(&mut self, window: u64) -> io::Result<Option<u64>>;

    /// `window`'s style bits, as Windows keeps them, which say whether it
    /// is visible and whether it is disabled. A window that closed has
    /// none.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn style(&mut self, window: u64) -> io::Result<u32>;

    /// `window`'s left, top, right, and bottom edges on the screen, borders
    /// included, as [`Input::own_place`] answers places.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error, including for a window that
    /// closed.
    fn bounds(&mut self, window: u64) -> io::Result<(i32, i32, i32, i32)>;

    /// What `window` answers when Windows asks where on it the screen place
    /// `at` is, as it does to route the mouse (`WM_NCHITTEST`). It waits
    /// for the window's answer, at most `wait`.
    ///
    /// # Errors
    ///
    /// Returns [`io::ErrorKind::TimedOut`] when the window did not answer
    /// within `wait`, [`io::ErrorKind::PermissionDenied`] when Windows
    /// blocks messages to the window, as [`Input::post`] does, and the
    /// operating system's error otherwise.
    fn hit_test(&mut self, window: u64, at: (i16, i16), wait: Duration) -> io::Result<usize>;

    /// The style bits of `window`'s class, as Windows keeps them, which say
    /// whether the window is told about double clicks. A window that closed
    /// has none.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn class_style(&mut self, window: u64) -> io::Result<u32>;

    /// Puts `message` in `window`'s message queue and returns without
    /// waiting for the window to read it.
    ///
    /// # Errors
    ///
    /// Returns [`io::ErrorKind::PermissionDenied`] when Windows blocks
    /// messages to the window, because its program runs with more rights
    /// than this one, as one started as administrator does, and the
    /// operating system's error otherwise, including for a window that
    /// closed.
    fn post(&mut self, window: u64, message: MouseMessage) -> io::Result<()>;
}

/// The input of a platform AppyTappy does not support: every call fails
/// with [`io::ErrorKind::Unsupported`], except that [`Input::hold`] does
/// not wait and [`Input::now`] reads the clock.
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

    fn now(&mut self) -> Instant {
        Instant::now()
    }

    fn own_place(&mut self, _window: u64, _x: i32, _y: i32) -> io::Result<(i32, i32)> {
        unsupported()
    }

    fn client_point(&mut self, _window: u64, _x: i32, _y: i32) -> io::Result<(i32, i32)> {
        unsupported()
    }

    fn client_size(&mut self, _window: u64) -> io::Result<(i32, i32)> {
        unsupported()
    }

    fn top_child(&mut self, _window: u64) -> io::Result<Option<u64>> {
        unsupported()
    }

    fn below(&mut self, _window: u64) -> io::Result<Option<u64>> {
        unsupported()
    }

    fn style(&mut self, _window: u64) -> io::Result<u32> {
        unsupported()
    }

    fn bounds(&mut self, _window: u64) -> io::Result<(i32, i32, i32, i32)> {
        unsupported()
    }

    fn hit_test(&mut self, _window: u64, _at: (i16, i16), _wait: Duration) -> io::Result<usize> {
        unsupported()
    }

    fn class_style(&mut self, _window: u64) -> io::Result<u32> {
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
            input.own_place(7, 0, 0).map(drop),
            input.client_point(7, 0, 0).map(drop),
            input.client_size(7).map(drop),
            input.top_child(7).map(drop),
            input.below(7).map(drop),
            input.style(7).map(drop),
            input.bounds(7).map(drop),
            input.hit_test(7, (0, 0), Duration::ZERO).map(drop),
            input.class_style(7).map(drop),
            input.post(7, message),
        ]
        .map(|result| result.expect_err("unsupported").kind());
        assert_eq!(kinds, [io::ErrorKind::Unsupported; 20]);
    }
}
