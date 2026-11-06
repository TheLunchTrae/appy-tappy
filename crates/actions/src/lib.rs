//! Windows actions the runtime performs when a rule fires.
//!
//! [`perform`] runs one action through [`Controls`] and returns what a paired
//! action needs to undo it; [`restore`] undoes it. The Windows adapter in
//! `win` implements [`Controls`]. `docs/actions.md` explains the v1 set.
//!
//! See `docs/architecture.md` for this crate's boundaries.

use std::error::Error;
use std::fmt;
use std::io;

use appytappy_config::action::{Action, MediaCommand, MediaTarget, VolumeLevel, VolumeTarget};

#[cfg(windows)]
pub mod win;

/// A media session as Windows reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaSession {
    /// The session's app ID, such as `Spotify.exe`.
    pub app: String,
    /// Whether it is playing.
    pub playing: bool,
}

/// The operating system controls that actions use.
pub trait Controls {
    /// Finds the media session `target` names.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn media_session(&mut self, target: &MediaTarget) -> io::Result<Option<MediaSession>>;

    /// Sends `command` to the media session whose app ID is `app`. Returns
    /// whether a session took it.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn media_command(&mut self, app: &str, command: MediaCommand) -> io::Result<bool>;

    /// Reads the volume `target` names, or `None` when it has no audio.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn volume(&mut self, target: &VolumeTarget) -> io::Result<Option<VolumeLevel>>;

    /// Sets the volume `target` names. Returns whether it had audio to set.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn set_volume(&mut self, target: &VolumeTarget, level: VolumeLevel) -> io::Result<bool>;
}

/// What undoes an action that changed something.
#[derive(Debug, Clone, PartialEq)]
pub enum Restore {
    /// Send `command` to the media session of `app`.
    Media {
        /// The session's app ID.
        app: String,
        /// The command that undoes the action.
        command: MediaCommand,
    },
    /// Set the volume of `target` back to `level`.
    Volume {
        /// Whose volume to set.
        target: VolumeTarget,
        /// The level before the action.
        level: VolumeLevel,
    },
}

/// Runs `action` and returns what undoes it, if it changed anything a paired
/// action can restore.
///
/// Pausing restores only a session that was playing, and playing restores
/// only one that was not. Skipping tracks has nothing to restore.
///
/// # Errors
///
/// Returns [`ActionError`] when the target is missing, refuses, or the
/// operating system fails.
pub fn perform(
    controls: &mut impl Controls,
    action: &Action,
) -> Result<Option<Restore>, ActionError> {
    match action {
        Action::Media { target, command } => {
            let session = controls
                .media_session(target)?
                .ok_or(ActionError::NoMediaSession)?;
            let undo = match command {
                MediaCommand::Pause if session.playing => Some(MediaCommand::Play),
                MediaCommand::Play if !session.playing => Some(MediaCommand::Pause),
                MediaCommand::Pause | MediaCommand::Play => return Ok(None),
                MediaCommand::Next | MediaCommand::Previous => None,
            };
            send(controls, &session.app, *command)?;
            Ok(undo.map(|command| Restore::Media {
                app: session.app,
                command,
            }))
        },
        Action::SetVolume { target, level } => {
            let prior = controls.volume(target)?.ok_or(ActionError::NoAudio)?;
            set(controls, target, *level)?;
            Ok(Some(Restore::Volume {
                target: target.clone(),
                level: prior,
            }))
        },
    }
}

/// Undoes an action that [`perform`] ran.
///
/// # Errors
///
/// Returns [`ActionError`] when the target is gone, refuses, or the
/// operating system fails.
pub fn restore(controls: &mut impl Controls, restore: &Restore) -> Result<(), ActionError> {
    match restore {
        Restore::Media { app, command } => send(controls, app, *command),
        Restore::Volume { target, level } => set(controls, target, *level),
    }
}

fn send(controls: &mut impl Controls, app: &str, command: MediaCommand) -> Result<(), ActionError> {
    if controls.media_command(app, command)? {
        Ok(())
    } else {
        Err(ActionError::Refused)
    }
}

fn set(
    controls: &mut impl Controls,
    target: &VolumeTarget,
    level: VolumeLevel,
) -> Result<(), ActionError> {
    if controls.set_volume(target, level)? {
        Ok(())
    } else {
        Err(ActionError::NoAudio)
    }
}

/// An action could not run.
#[derive(Debug)]
pub enum ActionError {
    /// No media session matched the target.
    NoMediaSession,
    /// The media session did not take the command.
    Refused,
    /// The volume target had no audio session.
    NoAudio,
    /// The operating system failed.
    Os(io::Error),
}

impl fmt::Display for ActionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoMediaSession => formatter.write_str("no media session matches the target"),
            Self::Refused => formatter.write_str("the media session did not take the command"),
            Self::NoAudio => formatter.write_str("the volume target has no audio session"),
            Self::Os(error) => write!(formatter, "the operating system failed: {error}"),
        }
    }
}

