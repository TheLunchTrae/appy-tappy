//! Actions: the side effects a rule runs when it fires.
//!
//! These are validated types; `docs/actions.md` explains the v1 set.

use std::error::Error;
use std::fmt;

/// The longest app name, in bytes.
const MAX_APP_NAME_LEN: usize = 256;

/// An app as Windows reports it: a media session's app ID, such as
/// `Spotify.exe`, or a process's executable name.
///
/// A name is 1 to 256 bytes with no control characters. It matches a
/// reported name regardless of ASCII case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppName(Box<str>);

impl AppName {
    /// Validates `name`.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidAppName`] when `name` is empty, too long, or holds a
    /// control character.
    pub fn new(name: &str) -> Result<Self, InvalidAppName> {
        if name.is_empty() || name.len() > MAX_APP_NAME_LEN || name.chars().any(char::is_control) {
            return Err(InvalidAppName);
        }
        Ok(Self(name.into()))
    }

    /// The name as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether `reported`, a name Windows reports, names this app.
    #[must_use]
    pub fn matches(&self, reported: &str) -> bool {
        self.0.eq_ignore_ascii_case(reported)
    }
}

/// An app name was empty, longer than 256 bytes, or held a control character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidAppName;

impl fmt::Display for InvalidAppName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "an app name needs 1 to {MAX_APP_NAME_LEN} bytes and no control characters"
        )
    }
}

impl Error for InvalidAppName {}

/// A volume level from silent (0) to full (1).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct VolumeLevel(f32);

impl VolumeLevel {
    /// The level `percent` of full volume, or `None` above 100.
    #[must_use]
    pub fn from_percent(percent: u8) -> Option<Self> {
        (percent <= 100).then(|| Self(f32::from(percent) / 100.0))
    }

    /// The level `scalar`, as Windows reports it, or `None` outside 0 to 1.
    #[must_use]
    pub fn from_scalar(scalar: f32) -> Option<Self> {
        (0.0..=1.0).contains(&scalar).then_some(Self(scalar))
    }

    /// The level from 0 to 1.
    #[must_use]
    pub const fn scalar(self) -> f32 {
        self.0
    }
}

/// Which media session a media action controls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaTarget {
    /// The session Windows considers current, which its media overlay shows.
    Current,
    /// The session of one app, such as `Spotify.exe`.
    App(AppName),
}

/// What a media action does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaCommand {
    /// Start or resume playback.
    Play,
    /// Pause playback.
    Pause,
    /// Skip to the next track.
    Next,
    /// Go back to the previous track.
    Previous,
}

/// Whose volume a volume action sets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VolumeTarget {
    /// The default playback device.
    System,
    /// Every audio session of one executable, such as `Spotify.exe`, on the
    /// default playback device.
    App(AppName),
}

/// A side effect from the runtime's fixed set.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Controls a media session.
    Media {
        /// The session to control.
        target: MediaTarget,
        /// What to do.
        command: MediaCommand,
    },
    /// Sets a volume.
    SetVolume {
        /// Whose volume to set.
        target: VolumeTarget,
        /// The level to set.
        level: VolumeLevel,
    },
}

impl Action {
    /// Whether a rule can undo this action on release: pausing or playing
    /// can be reversed and a volume can be set back, but a skipped track
    /// cannot be unskipped.
    #[must_use]
    pub const fn restorable(&self) -> bool {
        !matches!(
            self,
            Self::Media {
                command: MediaCommand::Next | MediaCommand::Previous,
                ..
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_names_are_bounded_and_match_ignoring_case() {
        let longest = "a".repeat(MAX_APP_NAME_LEN);
        let too_long = "a".repeat(MAX_APP_NAME_LEN.checked_add(1).expect("small"));
        for valid in [
            "Spotify.exe",
            "SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify",
            longest.as_str(),
        ] {
            assert_eq!(
                AppName::new(valid).map(|name| name.as_str().to_owned()),
                Ok(valid.to_owned())
            );
        }
        for invalid in ["", "Spotify\n.exe", "\u{7}", too_long.as_str()] {
            assert_eq!(AppName::new(invalid), Err(InvalidAppName), "{invalid:?}");
        }
        let spotify = AppName::new("Spotify.exe").expect("valid");
        assert!(spotify.matches("spotify.EXE"));
        assert!(!spotify.matches("Spotify"));
        assert!(InvalidAppName.to_string().contains("256"));
    }

    #[test]
    fn volume_levels_run_from_zero_to_one() {
        assert_eq!(
            VolumeLevel::from_percent(0).map(VolumeLevel::scalar),
            Some(0.0)
        );
        assert_eq!(
            VolumeLevel::from_percent(25).map(VolumeLevel::scalar),
            Some(0.25)
        );
        assert_eq!(
            VolumeLevel::from_percent(100).map(VolumeLevel::scalar),
            Some(1.0)
        );
        assert_eq!(VolumeLevel::from_percent(101), None);
        assert_eq!(
            VolumeLevel::from_scalar(0.0).map(VolumeLevel::scalar),
            Some(0.0)
        );
        assert_eq!(
            VolumeLevel::from_scalar(1.0).map(VolumeLevel::scalar),
            Some(1.0)
        );
        for scalar in [-0.01, 1.01, f32::NAN] {
            assert_eq!(VolumeLevel::from_scalar(scalar), None);
        }
    }

    #[test]
    fn only_skips_cannot_be_restored() {
        let media = |command| Action::Media {
            target: MediaTarget::Current,
            command,
        };
        assert!(media(MediaCommand::Play).restorable());
        assert!(media(MediaCommand::Pause).restorable());
        assert!(!media(MediaCommand::Next).restorable());
        assert!(!media(MediaCommand::Previous).restorable());
        let level = VolumeLevel::from_percent(20).expect("valid");
        assert!(
            Action::SetVolume {
                target: VolumeTarget::System,
                level
            }
            .restorable()
        );
    }
}
