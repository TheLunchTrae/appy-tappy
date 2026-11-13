//! Clicks: where on the screen the runtime placed one, and the two ways it
//! reaches its window.
//!
//! A click with the cursor brings its window to the front, moves the real
//! cursor there, and presses through the system's input, so it works
//! wherever a person can click. A click sent as messages posts mouse
//! messages to the window that a real click at its place would reach, and
//! itself moves neither the cursor nor the focus, but an application may
//! ignore it, and nothing reports whether one acted on it. [`click`] runs
//! either; every choice is made here, and [`Input`] only reads or sends
//! one thing.

use std::io;
use std::time::{Duration, Instant};

use appytappy_config::click::{ClickButton, ClickDelivery};
use appytappy_config::process::ProcessId;

use crate::{ActionError, Input, Watched};

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

/// How many child windows of one window a click sent as messages looks
/// through, for the same reason. A window has far fewer.
const MAX_SIBLINGS: usize = 4096;

/// How long the windows under a click sent as messages may take in all to
/// answer where its place is on them, so a click cannot hold up the actions
/// behind it for longer, however many windows it asks.
const WALK_TIME: Duration = Duration::from_millis(500);

/// How long one window may take to answer where a place is on it.
const ASK_TIME: Duration = Duration::from_millis(200);

/// The shortest time a window is given to answer. Windows counts the wait
/// in whole milliseconds, so less than one would be no wait at all.
const SHORTEST_ASK: Duration = Duration::from_millis(1);

/// The window style bit of a window that is not hidden: `WS_VISIBLE`.
const STYLE_VISIBLE: u32 = 0x1000_0000;

/// The window style bit of a window that takes no input: `WS_DISABLED`.
const STYLE_DISABLED: u32 = 0x0800_0000;

/// The class style bit of a window that is told about double clicks:
/// `CS_DBLCLKS`.
const CLASS_DOUBLE_CLICKS: u32 = 0x0008;

/// What a window answers when the mouse goes through it at a place, to the
/// window behind it: `HTTRANSPARENT`.
const HIT_THROUGH: i32 = -1;

/// What a window answers for a place in its client area: `HTCLIENT`.
const HIT_CLIENT: i32 = 1;

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
    /// and nothing otherwise. It names no modifier key, whatever the user
    /// holds, so the message itself never says Ctrl-click or Shift-click.
    /// An application that asks Windows which keys are down while it
    /// handles the message still sees the keys the user holds.
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

    /// The message's `lParam`: its place, as [`pack`] packs it.
    #[must_use]
    pub const fn place(self) -> u32 {
        pack(self.at)
    }
}

/// A place as `MAKELPARAM` packs it for a message: across in the low 16
/// bits and down in the next 16.
#[must_use]
pub const fn pack((across, down): (i16, i16)) -> u32 {
    let [across_low, across_high] = across.to_le_bytes();
    let [down_low, down_high] = down.to_le_bytes();
    u32::from_le_bytes([across_low, across_high, down_low, down_high])
}

/// The messages that click `button` at `at`: the pointer arrives, then the
/// button goes down and up.
///
/// A double click's second press depends on the window. Windows tells a
/// window about double clicks only when its class asks for them, which
/// `double_clicks` says: such a window gets the second press as a double
/// click, and usually ignores a plain second press. Any other window gets
/// two plain presses from the mouse, and usually ignores a double-click
/// message, so it gets two plain presses here too.
fn messages(button: ClickButton, at: (i16, i16), double_clicks: bool) -> Vec<MouseMessage> {
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
            let again = if double_clicks {
                MouseEvent::DoubleDown(MouseButton::Left)
            } else {
                MouseEvent::Down(MouseButton::Left)
            };
            let mut events = click(MouseButton::Left);
            events.extend([again, MouseEvent::Up(MouseButton::Left)]);
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
/// As messages, it finds the window that a real click at the place would
/// reach, among the click's window and its child windows, and posts that
/// window the pointer arriving and the button going down and up, a moment
/// apart. It brings nothing to the front and leaves the cursor where it
/// is, so it needs neither the focus nor a place that shows: a posted
/// message reaches only its window, whatever is in front or covers it. The
/// messages name no modifier key. `Ok` says the messages were posted, not
/// that the application acted on them: many applications ignore posted
/// mouse messages, and nothing reports which.
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
            let Landing { window, at } = landing(input, click)?;
            let double_clicks = reports_double_clicks(input.class_style(window)?);
            let messages = messages(click.button, at, double_clicks);
            post(input, window, &messages, &click.app)
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

/// Where a click sent as messages lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Landing {
    /// The window that gets the messages.
    window: u64,
    /// The place in that window's client area, as the window counts it.
    at: (i16, i16),
}