impl Error for ActionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Os(error) => Some(error),
            Self::NoMediaSession | Self::Refused | Self::NoAudio => None,
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
    use appytappy_config::action::AppName;

    use super::*;

    /// Controls that play back one media session and one volume, and record
    /// every change.
    #[derive(Default)]
    struct Fake {
        session: Option<MediaSession>,
        refuse: bool,
        volume: Option<VolumeLevel>,
        sent: Vec<(String, MediaCommand)>,
        set: Vec<VolumeLevel>,
    }

    impl Controls for Fake {
        fn media_session(&mut self, _target: &MediaTarget) -> io::Result<Option<MediaSession>> {
            Ok(self.session.clone())
        }

        fn media_command(&mut self, app: &str, command: MediaCommand) -> io::Result<bool> {
            self.sent.push((app.to_owned(), command));
            Ok(!self.refuse)
        }

        fn volume(&mut self, _target: &VolumeTarget) -> io::Result<Option<VolumeLevel>> {
            Ok(self.volume)
        }

        fn set_volume(&mut self, _target: &VolumeTarget, level: VolumeLevel) -> io::Result<bool> {
            self.set.push(level);
            Ok(self.volume.is_some())
        }
    }

    fn spotify(playing: bool) -> Fake {
        Fake {
            session: Some(MediaSession {
                app: "Spotify.exe".to_owned(),
                playing,
            }),
            ..Fake::default()
        }
    }

    fn media(command: MediaCommand) -> Action {
        Action::Media {
            target: MediaTarget::Current,
            command,
        }
    }

    fn percent(level: u8) -> VolumeLevel {
        VolumeLevel::from_percent(level).expect("at most 100")
    }

    #[test]
    fn pausing_a_playing_session_restores_by_playing_it() {
        let mut controls = spotify(true);
        let undo = perform(&mut controls, &media(MediaCommand::Pause)).expect("paused");
        assert_eq!(
            undo,
            Some(Restore::Media {
                app: "Spotify.exe".to_owned(),
                command: MediaCommand::Play
            })
        );
        restore(&mut controls, &undo.expect("restorable")).expect("resumed");
        assert_eq!(
            controls.sent,
            [
                ("Spotify.exe".to_owned(), MediaCommand::Pause),
                ("Spotify.exe".to_owned(), MediaCommand::Play)
            ]
        );
    }

    #[test]
    fn playing_a_paused_session_restores_by_pausing_it() {
        let mut controls = spotify(false);
        let undo = perform(&mut controls, &media(MediaCommand::Play)).expect("played");
        assert_eq!(
            undo,
            Some(Restore::Media {
                app: "Spotify.exe".to_owned(),
                command: MediaCommand::Pause
            })
        );
        assert_eq!(
            controls.sent,
            [("Spotify.exe".to_owned(), MediaCommand::Play)]
        );
    }

    #[test]
    fn a_session_already_in_the_wanted_state_is_left_alone() {
        for (playing, command) in [(false, MediaCommand::Pause), (true, MediaCommand::Play)] {
            let mut controls = spotify(playing);
            assert_eq!(
                perform(&mut controls, &media(command)).expect("no-op"),
                None
            );
            assert_eq!(controls.sent, []);
        }
    }

    #[test]
    fn skipping_tracks_has_nothing_to_restore() {
        for command in [MediaCommand::Next, MediaCommand::Previous] {
            let mut controls = spotify(true);
            assert_eq!(
                perform(&mut controls, &media(command)).expect("skipped"),
                None
            );
            assert_eq!(controls.sent, [("Spotify.exe".to_owned(), command)]);
        }
    }

    #[test]
    fn media_actions_fail_without_a_session_or_when_refused() {
        let mut controls = Fake::default();
        assert!(matches!(
            perform(&mut controls, &media(MediaCommand::Pause)),
            Err(ActionError::NoMediaSession)
        ));
        let mut controls = Fake {
            refuse: true,
            ..spotify(true)
        };
        assert!(matches!(
            perform(&mut controls, &media(MediaCommand::Pause)),
            Err(ActionError::Refused)
        ));
    }

    #[test]
    fn setting_a_volume_restores_the_prior_level() {
        let target = VolumeTarget::App(AppName::new("Spotify.exe").expect("valid"));
        let mut controls = Fake {
            volume: Some(percent(80)),
            ..Fake::default()
        };
        let action = Action::SetVolume {
            target: target.clone(),
            level: percent(20),
        };
        let undo = perform(&mut controls, &action).expect("set");
        assert_eq!(
            undo,
            Some(Restore::Volume {
                target,
                level: percent(80)
            })
        );
        restore(&mut controls, &undo.expect("restorable")).expect("restored");
        assert_eq!(controls.set, [percent(20), percent(80)]);
    }

    #[test]
    fn volume_actions_fail_without_audio() {
        let mut controls = Fake::default();
        let action = Action::SetVolume {
            target: VolumeTarget::System,
            level: percent(20),
        };
        assert!(matches!(
            perform(&mut controls, &action),
            Err(ActionError::NoAudio)
        ));
        let undo = Restore::Volume {
            target: VolumeTarget::System,
            level: percent(20),
        };
        assert!(matches!(
            restore(&mut controls, &undo),
            Err(ActionError::NoAudio)
        ));
    }

    #[test]
    fn os_errors_keep_their_cause() {
        let error = ActionError::from(io::Error::other("device gone"));
        assert!(error.to_string().contains("device gone"));
        assert!(error.source().is_some());
        assert!(ActionError::NoAudio.source().is_none());
    }
}
