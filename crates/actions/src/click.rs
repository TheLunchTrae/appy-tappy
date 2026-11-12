//! Clicks: where on the screen the runtime placed one, and the two ways it
//! reaches its window.
//!
//! A click with the cursor brings its window to the front, moves the real
//! cursor there, and presses through the system's input, so it works in
//! every application. A click sent as messages posts mouse messages to the
//! window under its place and moves neither the cursor nor the focus, but
//! an application may ignore it, and nothing reports whether one acted on
//! it. [`click`] runs either; every choice is made here, and [`Input`]
//! only reads or sends one thing.

use std::io;
use std::time::Duration;

use appytappy_config::anti_cheat::ProcessId;
use appytappy_config::click::{ClickButton, ClickDelivery};

use crate::{ActionError, Input, Scale, Watched};

/// A mouse button that a click presses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    /// The left button.
    Left,
    /// The right button.
    Right,
}

/// One mouse button going down (`down` is `true`) or up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Press {
    /// The button.
    pub button: MouseButton,
    /// Whether it goes down.
    pub down: bool,
}

impl Press {
    /// The same button going the other way: up for down, down for up.
    #[must_use]
    pub const fn reversed(self) -> Self {
        Self {
            button: self.button,
            down: !self.down,
        }
    }
}

/// How long the cursor stays on the place it clicked before it goes back,
/// so an application that reads the cursor once a frame still sees it
/// there.
const CLICK_HOLD: Duration = Duration::from_millis(40);

/// How long each press or message of a click lasts before the next, so an
/// application that reads the mouse once a frame sees the button down, and
/// for a double click up again between the two.
const PRESS_HOLD: Duration = Duration::from_millis(20);

/// How many windows deep a click sent as messages looks for the window
/// under its place, so windows that keep answering with another child
/// cannot hold up the actions behind it. Windows nests windows far less
/// deeply.
const MAX_DEPTH: usize = 64;

/// The presses that click `button`: down and up, twice for a double click.
fn presses(button: ClickButton) -> Vec<Press> {
    let click = |button| {
        [
            Press { button, down: true },
            Press {
                button,
                down: false,
            },
        ]
    };
    match button {
        ClickButton::Left => click(MouseButton::Left).to_vec(),
        ClickButton::Right => click(MouseButton::Right).to_vec(),
        ClickButton::Double => click(MouseButton::Left).repeat(2),
    }
}

/// What a mouse message tells a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseEvent {
    /// The pointer is at the place, with no button down.
    Move,
    /// A button went down.
    Down(MouseButton),
    /// A button went down for the second time of a double click.
    DoubleDown(MouseButton),
    /// A button came up.
    Up(MouseButton),
}

/// One mouse message for a window: what happened, and where in the window's
/// client area, in the pixels the window itself counts in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseMessage {
    /// What happened.
    pub event: MouseEvent,
    /// Where, from the top-left corner of the window's client area.
    pub at: (i16, i16),
}

impl MouseMessage {
    /// The message's number in Windows: `WM_MOUSEMOVE`, or the button's
    /// `WM_?BUTTONDOWN`, `WM_?BUTTONUP`, or `WM_?BUTTONDBLCLK`.
    #[must_use]
    pub const fn number(self) -> u32 {
        match self.event {
            MouseEvent::Move => 0x0200,
            MouseEvent::Down(MouseButton::Left) => 0x0201,
            MouseEvent::Up(MouseButton::Left) => 0x0202,
            MouseEvent::DoubleDown(MouseButton::Left) => 0x0203,
            MouseEvent::Down(MouseButton::Right) => 0x0204,
            MouseEvent::Up(MouseButton::Right) => 0x0205,
            MouseEvent::DoubleDown(MouseButton::Right) => 0x0206,
        }
    }

    /// The message's `wParam`: the mouse buttons that are down as it
    /// arrives, `MK_LBUTTON` or `MK_RBUTTON` while its button goes down
    /// and nothing otherwise. It never names a modifier key, so a click is
    /// never a Ctrl-click or a Shift-click, whatever the user holds.
    #[must_use]
    pub const fn held(self) -> usize {
        match self.event {
            MouseEvent::Move | MouseEvent::Up(_) => 0,
            MouseEvent::Down(MouseButton::Left) | MouseEvent::DoubleDown(MouseButton::Left) => {
                0x0001
            },
            MouseEvent::Down(MouseButton::Right) | MouseEvent::DoubleDown(MouseButton::Right) => {
                0x0002
            },
        }
    }

    /// The message's `lParam`, as `MAKELPARAM` packs it: across in the low
    /// 16 bits and down in the next 16.
    #[must_use]
    pub const fn place(self) -> u32 {
        let [across_low, across_high] = self.at.0.to_le_bytes();
        let [down_low, down_high] = self.at.1.to_le_bytes();
        u32::from_le_bytes([across_low, across_high, down_low, down_high])
    }
}

