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
                "the sound did not play: the file is missing or not a WAV file, or no playback \
                 device is connected",
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
            | Self::SoundNotPlayed => None,
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
        ] {
            assert!(error.to_string().contains(words), "{error}");
            assert!(error.source().is_none());
        }
    }
}
