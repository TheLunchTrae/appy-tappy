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

/// Sets a volume control to a level with `set`, which answers the
/// platform's error. An interface whose device went away keeps failing even
/// after the device returns, so a failure finds the control again with
/// `find_again` and sets that instead. Answers `false` when the control is
/// gone for good. While the device is still missing, the first error says
/// why, rather than the failure to find it.
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the Windows adapter sets volumes")
)]
pub(crate) fn set_or_find_again<C>(
    control: &C,
    set: impl Fn(&C) -> io::Result<()>,
    find_again: impl FnOnce() -> io::Result<Option<C>>,
) -> io::Result<bool> {
    let Err(error) = set(control) else {
        return Ok(true);
    };
    match find_again() {
        Ok(Some(found)) => set(&found).map(|()| true),
        Ok(None) => Ok(false),
        Err(_missing) => Err(error),
    }
}

/// Names each audio session in `sessions`, given with the process that
/// created it, after that process's executable in `executables`. A session
/// whose process is not listed belongs to a process that ended after the
/// list was made, so nothing can be matched to it and it is left out.
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the Windows adapter lists audio sessions")
)]
pub(crate) fn named_sessions<T>(
    sessions: impl IntoIterator<Item = (ProcessId, T)>,
    executables: &[(ProcessId, String)],
) -> Vec<(String, T)> {
    sessions
        .into_iter()
        .filter_map(|(process, session)| {
            executables
                .iter()
                .find(|(listed, _)| *listed == process)
                .map(|(_, executable)| (executable.clone(), session))
        })
        .collect()
}

/// Opens one program, file, or link at a time. An open runs on a thread of
/// its own, `T`; one that timed out may still be waiting on the shell, and
/// the next open is skipped until it finishes, so a target that never
/// answers leaves one waiting thread rather than one for each try.
#[derive(Debug)]
pub(crate) struct OneOpen<T> {
    /// The open that timed out, if it may still be running.
    waiting: Option<T>,
}

impl<T> Default for OneOpen<T> {
    fn default() -> Self {
        Self { waiting: None }
    }
}

#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the Windows adapter opens targets")
)]
impl<T> OneOpen<T> {
    /// Lets a new open start, given whether an open `finished`. Answers the
    /// open that timed out and has since finished, for the caller to wait
    /// out, or a [`io::ErrorKind::WouldBlock`] error while it still runs.
    ///
    /// # Errors
    ///
    /// Returns [`io::ErrorKind::WouldBlock`] while the open that timed out
    /// is still running.
    pub(crate) fn start(&mut self, finished: impl Fn(&T) -> bool) -> io::Result<Option<T>> {
        match self.waiting.take() {
            Some(last) if !finished(&last) => {
                self.waiting = Some(last);
                Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "the last program, file, or link is still opening, so this one was skipped",
                ))
            },
            last => Ok(last),
        }
    }

    /// Keeps `open`, which timed out and may still be running.
    pub(crate) fn timed_out(&mut self, open: T) {
        self.waiting = Some(open);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_volume_set_finds_the_control_again() {
        let fails = |control: &u8| -> io::Result<()> {
            if *control == 1 {
                Err(io::Error::other("the device went away"))
            } else {
                Ok(())
            }
        };
        assert!(set_or_find_again(&2, fails, || panic!("not needed")).unwrap());
        assert!(set_or_find_again(&1, fails, || Ok(Some(2))).unwrap());
        assert!(
            !set_or_find_again(&1, fails, || Ok(None)).unwrap(),
            "gone for good"
        );
        let error =
            set_or_find_again(&1, fails, || Err(io::Error::other("no device"))).unwrap_err();
        assert_eq!(
            error.to_string(),
            "the device went away",
            "the first error says why"
        );
    }

    #[test]
    fn sessions_are_named_after_their_process_and_ended_ones_left_out() {
        let executables = [
            (ProcessId::new(7), "Player.exe".to_owned()),
            (ProcessId::new(9), "Game.exe".to_owned()),
        ];
        let sessions = [
            (ProcessId::new(9), 'a'),
            (ProcessId::new(8), 'b'),
            (ProcessId::new(7), 'c'),
        ];
        assert_eq!(
            named_sessions(sessions, &executables),
            [("Game.exe".to_owned(), 'a'), ("Player.exe".to_owned(), 'c')]
        );
    }

    #[test]
    fn an_open_is_skipped_while_one_that_timed_out_still_runs() {
        let mut opens = OneOpen::default();
        assert_eq!(
            opens.start(|_: &u8| true).unwrap(),
            None,
            "nothing waits at first"
        );
        opens.timed_out(1);
        let skipped = opens.start(|_| false).unwrap_err();
        assert_eq!(skipped.kind(), io::ErrorKind::WouldBlock);
        assert_eq!(
            opens.start(|_| true).unwrap(),
            Some(1),
            "the finished open is waited out"
        );
        assert_eq!(opens.start(|_| false).unwrap(), None, "and is not kept");
    }

    #[test]
    fn no_controls_can_be_made_where_actions_cannot_run() {
        let made = NoControls::new(Box::new(|| Ok(Vec::new())));
        assert_eq!(
            made.expect_err("unsupported").kind(),
            io::ErrorKind::Unsupported
        );
    }
}