/// The messages that click `button` at `at`: the pointer arrives, then the
/// button goes down and up. A double click's second press goes down as a
/// double click, the way Windows reports one.
fn messages(button: ClickButton, at: (i16, i16)) -> Vec<MouseMessage> {
    let click = |button| {
        vec![
            MouseEvent::Move,
            MouseEvent::Down(button),
            MouseEvent::Up(button),
        ]
    };
    let events = match button {
        ClickButton::Left => click(MouseButton::Left),
        ClickButton::Right => click(MouseButton::Right),
        ClickButton::Double => {
            let mut events = click(MouseButton::Left);
            events.extend([
                MouseEvent::DoubleDown(MouseButton::Left),
                MouseEvent::Up(MouseButton::Left),
            ]);
            events
        },
    };
    events
        .into_iter()
        .map(|event| MouseMessage { event, at })
        .collect()
}

/// A click the runtime placed on the screen: the window it is for, where
/// it lands, and how it presses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacedClick {
    /// The window it clicks in.
    pub window: u64,
    /// The process that owns the window.
    pub process: ProcessId,
    /// The application's name, for messages.
    pub app: String,
    /// Where it lands, in screen pixels.
    pub at: (i32, i32),
    /// Which button, and how many times.
    pub button: ClickButton,
    /// How it reaches the window.
    pub delivery: ClickDelivery,
}

/// Clicks `click`, which the caller placed on the screen, for `watched`,
/// the way its [`PlacedClick::delivery`] says.
///
/// With the cursor, it clicks only while the watched application or the
/// one it clicks in has the keyboard focus, because those are the
/// applications whose anti-cheat was checked, and only while no mouse
/// button or modifier key is held, so the click is never a drag or a
/// Ctrl-click. It brings the click's window to the front first, so a window
/// in front that holds the mouse, such as a game that keeps the cursor
/// inside it, does not get the press. It clicks only when the click's
/// window is the one showing at its place, so it never lands in a window
/// that covers it, and only when the cursor reached the place. It presses,
/// holding each button down a moment, waits, and moves the cursor back.
/// When the click asks for it, the window that was in front gets the focus
/// back. A button that went down goes up again, even when Windows drops a
/// press.
///
/// As messages, it finds the window under the click's place among the
/// click's window and its child windows, and posts that window the pointer
/// arriving and the button going down and up, a moment apart. Nothing
/// comes to the front and the cursor stays where it is, so it needs neither
/// the focus nor a place that shows: a posted message reaches only its
/// window, whatever is in front or covers it. The messages name no
/// modifier key. `Ok` says the messages were posted, not that the
/// application acted on them: many applications ignore posted mouse
/// messages, and nothing reports which.
///
/// # Errors
///
/// Returns [`ActionError`] when the click could not click or could not be
/// sent, or when a click with the cursor landed but could not put the
/// cursor or the focus back.
pub fn click<I: Input>(
    input: &mut I,
    click: &PlacedClick,
    watched: &Watched,
) -> Result<(), ActionError> {
    match click.delivery {
        ClickDelivery::Cursor { return_focus } => {
            click_with_cursor(input, click, return_focus, watched)
        },
        ClickDelivery::Message => {
            let (window, at) = landing(input, click)?;
            post(input, window, &messages(click.button, at), &click.app)
        },
    }
}

/// Clicks `click` with the real cursor, and gives the focus back after when
/// `return_focus` says so.
fn click_with_cursor<I: Input>(
    input: &mut I,
    click: &PlacedClick,
    return_focus: bool,
    watched: &Watched,
) -> Result<(), ActionError> {
    let focused = input.focused_process()?;
    if focused != Some(watched.process) && focused != Some(click.process) {
        return Err(ActionError::ClickNotInFront {
            watched: watched.executable.clone(),
            target: click.app.clone(),
        });
    }
    if input.buttons_held()? || !input.held_modifiers()?.is_empty() {
        return Err(ActionError::HeldDown);
    }
    let front = input.front_window()?;
    if front != Some(click.window) && !input.bring_to_front(click.window)? {
        return Err(ActionError::NotBroughtToFront(click.app.clone()));
    }
    let clicked = click_in_front(input, click);
    // Even a click that failed brought its window forward.
    let returned = match front {
        Some(front) if return_focus && front != click.window => input.bring_to_front(front),
        _ => Ok(true),
    };
    clicked?;
    if returned? {
        Ok(())
    } else {
        Err(ActionError::FocusNotReturned)
    }
}

/// Clicks `click` once its window is in front, and puts the cursor back.
fn click_in_front<I: Input>(input: &mut I, click: &PlacedClick) -> Result<(), ActionError> {
    let (x, y) = click.at;
    if input.window_at(x, y)? != Some(click.window) {
        return Err(ActionError::Covered(click.app.clone()));
    }
    let (cursor_x, cursor_y) = input.cursor()?;
    input.move_cursor(x, y)?;
    // Windows keeps the cursor inside a window that confines it without
    // failing the move, so the press waits until the cursor is there.
    let pressed = match input.cursor() {
        Ok(at) if at == (x, y) => press_buttons(input, &presses(click.button)),
        Ok(_) => Err(ActionError::CursorBlocked(click.app.clone())),
        Err(error) => Err(error.into()),
    };
    if pressed.is_ok() {
        input.hold(CLICK_HOLD);
    }
    let moved_back = input.move_cursor(cursor_x, cursor_y);
    pressed?;
    moved_back?;
    Ok(())
}