/// The window that gets `click` as messages, and its place there.
///
/// It is the window a real click at the place would reach, which Windows
/// finds by asking the windows themselves, so this asks them the same way.
/// A window that is disabled takes no click at all, as while it shows a
/// dialog. The place is first put in the pixels the window's program
/// counts the screen in, which are not the screen's own for a window that
/// Windows stretches. The windows asked have [`WALK_TIME`] in all to
/// answer.
fn landing<I: Input>(input: &mut I, click: &PlacedClick) -> Result<Landing, ActionError> {
    if disabled(input.style(click.window)?) {
        return Err(ActionError::WindowDisabled(click.app.clone()));
    }
    let (x, y) = click.at;
    let place = input.own_place(click.window, x, y)?;
    let started = input.now();
    let walk = Walk {
        app: &click.app,
        // A clock that cannot count that far leaves no time to ask.
        deadline: started.checked_add(WALK_TIME).unwrap_or(started),
    };
    under(input, click.window, place, MAX_DEPTH, walk)?
        .ok_or_else(|| ActionError::Unreachable(click.app.clone()))
}

/// What looking for the window under a click keeps all the way down.
#[derive(Debug, Clone, Copy)]
struct Walk<'click> {
    /// The application's name, for messages.
    app: &'click str,
    /// When the windows must have answered.
    deadline: Instant,
}

/// How long a window asked at `now` may take to answer, or `None` once
/// `deadline` has come: [`ASK_TIME`], or what is left when that is less,
/// and never less than [`SHORTEST_ASK`] while any time is left.
fn ask_time(now: Instant, deadline: Instant) -> Option<Duration> {
    let left = deadline.saturating_duration_since(now);
    (!left.is_zero()).then(|| left.min(ASK_TIME).max(SHORTEST_ASK))
}

/// The window that the mouse reaches at `place` in `window` or the windows
/// inside it, with the place in that window's client area, or `None` when
/// the mouse goes through `window` there to whatever is behind it.
///
/// It follows how Windows routes the mouse. Inside `window`'s client area
/// it looks at the child windows, topmost first, and passes over one that
/// is hidden, is disabled, or does not hold the place. The first child
/// that takes the place, itself or through a window inside it, gets the
/// click. When none does, `window` answers where the place is on it
/// (`WM_NCHITTEST`): in its client area, where a mouse message can name
/// it; nowhere the mouse stops, as a label or a group box answers, so the
/// click goes on to the window behind; or on another part, such as a title
/// bar, a border, or a scroll bar, which gets other messages that a click
/// does not send. A window gets [`ASK_TIME`] to answer, and no longer than
/// what is left until `walk`'s deadline.
fn under<I: Input>(
    input: &mut I,
    window: u64,
    place: (i32, i32),
    depth: usize,
    walk: Walk<'_>,
) -> Result<Option<Landing>, ActionError> {
    let Walk { app, deadline } = walk;
    let unreachable = || ActionError::Unreachable(app.to_owned());
    let too_slow = || ActionError::TooSlow(app.to_owned());
    let Some(deeper) = depth.checked_sub(1) else {
        return Err(unreachable());
    };
    let point = input.client_point(window, place.0, place.1)?;
    if inside(point, input.client_size(window)?) {
        let mut next = input.top_child(window)?;
        for _ in 0..MAX_SIBLINGS {
            let Some(child) = next else {
                break;
            };
            let style = input.style(child)?;
            if !hidden(style)
                && !disabled(style)
                && holds(input.bounds(child)?, place)
                && let Some(found) = under(input, child, place, deeper, walk)?
            {
                return Ok(Some(found));
            }
            next = input.below(child)?;
        }
        if next.is_some() {
            return Err(unreachable());
        }
    }
    let asked = short(place).ok_or_else(unreachable)?;
    let wait = ask_time(input.now(), deadline).ok_or_else(too_slow)?;
    let answer = input.hit_test(window, asked, wait).map_err(|error| {
        if error.kind() == io::ErrorKind::TimedOut {
            too_slow()
        } else {
            refused(error, app)
        }
    })?;
    match hit_code(answer) {
        HIT_THROUGH => Ok(None),
        HIT_CLIENT => short(point)
            .map(|at| Some(Landing { window, at }))
            .ok_or_else(unreachable),
        _ => Err(unreachable()),
    }
}

