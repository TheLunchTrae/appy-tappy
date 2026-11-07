//! Key presses: the strokes that press a key chord and let it go.

use std::time::Duration;

use appytappy_config::action::{Key, KeyChord, Modifier};

/// One key that a key press moves, as [`strokes`] orders them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stroke {
    /// A modifier goes down (`true`) or up.
    Modifier(Modifier, bool),
    /// The key goes down (`true`) or up.
    Key(Key, bool),
}

impl Stroke {
    /// The same key going the other way: up for down, down for up.
    #[must_use]
    pub const fn reversed(self) -> Self {
        match self {
            Self::Modifier(modifier, down) => Self::Modifier(modifier, !down),
            Self::Key(key, down) => Self::Key(key, !down),
        }
    }
}

/// How long a key press holds its keys down, so an application that checks
/// the keyboard once a frame still sees them.
pub const HOLD: Duration = Duration::from_millis(40);

/// The keys pressing `keys` moves down, then the keys it moves up.
///
/// It goes the way a person presses a shortcut: the modifiers go down in
/// order, the key goes down and up, and the modifiers go up in reverse. A
/// modifier in `held`, which the user is holding already, is left alone, so
/// it stays down for them afterwards.
#[must_use]
pub fn strokes(keys: &KeyChord, held: &[Modifier]) -> (Vec<Stroke>, Vec<Stroke>) {
    let modifiers: Vec<Modifier> = keys
        .modifiers()
        .into_iter()
        .filter(|modifier| !held.contains(modifier))
        .collect();
    let down = modifiers
        .iter()
        .map(|&modifier| Stroke::Modifier(modifier, true))
        .chain([Stroke::Key(keys.key(), true)])
        .collect();
    let up = std::iter::once(Stroke::Key(keys.key(), false))
        .chain(
            modifiers
                .iter()
                .rev()
                .map(|&modifier| Stroke::Modifier(modifier, false)),
        )
        .collect();
    (down, up)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers_wrap_the_key_and_come_up_in_reverse() {
        let keys = KeyChord::parse("Shift+Ctrl+M").expect("valid");
        assert_eq!(
            strokes(&keys, &[]),
            (
                vec![
                    Stroke::Modifier(Modifier::Ctrl, true),
                    Stroke::Modifier(Modifier::Shift, true),
                    Stroke::Key(Key::Letter('M'), true),
                ],
                vec![
                    Stroke::Key(Key::Letter('M'), false),
                    Stroke::Modifier(Modifier::Shift, false),
                    Stroke::Modifier(Modifier::Ctrl, false),
                ]
            )
        );
        assert_eq!(
            strokes(&keys, &[Modifier::Shift, Modifier::Alt]),
            (
                vec![
                    Stroke::Modifier(Modifier::Ctrl, true),
                    Stroke::Key(Key::Letter('M'), true),
                ],
                vec![
                    Stroke::Key(Key::Letter('M'), false),
                    Stroke::Modifier(Modifier::Ctrl, false),
                ]
            ),
            "a modifier the user holds stays as it is"
        );
        let alone = KeyChord::parse("F9").expect("valid");
        assert_eq!(
            strokes(&alone, &[]),
            (
                vec![Stroke::Key(Key::Function(9), true)],
                vec![Stroke::Key(Key::Function(9), false)]
            )
        );
        assert_eq!(
            Stroke::Modifier(Modifier::Win, true).reversed(),
            Stroke::Modifier(Modifier::Win, false)
        );
        assert_eq!(
            Stroke::Key(Key::Enter, false).reversed(),
            Stroke::Key(Key::Enter, true)
        );
    }
}