/// Sends `presses` one at a time, holding each button down [`PRESS_HOLD`],
/// since an application that reads the buttons once a frame would miss a
/// press that goes down and up at once. A button that went down goes up
/// again when Windows drops or fails the next press.
fn press_buttons<I: Input>(input: &mut I, presses: &[Press]) -> Result<(), ActionError> {
    let mut last: Option<Press> = None;
    for (sent, &press) in presses.iter().enumerate() {
        let failed = match input.send_presses(&[press]) {
            Ok(0) => ActionError::ButtonsDropped {
                sent,
                of: presses.len(),
            },
            Ok(_) => {
                // Each press stays a moment before the next, so a double
                // click's second press is seen as a second one.
                if press.down || sent < presses.len().saturating_sub(1) {
                    input.hold(PRESS_HOLD);
                }
                last = Some(press);
                continue;
            },
            Err(error) => error.into(),
        };
        if let Some(last) = last
            && last.down
        {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the click already failed, and its error says why"
            )]
            let _ = input.send_presses(&[last.reversed()]);
        }
        return Err(failed);
    }
    Ok(())
}

/// The window that gets `click` as messages, and its place there as that
/// window counts it.
///
/// The window is the deepest one under the click's place, among the
/// click's window and its children: the mouse itself goes to the child
/// under it, so the messages do too. A child that is hidden, disabled, or
/// transparent to the mouse is passed over, as the mouse passes it over.
/// The place has to lie in the client area of each window on the way,
/// because a mouse message names a place there: a title bar, a border, or a
/// scroll bar gets other messages, which a click does not send. A window
/// that Windows stretches, because its program does not follow the
/// monitor's scale, counts in its own pixels, so the place is scaled to
/// them.
fn landing<I: Input>(input: &mut I, click: &PlacedClick) -> Result<(u64, (i16, i16)), ActionError> {
    let (x, y) = click.at;
    let unreachable = || ActionError::Unreachable(click.app.clone());
    let mut window = click.window;
    for _ in 0..MAX_DEPTH {
        let point = input.client_point(window, x, y)?;
        if !inside(point, input.client_size(window)?) {
            return Err(unreachable());
        }
        match input.child_at(window, point.0, point.1)? {
            Some(child) if child != window => window = child,
            Some(_) => {
                let scale = input.scale(window)?;
                return scaled(point, scale)
                    .map(|at| (window, at))
                    .ok_or_else(unreachable);
            },
            None => return Err(unreachable()),
        }
    }
    Err(unreachable())
}

/// Whether `point` lies in a client area of `size`.
fn inside((x, y): (i32, i32), (width, height): (i32, i32)) -> bool {
    (0..width).contains(&x) && (0..height).contains(&y)
}

/// `point`, in screen pixels from a window's corner, in the pixels the
/// window counts in, or `None` when a mouse message cannot name a place
/// that far in. A screen pixel belongs to the window pixel that covers it.
fn scaled((x, y): (i32, i32), scale: Scale) -> Option<(i16, i16)> {
    let along = |pixels: i32| {
        let scaled = i64::from(pixels)
            .checked_mul(i64::from(scale.window.get()))?
            .checked_div(i64::from(scale.monitor.get()))?;
        i16::try_from(scaled).ok()
    };
    Some((along(x)?, along(y)?))
}

