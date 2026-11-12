//! Why an action failed.

use std::error::Error;
use std::fmt;
use std::io;

/// An action could not run.
#[derive(Debug)]
pub enum ActionError {
    /// No media session matched the target.
    NoMediaSession,
    /// The media session did not take the command.
    Refused,
    /// The volume target had no audio session.
    NoAudio,
    /// A key press did not press its keys, because the named executable's
    /// window did not have the keyboard focus.
    NotInFront(String),
    /// The operating system took only `sent` of `of` key events.
    KeysDropped {
        /// How many it took.
        sent: usize,
        /// How many there were.
        of: usize,
    },
    /// Windows did not start playing the sound file.
    SoundNotPlayed,
    /// A click did not click, because neither the watched application nor
    /// the one it clicks in had the keyboard focus.
    ClickNotInFront {
        /// The watched application's executable.
        watched: String,
        /// The application it clicks in.
        target: String,
    },
    /// A click did not click, because another window covers its place in
    /// the named application's window.
    Covered(String),
    /// The operating system took only `sent` of `of` mouse button events.
    ButtonsDropped {
        /// How many it took.
        sent: usize,
        /// How many there were.
        of: usize,
    },
    /// A click did not click, because a mouse button or a modifier key was
    /// held down.
    HeldDown,
    /// A click did not click, because Windows did not bring the named
    /// application's window to the front.
    NotBroughtToFront(String),
    /// A click did not click, because the cursor could not reach its place
    /// in the named application's window, such as while another window
    /// confines the cursor.
    CursorBlocked(String),
    /// A click sent as messages was not sent, because its place in the
    /// named application's window is not in a window's client area, such as
    /// on a title bar, a border, or a scroll bar, or is too far in for a
    /// mouse message to name.
    Unreachable(String),
    /// A click sent as messages was not sent, because Windows blocks
    /// messages to the named application's window: its program runs with
    /// more rights than AppyTappy, such as as administrator.
    MessageBlocked(String),
    /// A click reached the journal without a place: the runtime places
    /// clicks and runs them through [`crate::click`].
    Unplaced,
    /// The click landed, but the window that was in front did not get the
    /// focus back.
    FocusNotReturned,
    /// The operating system failed.
    Os(io::Error),
}

impl fmt::Display for ActionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoMediaSession => formatter.write_str("no media session matches the target"),
            Self::Refused => formatter.write_str("the media session did not take the command"),
            Self::NoAudio => formatter.write_str("the volume target has no audio session"),
            Self::NotInFront(watched) => write!(
                formatter,
                "the keys were not pressed, because {watched} was not the window in front"
            ),
            Self::KeysDropped { sent, of } => {
                write!(formatter, "Windows took only {sent} of {of} key events")
            },
            Self::SoundNotPlayed => formatter.write_str(
                "the sound did not play: the file is missing or Windows could not play it",
            ),
            Self::ClickNotInFront { watched, target } => write!(
                formatter,
                "the click did not click, because neither {watched} nor {target} was the window in front"
            ),
            Self::Covered(target) => write!(
                formatter,
                "the click did not click, because another window covers its place in {target}"
            ),
            Self::ButtonsDropped { sent, of } => {
                write!(formatter, "Windows took only {sent} of {of} mouse button events")
            },
            Self::HeldDown => formatter.write_str(
                "the click did not click, because a mouse button or a modifier key was held down",
            ),
            Self::NotBroughtToFront(target) => write!(
                formatter,
                "the click did not click, because Windows did not bring {target} to the front"
            ),
            Self::CursorBlocked(target) => write!(
                formatter,
                "the click did not click, because the cursor could not reach its place in {target}"
            ),
            Self::Unreachable(target) => write!(
                formatter,
                "the click was not sent, because a click sent to the window cannot land at its \
                 place in {target}, such as on a title bar, a border, or a scroll bar"
            ),
            Self::MessageBlocked(target) => write!(
                formatter,
                "the click was not sent, because Windows blocks messages to {target}, which runs \
                 with more rights than AppyTappy, such as as administrator"
            ),
            Self::Unplaced => formatter.write_str("the click was not placed on the screen"),
            Self::FocusNotReturned => formatter.write_str(
                "the click landed, but Windows did not give the focus back to the window that was in front",
            ),
            Self::Os(error) => write!(formatter, "the operating system failed: {error}"),
        }
    }
}

impl Error for ActionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Os(error) => Some(error),
            Self::NoMediaSession
            | Self::Refused
            | Self::NoAudio
            | Self::NotInFront(_)
            | Self::KeysDropped { .. }
            | Self::SoundNotPlayed
            | Self::ClickNotInFront { .. }
            | Self::Covered(_)
            | Self::ButtonsDropped { .. }
            | Self::HeldDown
            | Self::NotBroughtToFront(_)
            | Self::CursorBlocked(_)
            | Self::Unreachable(_)
            | Self::MessageBlocked(_)
            | Self::Unplaced
            | Self::FocusNotReturned => None,
        }
    }
}

impl From<io::Error> for ActionError {
    fn from(error: io::Error) -> Self {
        Self::Os(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_say_what_failed_and_keep_their_cause() {
        let error = ActionError::from(io::Error::other("device gone"));
        assert!(error.to_string().contains("device gone"));
        assert!(error.source().is_some());
        for (error, words) in [
            (ActionError::NoMediaSession, "no media session"),
            (ActionError::Refused, "did not take"),
            (ActionError::NoAudio, "no audio session"),
            (ActionError::SoundNotPlayed, "did not play"),
            (
                ActionError::ClickNotInFront {
                    watched: "Watched.exe".to_owned(),
                    target: "Target App".to_owned(),
                },
                "neither Watched.exe nor Target App",
            ),
            (
                ActionError::Covered("Target App".to_owned()),
                "covers its place in Target App",
            ),
            (
                ActionError::ButtonsDropped { sent: 1, of: 4 },
                "only 1 of 4 mouse",
            ),
            (ActionError::FocusNotReturned, "focus back"),
            (ActionError::HeldDown, "held down"),
            (
                ActionError::NotBroughtToFront("Target App".to_owned()),
                "bring Target App to the front",
            ),
            (
                ActionError::CursorBlocked("Target App".to_owned()),
                "reach its place in Target App",
            ),
            (
                ActionError::Unreachable("Target App".to_owned()),
                "cannot land at its place in Target App",
            ),
            (
                ActionError::MessageBlocked("Target App".to_owned()),
                "blocks messages to Target App",
            ),
            (ActionError::Unplaced, "not placed"),
        ] {
            assert!(error.to_string().contains(words), "{error}");
            assert!(error.source().is_none());
        }
    }
}
