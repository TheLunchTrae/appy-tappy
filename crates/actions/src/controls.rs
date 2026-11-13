//! What actions act on: the media sessions and volume controls that
//! [`Controls`] lists, the calls that act on them, and the [`Input`] that
//! key presses and clicks go through.

use std::convert::Infallible;
use std::io;
use std::path::Path;

use appytappy_config::action::{MediaCommand, OpenTarget, VolumeLevel};
use appytappy_config::process::ProcessId;

use crate::Input;

/// A media session as Windows reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaSession<K> {
    /// The session itself, for sending it commands.
    pub key: K,
    /// The session's app ID, such as `Player.exe`.
    pub app: String,
    /// Whether it is playing.
    pub playing: bool,
}

/// One volume control and its current level.
#[derive(Debug, Clone, PartialEq)]
pub struct Volume<K> {
    /// The control itself, for setting its level.
    pub key: K,
    /// Its level now.
    pub level: VolumeLevel,
}

/// The operating system controls that actions use. Each method lists what
/// exists or acts on one thing it listed, and makes no choices.
pub trait Controls {
    /// A media session. Two keys are equal when they name the same session.
    type Media: PartialEq;
    /// A volume control: a device's or one audio session's. Two keys are
    /// equal when they name the same control.
    type Volume: PartialEq;
    /// The keyboard, the mouse, and the windows.
    type Input: Input;

    /// The input that key presses and clicks go through.
    fn input(&mut self) -> &mut Self::Input;

    /// The session Windows considers current, if any.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn current_media(&mut self) -> io::Result<Option<MediaSession<Self::Media>>>;

    /// Every media session.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn media_sessions(&mut self) -> io::Result<Vec<MediaSession<Self::Media>>>;

    /// Sends `command` to `session`. Returns whether the session took it.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error, including for a session that is
    /// gone.
    fn media_command(&mut self, session: &Self::Media, command: MediaCommand) -> io::Result<bool>;

    /// The default playback device's volume, or `None` with no device.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn system_volume(&mut self) -> io::Result<Option<Volume<Self::Volume>>>;

    /// Every audio session on the default playback device, each with the
    /// executable name of the process that owns it, such as `Player.exe`.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn app_volumes(&mut self) -> io::Result<Vec<(String, Volume<Self::Volume>)>>;

    /// Sets `control` to `level`. Returns `false` when the control is gone
    /// for good, such as the audio session of an app that closed.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error, including for a device that is
    /// unplugged, which may come back.
    fn set_volume(&mut self, control: &Self::Volume, level: VolumeLevel) -> io::Result<bool>;

    /// Opens `target` the way opening it from File Explorer would: a program
    /// runs, a file opens in the program its type is set to, and a link
    /// opens in the default browser. It passes nothing to what it opens and
    /// shows no error dialog.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error, such as for a file that is not
    /// there, or a timeout when opening takes too long to answer.
    fn open(&mut self, target: &OpenTarget) -> io::Result<()>;

    /// Starts playing the WAV file at `file` and returns without waiting for
    /// it to end. A sound that is still playing stops. Returns whether it
    /// started: it does not for a file that is missing or is not a WAV
    /// file, or with no playback device.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error.
    fn play_sound(&mut self, file: &Path) -> io::Result<bool>;
}

/// Lists the running processes' executable names with their process IDs.
/// It must not open any process: the controls rely on that so that no
/// action opens a watched app's process.
pub type Executables = Box<dyn Fn() -> io::Result<Vec<(ProcessId, String)>>>;

/// The controls of a platform AppyTappy does not support: none can be made,
/// so no action runs there.
#[derive(Debug)]
pub struct NoControls(Infallible);

impl NoControls {
    /// Fails, as there are no controls to connect to.
    ///
    /// # Errors
    ///
    /// Always returns an [`io::ErrorKind::Unsupported`] error.
    pub fn new(_executables: Executables) -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "actions run only on Windows",
        ))
    }
}

// `NoControls` holds `Infallible`, so no value exists and none of these
// bodies ever runs. No test can call them, so mutating them cannot be
// caught.
impl Controls for NoControls {
    type Media = ();
    type Volume = ();
    type Input = crate::Unsupported;

    #[mutants::skip]
    fn input(&mut self) -> &mut crate::Unsupported {
        match self.0 {}
    }

    #[mutants::skip]
    fn current_media(&mut self) -> io::Result<Option<MediaSession<()>>> {
        match self.0 {}
    }

    #[mutants::skip]
    fn media_sessions(&mut self) -> io::Result<Vec<MediaSession<()>>> {
        match self.0 {}
    }

    #[mutants::skip]
    fn media_command(&mut self, (): &(), _command: MediaCommand) -> io::Result<bool> {
        match self.0 {}
    }

    #[mutants::skip]
    fn system_volume(&mut self) -> io::Result<Option<Volume<()>>> {
        match self.0 {}
    }

    #[mutants::skip]
    fn app_volumes(&mut self) -> io::Result<Vec<(String, Volume<()>)>> {
        match self.0 {}
    }

    #[mutants::skip]
    fn set_volume(&mut self, (): &(), _level: VolumeLevel) -> io::Result<bool> {
        match self.0 {}
    }

    #[mutants::skip]
    fn open(&mut self, _target: &OpenTarget) -> io::Result<()> {
        match self.0 {}
    }

    #[mutants::skip]
    fn play_sound(&mut self, _file: &Path) -> io::Result<bool> {
        match self.0 {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_controls_can_be_made_where_actions_cannot_run() {
        let made = NoControls::new(Box::new(|| Ok(Vec::new())));
        assert_eq!(
            made.expect_err("unsupported").kind(),
            io::ErrorKind::Unsupported
        );
    }
}