/// Posts `messages` to `window` one at a time, [`PRESS_HOLD`] apart, since
/// an application that reads its messages once a frame would miss a button
/// that goes down and up at once. A button that went down goes up again
/// when Windows refuses the next message.
fn post<I: Input>(
    input: &mut I,
    window: u64,
    messages: &[MouseMessage],
    app: &str,
) -> Result<(), ActionError> {
    let last = messages.len().saturating_sub(1);
    let mut down: Option<MouseButton> = None;
    for (sent, &message) in messages.iter().enumerate() {
        if let Err(error) = input.post(window, message) {
            if let Some(button) = down {
                let up = MouseMessage {
                    event: MouseEvent::Up(button),
                    at: message.at,
                };
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the click already failed, and its error says why"
                )]
                let _ = input.post(window, up);
            }
            return Err(if error.kind() == io::ErrorKind::PermissionDenied {
                ActionError::MessageBlocked(app.to_owned())
            } else {
                error.into()
            });
        }
        down = match message.event {
            MouseEvent::Down(button) | MouseEvent::DoubleDown(button) => Some(button),
            MouseEvent::Up(_) => None,
            MouseEvent::Move => down,
        };
        if sent != last {
            input.hold(PRESS_HOLD);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::num::NonZeroU32;

    use appytappy_config::action::Modifier;

    use super::*;
    use crate::Stroke;

    /// A window's client area on the screen, and the windows inside it.
    #[derive(Default)]
    struct Pane {
        /// The screen place of its client area's top-left corner.
        origin: (i32, i32),
        /// Its client area's size.
        size: (i32, i32),
        /// How far its border reaches around its client area.
        border: i32,
        /// Its child windows, topmost first.
        children: Vec<u64>,
    }

    impl Pane {
        /// Whether the window, border included, holds the screen place.
        fn holds(&self, (x, y): (i32, i32)) -> bool {
            let within = |at: i32, origin: i32, size: i32| {
                let start = origin.checked_sub(self.border).expect("on the screen");
                let end = origin
                    .checked_add(size)
                    .and_then(|end| end.checked_add(self.border))
                    .expect("on the screen");
                (start..end).contains(&at)
            };
            within(x, self.origin.0, self.size.0) && within(y, self.origin.1, self.size.1)
        }
    }

    /// A screen with a cursor, a window in front, and the windows a click
    /// sent as messages looks through, which logs what a click does to it.
    #[derive(Default)]
    struct Desk {
        focused: Option<u32>,
        held: Vec<Modifier>,
        /// How many presses each next send takes, first first; once it runs
        /// out, a send takes them all.
        takes: Vec<usize>,
        /// Whether sending presses fails.
        mouse_broken: bool,
        /// The window in front.
        front: Option<u64>,
        /// The window that shows at every place.
        under: Option<u64>,
        /// The windows that stay behind when asked to come to the front.
        stays_behind: Vec<u64>,
        cursor: (i32, i32),
        /// Where a window that confines the cursor stops every move.
        confined: Option<(i32, i32)>,
        /// Whether a mouse button is held down.
        button_down: bool,
        /// Whether each next cursor move fails, first first; once it runs
        /// out, a move works.
        moves_fail: Vec<bool>,
        /// Each window's client area, by window.
        panes: BTreeMap<u64, Pane>,
        /// The windows that answer that a place is outside them.
        outside: Vec<u64>,
        /// Whether every window has another child under every place.
        endless: bool,
        /// The window's and its monitor's dots per inch, or 96 for both.
        scale: Option<(u32, u32)>,
        /// How each next post fails, first first, with `None` for one that
        /// works; once it runs out, a post works.
        posts_fail: Vec<Option<io::ErrorKind>>,
        log: Vec<String>,
    }

    impl Desk {
        fn pane(&self, window: u64) -> io::Result<&Pane> {
            self.panes
                .get(&window)
                .ok_or_else(|| io::ErrorKind::NotFound.into())
        }
    }

    impl Input for Desk {
        fn focused_process(&mut self) -> io::Result<Option<ProcessId>> {
            Ok(self.focused.map(ProcessId::new))
        }

        fn held_modifiers(&mut self) -> io::Result<Vec<Modifier>> {
            Ok(self.held.clone())
        }

        fn send_strokes(&mut self, _strokes: &[Stroke]) -> io::Result<usize> {
            Err(io::ErrorKind::Unsupported.into())
        }

        fn front_window(&mut self) -> io::Result<Option<u64>> {
            Ok(self.front)
        }

        fn window_at(&mut self, x: i32, y: i32) -> io::Result<Option<u64>> {
            self.log.push(format!("under {x},{y}"));
            Ok(self.under)
        }

        fn bring_to_front(&mut self, window: u64) -> io::Result<bool> {
            self.log.push(format!("front {window}"));
            if !self.stays_behind.contains(&window) {
                self.front = Some(window);
            }
            Ok(self.front == Some(window))
        }

        fn cursor(&mut self) -> io::Result<(i32, i32)> {
            Ok(self.cursor)
        }

        fn move_cursor(&mut self, x: i32, y: i32) -> io::Result<()> {
            if !self.moves_fail.is_empty() && self.moves_fail.remove(0) {
                return Err(io::Error::other("input blocked"));
            }
            self.log.push(format!("cursor {x},{y}"));
            self.cursor = self.confined.unwrap_or((x, y));
            Ok(())
        }

        fn buttons_held(&mut self) -> io::Result<bool> {
            Ok(self.button_down)
        }

        fn send_presses(&mut self, presses: &[Press]) -> io::Result<usize> {
            if self.mouse_broken {
                return Err(io::Error::other("input blocked"));
            }
            let sent = if self.takes.is_empty() {
                presses.len()
            } else {
                self.takes.remove(0).min(presses.len())
            };
            let words: Vec<String> = presses
                .iter()
                .take(sent)
                .map(|press| {
                    let arrow = if press.down { "↓" } else { "↑" };
                    format!("{:?}{arrow}", press.button)
                })
                .collect();
            self.log.push(format!("buttons {}", words.join(" ")));
            // A click brings the window it lands in to the front.
            if sent > 0 {
                self.front = self.under;
            }
            Ok(sent)
        }

        fn hold(&mut self, time: Duration) {
            self.log.push(format!("hold {}ms", time.as_millis()));
        }

        fn client_point(&mut self, window: u64, x: i32, y: i32) -> io::Result<(i32, i32)> {
            if self.endless {
                return Ok((0, 0));
            }
            let (left, top) = self.pane(window)?.origin;
            Ok((
                x.checked_sub(left).expect("on the screen"),
                y.checked_sub(top).expect("on the screen"),
            ))
        }

        fn client_size(&mut self, window: u64) -> io::Result<(i32, i32)> {
            if self.endless {
                return Ok((1, 1));
            }
            Ok(self.pane(window)?.size)
        }

        fn child_at(&mut self, window: u64, x: i32, y: i32) -> io::Result<Option<u64>> {
            if self.endless {
                return Ok(Some(window.checked_add(1).expect("few windows")));
            }
            if self.outside.contains(&window) {
                return Ok(None);
            }
            let pane = self.pane(window)?;
            let place = (
                pane.origin.0.checked_add(x).expect("on the screen"),
                pane.origin.1.checked_add(y).expect("on the screen"),
            );
            let child = pane.children.iter().copied().find(|child| {
                self.panes
                    .get(child)
                    .is_some_and(|child| child.holds(place))
            });
            Ok(Some(child.unwrap_or(window)))
        }

        fn scale(&mut self, window: u64) -> io::Result<Scale> {
            self.pane(window)?;
            let (window, monitor) = self.scale.unwrap_or((96, 96));
            Ok(scale(window, monitor))
        }

        fn post(&mut self, window: u64, message: MouseMessage) -> io::Result<()> {
            if !self.posts_fail.is_empty()
                && let Some(kind) = self.posts_fail.remove(0)
            {
                return Err(kind.into());
            }
            let (x, y) = message.at;
            self.log
                .push(format!("post {window} {:?} {x},{y}", message.event));
            Ok(())
        }
    }

    fn scale(window: u32, monitor: u32) -> Scale {
        Scale {
            window: NonZeroU32::new(window).expect("not zero"),
            monitor: NonZeroU32::new(monitor).expect("not zero"),
        }
    }

    fn press(button: MouseButton, down: bool) -> Press {
        Press { button, down }
    }

    fn watched() -> Watched {
        Watched {
            process: ProcessId::new(7),
            executable: "Example.exe".to_owned(),
        }
    }

    /// A click for `watched` in window 9 of process 8, at (100, 200).
    fn click_at(button: ClickButton, delivery: ClickDelivery) -> PlacedClick {
        PlacedClick {
            window: 9,
            process: ProcessId::new(8),
            app: "Target App".to_owned(),
            at: (100, 200),
            button,
            delivery,
        }
    }

    /// A click with the cursor.
    fn placed(button: ClickButton, return_focus: bool) -> PlacedClick {
        click_at(button, ClickDelivery::Cursor { return_focus })
    }

    /// A click sent as messages.
    fn posted(button: ClickButton) -> PlacedClick {
        click_at(button, ClickDelivery::Message)
    }

    /// A screen where window 1 is in front, window 9 shows at every place,
    /// and the cursor is at (5, 5).
    fn screen(focused: u32) -> Desk {
        Desk {
            focused: Some(focused),
            front: Some(1),
            under: Some(9),
            cursor: (5, 5),
            ..Desk::default()
        }
    }

    /// A screen where another application's window 1 is in front and covers
    /// window 9, whose 200 by 100 client area starts at (60, 150), while the
    /// user holds a mouse button and Ctrl: nothing a click with the cursor
    /// would click through.
    fn behind() -> Desk {
        Desk {
            focused: Some(3),
            front: Some(1),
            under: Some(1),
            cursor: (5, 5),
            button_down: true,
            held: vec![Modifier::Ctrl],
            panes: BTreeMap::from([(
                9,
                Pane {
                    origin: (60, 150),
                    size: (200, 100),
                    ..Pane::default()
                },
            )]),
            ..Desk::default()
        }
    }

    #[test]
    fn a_click_presses_its_button_down_then_up() {
        assert_eq!(
            presses(ClickButton::Left),
            [
                press(MouseButton::Left, true),
                press(MouseButton::Left, false)
            ]
        );
        assert_eq!(
            presses(ClickButton::Right),
            [
                press(MouseButton::Right, true),
                press(MouseButton::Right, false)
            ]
        );
        assert_eq!(
            presses(ClickButton::Double),
            [
                press(MouseButton::Left, true),
                press(MouseButton::Left, false),
                press(MouseButton::Left, true),
                press(MouseButton::Left, false)
            ]
        );
    }

    #[test]
    fn a_reversed_press_goes_the_other_way() {
        assert_eq!(
            press(MouseButton::Right, true).reversed(),
            press(MouseButton::Right, false)
        );
        assert_eq!(
            press(MouseButton::Left, false).reversed(),
            press(MouseButton::Left, true)
        );
    }

    #[test]
    fn a_click_presses_where_it_lands_then_puts_the_cursor_and_focus_back() {
        let mut desk = screen(7);
        click(&mut desk, &placed(ClickButton::Double, true), &watched()).expect("clicked");
        assert_eq!(
            desk.log,
            [
                "front 9",
                "under 100,200",
                "cursor 100,200",
                "buttons Left↓",
                "hold 20ms",
                "buttons Left↑",
                "hold 20ms",
                "buttons Left↓",
                "hold 20ms",
                "buttons Left↑",
                "hold 40ms",
                "cursor 5,5",
                "front 1"
            ]
        );
        assert_eq!(desk.cursor, (5, 5));
    }

    #[test]
    fn a_click_leaves_the_focus_where_it_was_asked_to() {
        let mut desk = screen(7);
        click(&mut desk, &placed(ClickButton::Right, false), &watched()).expect("clicked");
        assert_eq!(
            desk.log,
            [
                "front 9",
                "under 100,200",
                "cursor 100,200",
                "buttons Right↓",
                "hold 20ms",
                "buttons Right↑",
                "hold 40ms",
                "cursor 5,5"
            ]
        );
        // The window clicked in was in front already, so nothing changes.
        let mut desk = screen(8);
        desk.front = Some(9);
        click(&mut desk, &placed(ClickButton::Left, true), &watched()).expect("clicked");
        assert!(
            !desk.log.iter().any(|line| line.starts_with("front")),
            "{:?}",
            desk.log
        );
    }

    #[test]
    fn a_click_clicks_only_while_the_watched_or_target_application_is_in_front() {
        for focused in [Some(3), None] {
            let mut desk = screen(7);
            desk.focused = focused;
            let error = click(&mut desk, &placed(ClickButton::Left, true), &watched())
                .expect_err("not in front");
            assert!(
                matches!(
                    &error,
                    ActionError::ClickNotInFront { watched, target }
                        if watched == "Example.exe" && target == "Target App"
                ),
                "{error}"
            );
            assert_eq!(desk.log, Vec::<String>::new());
        }
    }

    #[test]
    fn a_click_waits_for_held_buttons_and_modifiers() {
        let mut desk = screen(7);
        desk.button_down = true;
        assert!(matches!(
            click(&mut desk, &placed(ClickButton::Left, true), &watched()),
            Err(ActionError::HeldDown)
        ));
        let mut desk = screen(7);
        desk.held = vec![Modifier::Ctrl];
        assert!(matches!(
            click(&mut desk, &placed(ClickButton::Left, true), &watched()),
            Err(ActionError::HeldDown)
        ));
        assert_eq!(desk.log, Vec::<String>::new());
    }

    #[test]
    fn a_click_needs_its_window_in_front() {
        let mut desk = screen(7);
        desk.stays_behind = vec![9];
        assert!(matches!(
            click(&mut desk, &placed(ClickButton::Left, true), &watched()),
            Err(ActionError::NotBroughtToFront(app)) if app == "Target App"
        ));
        assert_eq!(desk.log, ["front 9"]);
    }

    #[test]
    fn a_click_does_not_land_in_a_window_that_covers_its_place() {
        let mut desk = screen(8);
        desk.under = Some(4);
        assert!(matches!(
            click(&mut desk, &placed(ClickButton::Left, true), &watched()),
            Err(ActionError::Covered(app)) if app == "Target App"
        ));
        assert_eq!(
            desk.log,
            ["front 9", "under 100,200", "front 1"],
            "the focus goes back"
        );
    }

    #[test]
    fn a_click_does_not_press_where_the_cursor_was_stopped() {
        let mut desk = screen(8);
        desk.confined = Some((60, 200));
        assert!(matches!(
            click(&mut desk, &placed(ClickButton::Left, true), &watched()),
            Err(ActionError::CursorBlocked(app)) if app == "Target App"
        ));
        assert_eq!(
            desk.log,
            [
                "front 9",
                "under 100,200",
                "cursor 100,200",
                "cursor 5,5",
                "front 1"
            ]
        );
    }

    #[test]
    fn a_button_that_went_down_goes_up_when_presses_are_dropped() {
        for (takes, sent, of, release) in [
            (vec![1, 0], 1, 2, Some("buttons Left↑")),
            (vec![0], 0, 2, None),
            (vec![1, 1, 0], 2, 4, None),
            (vec![1, 1, 1, 0], 3, 4, Some("buttons Left↑")),
        ] {
            let mut desk = screen(7);
            desk.takes = takes;
            let button = if of == 2 {
                ClickButton::Left
            } else {
                ClickButton::Double
            };
            let error = click(&mut desk, &placed(button, true), &watched()).expect_err("dropped");
            assert!(
                matches!(error, ActionError::ButtonsDropped { sent: s, of: o } if s == sent && o == of),
                "{error}"
            );
            let log = &desk.log;
            let after = log
                .iter()
                .skip_while(|line| *line != "buttons ")
                .nth(1)
                .map(String::as_str);
            assert_eq!(
                after.filter(|line| line.starts_with("buttons")),
                release,
                "{log:?}"
            );
            assert!(!log.contains(&"hold 40ms".to_owned()), "{log:?}");
            assert_eq!(desk.cursor, (5, 5), "{log:?}");
            assert_eq!(log.last().map(String::as_str), Some("front 1"), "{log:?}");
        }
        let mut desk = screen(7);
        desk.mouse_broken = true;
        assert!(matches!(
            click(&mut desk, &placed(ClickButton::Left, true), &watched()),
            Err(ActionError::Os(_))
        ));
        assert_eq!(desk.cursor, (5, 5));
    }

    #[test]
    fn a_click_that_cannot_put_things_back_says_so() {
        let mut desk = screen(7);
        desk.moves_fail = vec![false, true];
        assert!(matches!(
            click(&mut desk, &placed(ClickButton::Left, true), &watched()),
            Err(ActionError::Os(_))
        ));
        assert_eq!(
            desk.log.last().map(String::as_str),
            Some("front 1"),
            "the focus goes back even when the cursor does not"
        );
        let mut desk = screen(7);
        desk.moves_fail = vec![true];
        assert!(matches!(
            click(&mut desk, &placed(ClickButton::Left, true), &watched()),
            Err(ActionError::Os(_))
        ));
        assert_eq!(desk.log, ["front 9", "under 100,200", "front 1"]);
    }

    #[test]
    fn a_click_whose_window_keeps_the_focus_says_so() {
        let mut desk = screen(7);
        desk.stays_behind = vec![1];
        assert!(matches!(
            click(&mut desk, &placed(ClickButton::Left, true), &watched()),
            Err(ActionError::FocusNotReturned)
        ));
    }

    #[test]
    fn a_click_s_messages_are_the_pointer_arriving_then_its_button() {
        let events = |button| -> Vec<MouseEvent> {
            messages(button, (3, 4))
                .into_iter()
                .map(|message| {
                    assert_eq!(message.at, (3, 4));
                    message.event
                })
                .collect()
        };
        let (left, right) = (MouseButton::Left, MouseButton::Right);
        assert_eq!(
            events(ClickButton::Left),
            [
                MouseEvent::Move,
                MouseEvent::Down(left),
                MouseEvent::Up(left)
            ]
        );
        assert_eq!(
            events(ClickButton::Right),
            [
                MouseEvent::Move,
                MouseEvent::Down(right),
                MouseEvent::Up(right)
            ]
        );
        assert_eq!(
            events(ClickButton::Double),
            [
                MouseEvent::Move,
                MouseEvent::Down(left),
                MouseEvent::Up(left),
                MouseEvent::DoubleDown(left),
                MouseEvent::Up(left)
            ]
        );
    }

    #[test]
    fn a_message_names_its_number_its_button_and_its_place() {
        let (left, right) = (MouseButton::Left, MouseButton::Right);
        for (event, number, held) in [
            (MouseEvent::Move, 0x0200, 0),
            (MouseEvent::Down(left), 0x0201, 1),
            (MouseEvent::Up(left), 0x0202, 0),
            (MouseEvent::DoubleDown(left), 0x0203, 1),
            (MouseEvent::Down(right), 0x0204, 2),
            (MouseEvent::Up(right), 0x0205, 0),
            (MouseEvent::DoubleDown(right), 0x0206, 2),
        ] {
            let message = MouseMessage { event, at: (0, 0) };
            assert_eq!(message.number(), number, "{event:?}");
            assert_eq!(message.held(), held, "{event:?}");
        }
        for (at, place) in [
            ((0, 0), 0),
            ((3, 5), 0x0005_0003),
            ((0x0123, 0x0456), 0x0456_0123),
            ((i16::MAX, 1), 0x0001_7FFF),
            ((-1, 2), 0x0002_FFFF),
            ((2, -1), 0xFFFF_0002),
        ] {
            let message = MouseMessage {
                event: MouseEvent::Move,
                at,
            };
            assert_eq!(message.place(), place, "{at:?}");
        }
    }

    #[test]
    fn a_click_sent_as_messages_moves_neither_the_cursor_nor_the_focus() {
        let mut desk = behind();
        click(&mut desk, &posted(ClickButton::Left), &watched()).expect("sent");
        assert_eq!(
            desk.log,
            [
                "post 9 Move 40,50",
                "hold 20ms",
                "post 9 Down(Left) 40,50",
                "hold 20ms",
                "post 9 Up(Left) 40,50"
            ],
            "nothing comes to the front, and no cursor moves"
        );
        assert_eq!(desk.cursor, (5, 5));
        assert_eq!(desk.front, Some(1));

        let mut desk = behind();
        click(&mut desk, &posted(ClickButton::Double), &watched()).expect("sent");
        assert_eq!(
            desk.log,
            [
                "post 9 Move 40,50",
                "hold 20ms",
                "post 9 Down(Left) 40,50",
                "hold 20ms",
                "post 9 Up(Left) 40,50",
                "hold 20ms",
                "post 9 DoubleDown(Left) 40,50",
                "hold 20ms",
                "post 9 Up(Left) 40,50"
            ]
        );
    }

    /// [`behind`], with window 10 inside window 9 at (90, 180), 50 by 40
    /// with a border of 2, and window 11 inside that at (95, 190), 20 by
    /// 20.
    fn nested() -> Desk {
        let mut desk = behind();
        desk.panes.entry(9).or_default().children = vec![10];
        desk.panes.insert(
            10,
            Pane {
                origin: (90, 180),
                size: (50, 40),
                border: 2,
                children: vec![11],
            },
        );
        desk.panes.insert(
            11,
            Pane {
                origin: (95, 190),
                size: (20, 20),
                ..Pane::default()
            },
        );
        desk
    }

    #[test]
    fn messages_go_to_the_deepest_window_under_the_place() {
        for (at, first) in [
            ((100, 200), "post 11 Move 5,10"),
            ((92, 182), "post 10 Move 2,2"),
            ((61, 151), "post 9 Move 1,1"),
        ] {
            let mut desk = nested();
            let click_ = PlacedClick {
                at,
                ..posted(ClickButton::Right)
            };
            click(&mut desk, &click_, &watched()).expect("sent");
            assert_eq!(desk.log.first().map(String::as_str), Some(first), "{at:?}");
            assert_eq!(desk.log.len(), 5, "{:?}", desk.log);
        }
    }

    #[test]
    fn a_place_outside_a_client_area_gets_no_message() {
        // Window 9's client area runs from (60, 150) to (259, 249).
        for (at, lands) in [
            ((60, 150), true),
            ((259, 249), true),
            ((59, 150), false),
            ((60, 149), false),
            ((260, 150), false),
            ((60, 250), false),
            ((59, 149), false),
            ((260, 250), false),
        ] {
            let mut desk = behind();
            let click_ = PlacedClick {
                at,
                ..posted(ClickButton::Left)
            };
            let sent = click(&mut desk, &click_, &watched());
            if lands {
                sent.expect("inside");
                assert_eq!(desk.log.len(), 5, "{at:?}");
            } else {
                assert!(
                    matches!(&sent, Err(ActionError::Unreachable(app)) if app == "Target App"),
                    "{at:?}: {sent:?}"
                );
                assert_eq!(desk.log, Vec::<String>::new(), "{at:?}");
            }
        }
        // A child's border belongs to the child, but not to its client
        // area.
        let mut desk = nested();
        let on_border = PlacedClick {
            at: (89, 180),
            ..posted(ClickButton::Left)
        };
        assert!(matches!(
            click(&mut desk, &on_border, &watched()),
            Err(ActionError::Unreachable(_))
        ));
        assert_eq!(desk.log, Vec::<String>::new());
        let mut desk = behind();
        desk.outside = vec![9];
        assert!(matches!(
            click(&mut desk, &posted(ClickButton::Left), &watched()),
            Err(ActionError::Unreachable(_))
        ));
        assert_eq!(desk.log, Vec::<String>::new());
    }

    #[test]
    fn a_stretched_window_gets_the_place_in_its_own_pixels() {
        let mut desk = behind();
        desk.scale = Some((96, 144));
        click(&mut desk, &posted(ClickButton::Left), &watched()).expect("sent");
        assert_eq!(
            desk.log.first().map(String::as_str),
            Some("post 9 Move 26,33")
        );
        assert_eq!(scaled((300, 150), scale(96, 144)), Some((200, 100)));
        assert_eq!(scaled((299, 149), scale(96, 144)), Some((199, 99)));
        assert_eq!(scaled((100, 10), scale(144, 96)), Some((150, 15)));
        assert_eq!(scaled((7, 9), scale(120, 120)), Some((7, 9)));
        assert_eq!(
            scaled((32_767, 32_767), scale(96, 96)),
            Some((i16::MAX, i16::MAX))
        );
        assert_eq!(scaled((32_768, 0), scale(96, 96)), None);
        assert_eq!(scaled((0, 32_768), scale(96, 96)), None);
        assert_eq!(scaled((30_000, 0), scale(144, 96)), None);
        // A place too far in for a message is not sent.
        let mut desk = behind();
        desk.panes.entry(9).or_default().size = (40_000, 100);
        let far = PlacedClick {
            at: (39_060, 200),
            ..posted(ClickButton::Left)
        };
        assert!(matches!(
            click(&mut desk, &far, &watched()),
            Err(ActionError::Unreachable(_))
        ));
        assert_eq!(desk.log, Vec::<String>::new());
    }

    #[test]
    fn windows_nested_without_end_get_no_message() {
        let mut desk = behind();
        desk.endless = true;
        assert!(matches!(
            click(&mut desk, &posted(ClickButton::Left), &watched()),
            Err(ActionError::Unreachable(app)) if app == "Target App"
        ));
        assert_eq!(desk.log, Vec::<String>::new());
    }

    #[test]
    fn a_window_that_refuses_a_message_says_so() {
        let mut desk = behind();
        desk.posts_fail = vec![Some(io::ErrorKind::PermissionDenied)];
        assert!(matches!(
            click(&mut desk, &posted(ClickButton::Left), &watched()),
            Err(ActionError::MessageBlocked(app)) if app == "Target App"
        ));
        assert_eq!(desk.log, Vec::<String>::new());

        let other = Some(io::ErrorKind::BrokenPipe);
        for (fails, log) in [
            // The pointer arrived, and no button went down.
            (vec![None, other], vec!["post 9 Move 40,50", "hold 20ms"]),
            // The button went down, so it goes up again.
            (
                vec![None, None, other],
                vec![
                    "post 9 Move 40,50",
                    "hold 20ms",
                    "post 9 Down(Left) 40,50",
                    "hold 20ms",
                    "post 9 Up(Left) 40,50",
                ],
            ),
            // The button was up when the second press was refused.
            (
                vec![None, None, None, other],
                vec![
                    "post 9 Move 40,50",
                    "hold 20ms",
                    "post 9 Down(Left) 40,50",
                    "hold 20ms",
                    "post 9 Up(Left) 40,50",
                    "hold 20ms",
                ],
            ),
            (
                vec![None, None, None, None, other],
                vec![
                    "post 9 Move 40,50",
                    "hold 20ms",
                    "post 9 Down(Left) 40,50",
                    "hold 20ms",
                    "post 9 Up(Left) 40,50",
                    "hold 20ms",
                    "post 9 DoubleDown(Left) 40,50",
                    "hold 20ms",
                    "post 9 Up(Left) 40,50",
                ],
            ),
        ] {
            let mut desk = behind();
            desk.posts_fail = fails;
            let sent = click(&mut desk, &posted(ClickButton::Double), &watched());
            assert!(
                matches!(&sent, Err(ActionError::Os(error)) if error.kind() == io::ErrorKind::BrokenPipe),
                "{sent:?}"
            );
            assert_eq!(desk.log, log);
        }
        // A window that closed has no client area to find the place in.
        let mut desk = behind();
        desk.panes.clear();
        assert!(matches!(
            click(&mut desk, &posted(ClickButton::Left), &watched()),
            Err(ActionError::Os(error)) if error.kind() == io::ErrorKind::NotFound
        ));
    }
}