/// Whether a window with `style` is hidden.
const fn hidden(style: u32) -> bool {
    style & STYLE_VISIBLE == 0
}

/// Whether a window with `style` is disabled, so it takes no input.
const fn disabled(style: u32) -> bool {
    style & STYLE_DISABLED != 0
}

/// Whether Windows tells a window whose class has `class_style` about
/// double clicks.
const fn reports_double_clicks(class_style: u32) -> bool {
    class_style & CLASS_DOUBLE_CLICKS != 0
}

/// Whether `point` lies in a client area of `size`.
fn inside((x, y): (i32, i32), (width, height): (i32, i32)) -> bool {
    (0..width).contains(&x) && (0..height).contains(&y)
}

/// Whether a window at `bounds`, its left, top, right, and bottom edges,
/// holds `place`.
fn holds((left, top, right, bottom): (i32, i32, i32, i32), (x, y): (i32, i32)) -> bool {
    (left..right).contains(&x) && (top..bottom).contains(&y)
}

/// `place` as a message names one, or `None` when it is too far out for a
/// message.
fn short((x, y): (i32, i32)) -> Option<(i16, i16)> {
    Some((i16::try_from(x).ok()?, i16::try_from(y).ok()?))
}

/// What a window answered to where a place is on it: the low 32 bits of
/// its answer, which is all a 32-bit program's window gives.
const fn hit_code(answer: usize) -> i32 {
    let [first, second, third, fourth, ..] = answer.to_le_bytes();
    i32::from_le_bytes([first, second, third, fourth])
}

