//! Clicks: the button presses a click sends, and where on the screen the
//! runtime placed it.

use std::time::Duration;

use appytappy_config::anti_cheat::ProcessId;
use appytappy_config::click::ClickButton;

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
pub const CLICK_HOLD: Duration = Duration::from_millis(40);

/// How long each press of a click lasts before the next, so an application
/// that reads the buttons once a frame sees the button down, and for a
/// double click up again between the two.
pub const PRESS_HOLD: Duration = Duration::from_millis(20);

/// The presses that click `button`: down and up, twice for a double click.
#[must_use]
pub fn presses(button: ClickButton) -> Vec<Press> {
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
    /// Whether the window that was in front gets the focus back after.
    pub return_focus: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(button: MouseButton, down: bool) -> Press {
        Press { button, down }
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
}