/// Why a window did not take what a click sent it: Windows blocks messages
/// to a window whose program runs with more rights, and anything else is
/// the operating system's own error.
fn refused(error: io::Error, app: &str) -> ActionError {
    if error.kind() == io::ErrorKind::PermissionDenied {
        ActionError::MessageBlocked(app.to_owned())
    } else {
        error.into()
    }
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
            return Err(refused(error, app));
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

    use appytappy_config::action::Modifier;

    use super::*;
    use crate::Stroke;

    /// A window style bit that says nothing about the mouse, which every
    /// window here has, so only the bits that matter are read.
    const STYLE_OTHER: u32 = 0x0001_0000;

    /// What a window answers for a place on its border: `HTBORDER`.
    const HIT_BORDER: usize = 18;

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
        hidden: bool,
        disabled: bool,
        /// What it answers for a place in its client area, when not that
        /// the place is there.
        answers: Option<usize>,
        /// Its class's style bits.
        class_style: u32,
    }

    impl Pane {
        /// Its left, top, right, and bottom edges, border included.
        fn edges(&self) -> (i32, i32, i32, i32) {
            let before = |origin: i32| origin.checked_sub(self.border).expect("on the screen");
            let after = |origin: i32, size: i32| {
                origin
                    .checked_add(size)
                    .and_then(|end| end.checked_add(self.border))
                    .expect("on the screen")
            };
            (
                before(self.origin.0),
                before(self.origin.1),
                after(self.origin.0, self.size.0),
                after(self.origin.1, self.size.1),
            )
        }
    }

    /// The windows a click sent as messages finds.
    #[derive(Default, PartialEq, Eq)]
    enum Windows {
        /// Those in [`Desk::panes`].
        #[default]
        Panes,
        /// Every window has another child under every place.
        Endless,
        /// Window 9 has hidden children without end.
        Crowded,
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
        /// How far the windows' own pixels start from the screen's.
        own: (i32, i32),
        /// Which windows a click sent as messages finds.
        windows: Windows,
        /// How asking a window where a place is on it fails, if it does.
        asking_fails: Option<io::ErrorKind>,
        /// When the clock started, once it was read.
        started: Option<Instant>,
        /// How long the clock has run since.
        elapsed: Duration,
        /// How long each window takes to answer where a place is on it.
        answer_takes: Duration,
        /// How long each window asked was given to answer.
        waits: Vec<Duration>,
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

        fn own_place(&mut self, _window: u64, x: i32, y: i32) -> io::Result<(i32, i32)> {
            Ok((
                x.checked_sub(self.own.0).expect("on the screen"),
                y.checked_sub(self.own.1).expect("on the screen"),
            ))
        }

        fn client_point(&mut self, window: u64, x: i32, y: i32) -> io::Result<(i32, i32)> {
            if self.windows == Windows::Endless {
                return Ok((0, 0));
            }
            let (left, top) = self.pane(window)?.origin;
            Ok((
                x.checked_sub(left).expect("on the screen"),
                y.checked_sub(top).expect("on the screen"),
            ))
        }

        fn client_size(&mut self, window: u64) -> io::Result<(i32, i32)> {
            if self.windows == Windows::Endless {
                return Ok((1, 1));
            }
            Ok(self.pane(window)?.size)
        }

        fn top_child(&mut self, window: u64) -> io::Result<Option<u64>> {
            if self.windows == Windows::Endless {
                return Ok(Some(window.checked_add(1).expect("few windows")));
            }
            if self.windows == Windows::Crowded && window == 9 {
                return Ok(Some(1000));
            }
            Ok(self.pane(window)?.children.first().copied())
        }

        fn below(&mut self, window: u64) -> io::Result<Option<u64>> {
            if self.windows == Windows::Endless {
                return Ok(None);
            }
            if self.windows == Windows::Crowded {
                return Ok(Some(window.checked_add(1).expect("few windows")));
            }
            Ok(self.panes.values().find_map(|pane| {
                let mut after = pane.children.iter().skip_while(|child| **child != window);
                after.next()?;
                after.next().copied()
            }))
        }

        fn style(&mut self, window: u64) -> io::Result<u32> {
            if self.windows == Windows::Endless {
                return Ok(STYLE_VISIBLE | STYLE_OTHER);
            }
            if self.windows == Windows::Crowded && window != 9 {
                return Ok(STYLE_OTHER);
            }
            let pane = self.pane(window)?;
            let visible = if pane.hidden { 0 } else { STYLE_VISIBLE };
            let disabled = if pane.disabled { STYLE_DISABLED } else { 0 };
            Ok(visible | disabled | STYLE_OTHER)
        }

        fn bounds(&mut self, window: u64) -> io::Result<(i32, i32, i32, i32)> {
            if self.windows == Windows::Endless {
                return Ok((i32::MIN, i32::MIN, i32::MAX, i32::MAX));
            }
            Ok(self.pane(window)?.edges())
        }

        fn now(&mut self) -> Instant {
            self.started
                .get_or_insert_with(Instant::now)
                .checked_add(self.elapsed)
                .expect("a short time")
        }

        fn hit_test(&mut self, window: u64, at: (i16, i16), wait: Duration) -> io::Result<usize> {
            if let Some(kind) = self.asking_fails {
                return Err(kind.into());
            }
            self.waits.push(wait);
            self.elapsed = self
                .elapsed
                .checked_add(self.answer_takes)
                .expect("a short time");
            let place = (i32::from(at.0), i32::from(at.1));
            let pane = self.pane(window)?;
            let (left, top) = pane.origin;
            let client = inside(
                (
                    place.0.checked_sub(left).expect("on the screen"),
                    place.1.checked_sub(top).expect("on the screen"),
                ),
                pane.size,
            );
            Ok(match pane.answers {
                Some(answer) if client => answer,
                _ if client => 1,
                _ => HIT_BORDER,
            })
        }

        fn class_style(&mut self, window: u64) -> io::Result<u32> {
            Ok(self.pane(window)?.class_style)
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
        let events = |button, double_clicks| -> Vec<MouseEvent> {
            messages(button, (3, 4), double_clicks)
                .into_iter()
                .map(|message| {
                    assert_eq!(message.at, (3, 4));
                    message.event
                })
                .collect()
        };
        let (left, right) = (MouseButton::Left, MouseButton::Right);
        for double_clicks in [false, true] {
            assert_eq!(
                events(ClickButton::Left, double_clicks),
                [
                    MouseEvent::Move,
                    MouseEvent::Down(left),
                    MouseEvent::Up(left)
                ]
            );
            assert_eq!(
                events(ClickButton::Right, double_clicks),
                [
                    MouseEvent::Move,
                    MouseEvent::Down(right),
                    MouseEvent::Up(right)
                ]
            );
        }
        assert_eq!(
            events(ClickButton::Double, true),
            [
                MouseEvent::Move,
                MouseEvent::Down(left),
                MouseEvent::Up(left),
                MouseEvent::DoubleDown(left),
                MouseEvent::Up(left)
            ],
            "a window that is told about double clicks gets one"
        );
        assert_eq!(
            events(ClickButton::Double, false),
            [
                MouseEvent::Move,
                MouseEvent::Down(left),
                MouseEvent::Up(left),
                MouseEvent::Down(left),
                MouseEvent::Up(left)
            ],
            "any other window gets two presses, as from the mouse"
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
            assert_eq!(pack(at), place, "{at:?}");
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
    }

    #[test]
    fn a_double_click_s_second_press_is_what_the_window_s_class_is_told() {
        // Other class style bits say nothing about double clicks.
        for (class_style, second) in [
            (0x0003, "post 9 Down(Left) 40,50"),
            (0x0008, "post 9 DoubleDown(Left) 40,50"),
            (0x000B, "post 9 DoubleDown(Left) 40,50"),
            (0, "post 9 Down(Left) 40,50"),
        ] {
            let mut desk = behind();
            desk.panes.entry(9).or_default().class_style = class_style;
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
                    second,
                    "hold 20ms",
                    "post 9 Up(Left) 40,50"
                ],
                "{class_style:#x}"
            );
        }
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
                ..Pane::default()
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

    /// The first thing a click sent as messages at `at` did to `desk`, or
    /// why it was not sent.
    fn first(desk: &mut Desk, at: (i32, i32)) -> Result<String, ActionError> {
        let click_ = PlacedClick {
            at,
            ..posted(ClickButton::Right)
        };
        click(desk, &click_, &watched())?;
        assert_eq!(desk.log.len(), 5, "{:?}", desk.log);
        Ok(desk.log.first().cloned().expect("a message"))
    }

    fn unreachable(sent: &Result<String, ActionError>) -> bool {
        matches!(sent, Err(ActionError::Unreachable(app)) if app == "Target App")
    }

    #[test]
    fn messages_go_to_the_deepest_window_under_the_place() {
        for (at, message) in [
            ((100, 200), "post 11 Move 5,10"),
            ((92, 182), "post 10 Move 2,2"),
            ((61, 151), "post 9 Move 1,1"),
            // Just outside window 10's border, on each side.
            ((87, 200), "post 9 Move 27,50"),
            ((142, 200), "post 9 Move 82,50"),
            ((100, 177), "post 9 Move 40,27"),
            ((100, 222), "post 9 Move 40,72"),
        ] {
            let mut desk = nested();
            let sent = first(&mut desk, at);
            assert_eq!(sent.as_deref().ok(), Some(message), "{at:?}: {sent:?}");
        }
    }

    #[test]
    fn the_topmost_child_that_takes_the_place_gets_the_click() {
        // Windows 12 and 13 overlap inside window 9, with 12 on top.
        let overlapping = || {
            let mut desk = behind();
            desk.panes.entry(9).or_default().children = vec![12, 13];
            for window in [12, 13] {
                desk.panes.insert(
                    window,
                    Pane {
                        origin: (90, 180),
                        size: (50, 40),
                        ..Pane::default()
                    },
                );
            }
            desk
        };
        let mut desk = overlapping();
        assert_eq!(
            first(&mut desk, (100, 200)).ok().as_deref(),
            Some("post 12 Move 10,20")
        );
        // The mouse goes through a label or a group box, in either form a
        // program gives the answer in, to the window behind it.
        for through in [0xFFFF_FFFF, usize::MAX] {
            let mut desk = overlapping();
            desk.panes.entry(12).or_default().answers = Some(through);
            assert_eq!(
                first(&mut desk, (100, 200)).ok().as_deref(),
                Some("post 13 Move 10,20"),
                "{through:#x}"
            );
            // With nothing behind it, the window around it gets the click.
            desk = overlapping();
            for window in [12, 13] {
                desk.panes.entry(window).or_default().answers = Some(through);
            }
            assert_eq!(
                first(&mut desk, (100, 200)).ok().as_deref(),
                Some("post 9 Move 40,50"),
                "{through:#x}"
            );
        }
        // A hidden or a disabled child is not there for the mouse.
        let mut desk = overlapping();
        desk.panes.entry(12).or_default().hidden = true;
        assert_eq!(
            first(&mut desk, (100, 200)).ok().as_deref(),
            Some("post 13 Move 10,20")
        );
        let mut desk = overlapping();
        desk.panes.entry(12).or_default().disabled = true;
        desk.panes.entry(13).or_default().disabled = true;
        assert_eq!(
            first(&mut desk, (100, 200)).ok().as_deref(),
            Some("post 9 Move 40,50")
        );
    }

    #[test]
    fn a_place_no_mouse_message_can_name_gets_no_message() {
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
            let sent = first(&mut desk, at);
            assert_eq!(sent.is_ok(), lands, "{at:?}: {sent:?}");
            if !lands {
                assert!(unreachable(&sent), "{at:?}: {sent:?}");
                assert_eq!(desk.log, Vec::<String>::new(), "{at:?}");
            }
        }
        // A child's border belongs to the child, but not to its client
        // area, on each side.
        for at in [(88, 200), (141, 200), (100, 178), (100, 221)] {
            let mut desk = nested();
            let sent = first(&mut desk, at);
            assert!(unreachable(&sent), "{at:?}: {sent:?}");
            assert_eq!(desk.log, Vec::<String>::new(), "{at:?}");
        }
        // A child that reaches past its parent's client area takes no click
        // out there.
        let mut desk = behind();
        desk.panes.entry(9).or_default().children = vec![12];
        desk.panes.insert(
            12,
            Pane {
                origin: (100, 240),
                size: (50, 40),
                ..Pane::default()
            },
        );
        assert_eq!(
            first(&mut desk, (110, 245)).ok().as_deref(),
            Some("post 12 Move 10,5")
        );
        let sent = first(&mut desk, (110, 260));
        assert!(unreachable(&sent), "{sent:?}");
        // The mouse goes through the click's own window, to another one.
        let mut desk = behind();
        desk.panes.entry(9).or_default().answers = Some(usize::MAX);
        let sent = first(&mut desk, (100, 200));
        assert!(unreachable(&sent), "{sent:?}");
        assert_eq!(desk.log, Vec::<String>::new());
    }

    #[test]
    fn a_disabled_window_gets_no_message() {
        let mut desk = nested();
        desk.panes.entry(9).or_default().disabled = true;
        assert!(matches!(
            click(&mut desk, &posted(ClickButton::Left), &watched()),
            Err(ActionError::WindowDisabled(app)) if app == "Target App"
        ));
        assert_eq!(desk.log, Vec::<String>::new());
    }

    #[test]
    fn the_place_is_counted_in_the_window_s_own_pixels() {
        let mut desk = nested();
        desk.own = (1000, 2000);
        assert_eq!(
            first(&mut desk, (1100, 2200)).ok().as_deref(),
            Some("post 11 Move 5,10")
        );
        // A place too far for a message to name is not sent: on the screen,
        // and in a window.
        let mut desk = behind();
        desk.panes.entry(9).or_default().size = (40_000, 100);
        let sent = first(&mut desk, (39_060, 200));
        assert!(unreachable(&sent), "{sent:?}");
        let mut desk = behind();
        desk.panes.insert(
            9,
            Pane {
                origin: (-30_000, 150),
                size: (70_000, 100),
                ..Pane::default()
            },
        );
        assert_eq!(
            first(&mut desk, (2_767, 200)).ok().as_deref(),
            Some("post 9 Move 32767,50")
        );
        let sent = first(&mut desk, (2_768, 200));
        assert!(unreachable(&sent), "{sent:?}");
        assert_eq!(desk.log.len(), 5, "only the first was sent");
    }

    #[test]
    fn the_windows_asked_have_half_a_second_in_all_to_answer() {
        // The mouse goes through windows 12 and 13 to window 9, so a click
        // asks all three, each of which takes `each` to answer.
        let asked = |each: Duration| {
            let mut desk = behind();
            desk.panes.entry(9).or_default().children = vec![12, 13];
            for window in [12, 13] {
                desk.panes.insert(
                    window,
                    Pane {
                        origin: (90, 180),
                        size: (50, 40),
                        answers: Some(usize::MAX),
                        ..Pane::default()
                    },
                );
            }
            desk.answer_takes = each;
            let sent = first(&mut desk, (100, 200));
            (sent, desk.waits, desk.log)
        };
        let ms = Duration::from_millis;
        // Each window gets 200 ms, and the last one only what is left.
        let (sent, waits, _) = asked(ms(200));
        assert_eq!(sent.ok().as_deref(), Some("post 9 Move 40,50"));
        assert_eq!(waits, [ms(200), ms(200), ms(100)]);
        let (sent, waits, _) = asked(ms(249));
        assert_eq!(sent.ok().as_deref(), Some("post 9 Move 40,50"));
        assert_eq!(waits, [ms(200), ms(200), ms(2)]);
        // Less than a millisecond left is still a wait, of a whole one.
        let (sent, waits, _) = asked(Duration::from_micros(249_800));
        assert_eq!(sent.ok().as_deref(), Some("post 9 Move 40,50"));
        assert_eq!(waits, [ms(200), ms(200), ms(1)]);
        let nearly = Duration::from_nanos(249_999_999);
        assert_eq!(asked(nearly).1, [ms(200), ms(200), ms(1)]);
        // With nothing left, the last window is not asked, and nothing is
        // sent.
        let (sent, waits, log) = asked(ms(250));
        assert!(
            matches!(&sent, Err(ActionError::TooSlow(app)) if app == "Target App"),
            "{sent:?}"
        );
        assert_eq!(waits, [ms(200), ms(200)]);
        assert_eq!(log, Vec::<String>::new());
    }

    #[test]
    fn windows_without_end_get_no_message() {
        let mut desk = behind();
        desk.windows = Windows::Endless;
        let sent = first(&mut desk, (100, 200));
        assert!(unreachable(&sent), "{sent:?}");
        assert_eq!(desk.log, Vec::<String>::new());
        let mut desk = behind();
        desk.windows = Windows::Crowded;
        let sent = first(&mut desk, (100, 200));
        assert!(unreachable(&sent), "{sent:?}");
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
        // Asking it where the place is fails the same ways.
        let mut desk = behind();
        desk.asking_fails = Some(io::ErrorKind::PermissionDenied);
        assert!(matches!(
            click(&mut desk, &posted(ClickButton::Left), &watched()),
            Err(ActionError::MessageBlocked(app)) if app == "Target App"
        ));
        desk.asking_fails = Some(io::ErrorKind::TimedOut);
        assert!(matches!(
            click(&mut desk, &posted(ClickButton::Left), &watched()),
            Err(ActionError::TooSlow(app)) if app == "Target App"
        ));
        desk.asking_fails = Some(io::ErrorKind::BrokenPipe);
        assert!(matches!(
            click(&mut desk, &posted(ClickButton::Left), &watched()),
            Err(ActionError::Os(error)) if error.kind() == io::ErrorKind::BrokenPipe
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
            desk.panes.entry(9).or_default().class_style = CLASS_DOUBLE_CLICKS;
            desk.posts_fail = fails;
            let sent = click(&mut desk, &posted(ClickButton::Double), &watched());
            assert!(
                matches!(&sent, Err(ActionError::Os(error)) if error.kind() == io::ErrorKind::BrokenPipe),
                "{sent:?}"
            );
            assert_eq!(desk.log, log);
        }
        // A window that closed has nothing to ask.
        let mut desk = behind();
        desk.panes.clear();
        assert!(matches!(
            click(&mut desk, &posted(ClickButton::Left), &watched()),
            Err(ActionError::Os(error)) if error.kind() == io::ErrorKind::NotFound
        ));
    }
}
