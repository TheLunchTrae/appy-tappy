//! Windows actions the runtime performs when a rule fires.
//!
//! [`Actions`] runs each action through [`Controls`] and keeps a journal of
//! what the restoring rules changed, so releasing a rule puts back the right
//! state even when several rules changed the same thing. [`Controls`] only
//! lists things and acts on one of them; every choice is made here, where it
//! is tested with fake controls on every platform. The Windows adapter in
//! `win` implements [`Controls`]. `docs/actions.md` explains the v1 set.
//!
//! See `docs/architecture.md` for this crate's boundaries.

use std::error::Error;
use std::fmt;
use std::io;

use appytappy_config::action::{Action, MediaCommand, MediaTarget, VolumeLevel, VolumeTarget};
use appytappy_config::rule::Restore;

#[cfg(windows)]
pub mod win;

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

    /// Sets `control` to `level`.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error, including for a control that is
    /// gone.
    fn set_volume(&mut self, control: &Self::Volume, level: VolumeLevel) -> io::Result<()>;
}

/// What one restoring action changed: the target, its state before, and the
/// state the action left it in.
#[derive(Debug)]
enum Change<M, V> {
    /// Whether a media session plays.
    Media {
        session: M,
        before: bool,
        after: bool,
    },
    /// A volume control's level.
    Volume {
        control: V,
        before: VolumeLevel,
        after: VolumeLevel,
    },
}

impl<M: PartialEq, V: PartialEq> Change<M, V> {
    fn same_target(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Media { session, .. }, Self::Media { session: other, .. }) => session == other,
            (Self::Volume { control, .. }, Self::Volume { control: other, .. }) => control == other,
            (Self::Media { .. }, Self::Volume { .. })
            | (Self::Volume { .. }, Self::Media { .. }) => false,
        }
    }

    /// Takes over the state `earlier` would have put back, because this
    /// change came after it on the same target.
    fn inherit(&mut self, earlier: Self) {
        match (self, earlier) {
            (
                Self::Media { before, .. },
                Self::Media {
                    before: original, ..
                },
            ) => *before = original,
            (
                Self::Volume { before, .. },
                Self::Volume {
                    before: original, ..
                },
            ) => {
                *before = original;
            },
            (Self::Media { .. }, Self::Volume { .. })
            | (Self::Volume { .. }, Self::Media { .. }) => {},
        }
    }
}

/// One entry of the restore journal.
#[derive(Debug)]
struct Entry<O, M, V> {
    owner: O,
    change: Change<M, V>,
}

/// Performs actions and restores what restoring rules changed.
///
/// The journal records, for each action of a restoring rule, the target it
/// touched, that target's state just before, and the state it set.
/// Releasing a rule undoes its entries newest first. When a later entry by
/// another rule touched the same target, the release leaves the target alone
/// and hands its own "before" state to that entry, so the target goes back
/// to its original state only once the last rule that changed it is
/// released. An entry whose action changed nothing, such as pausing a
/// session that was already paused, still holds its target, but releasing
/// it sends nothing unless it took over another rule's "before" state.
///
/// `O` names a rule: the runtime passes the watched app and the rule's
/// position.
#[derive(Debug)]
pub struct Actions<O, C: Controls> {
    controls: C,
    journal: Vec<Entry<O, C::Media, C::Volume>>,
}

impl<O: Copy + PartialEq, C: Controls> Actions<O, C> {
    /// Starts with an empty journal.
    #[must_use]
    pub const fn new(controls: C) -> Self {
        Self {
            controls,
            journal: Vec::new(),
        }
    }

    /// The controls, for reading state.
    pub const fn controls(&mut self) -> &mut C {
        &mut self.controls
    }

    /// Runs `action` for `owner`.
    ///
    /// With [`Restore::OnRelease`], the journal records what it changed so
    /// [`Actions::release`] can undo it. With [`Restore::Never`], the change
    /// is permanent: the journal forgets the targets it changed, so no
    /// earlier rule's release undoes it.
    ///
    /// A media action on an app picks its sessions here: pause pauses every
    /// playing session of the app, or holds them all when none plays; play
    /// starts the first session unless one is already playing; and a skip
    /// goes to the first playing session, or else the first session. A volume action on an app sets every audio
    /// session of its executable.
    ///
    /// # Errors
    ///
    /// Returns [`ActionError`] when the target is missing, refuses, or the
    /// operating system fails. Changes made before the failure stay in the
    /// journal, so a release still undoes them.
    pub fn perform(
        &mut self,
        owner: O,
        action: &Action,
        restore: Restore,
    ) -> Result<(), ActionError> {
        let restores = restore == Restore::OnRelease;
        match action {
            Action::Media { target, command } => {
                for session in self.pick_sessions(target, *command)? {
                    let after = match command {
                        MediaCommand::Pause => false,
                        MediaCommand::Play => true,
                        MediaCommand::Next | MediaCommand::Previous => {
                            send(&mut self.controls, &session.key, *command)?;
                            continue;
                        },
                    };
                    if after != session.playing {
                        send(&mut self.controls, &session.key, *command)?;
                    }
                    let change = Change::Media {
                        session: session.key,
                        before: session.playing,
                        after,
                    };
                    self.record(owner, change, restores);
                }
            },
            Action::SetVolume { target, level } => {
                for volume in self.pick_volumes(target)? {
                    self.controls.set_volume(&volume.key, *level)?;
                    let change = Change::Volume {
                        control: volume.key,
                        before: volume.level,
                        after: *level,
                    };
                    self.record(owner, change, restores);
                }
            },
        }
        Ok(())
    }

    /// Undoes what `owner` changed, newest first, and forgets it.
    ///
    /// # Errors
    ///
    /// Returns the first [`ActionError`] after trying every entry, because one
    /// failed restore should not keep the others from running.
    pub fn release(&mut self, owner: O) -> Result<(), ActionError> {
        let mut failure = None;
        while let Some(position) = self.journal.iter().rposition(|entry| entry.owner == owner) {
            let entry = self.journal.remove(position);
            let later = self.journal.get_mut(position..).and_then(|rest| {
                rest.iter_mut()
                    .find(|later| later.change.same_target(&entry.change))
            });
            if let Some(later) = later {
                later.change.inherit(entry.change);
            } else if let Err(error) = self.undo(entry.change) {
                failure.get_or_insert(error);
            }
        }
        failure.map_or(Ok(()), Err)
    }

    fn record(&mut self, owner: O, change: Change<C::Media, C::Volume>, restores: bool) {
        if restores {
            self.journal.push(Entry { owner, change });
        } else {
            self.journal
                .retain(|entry| !entry.change.same_target(&change));
        }
    }

    fn undo(&mut self, change: Change<C::Media, C::Volume>) -> Result<(), ActionError> {
        match change {
            Change::Media {
                session,
                before,
                after,
            } if before != after => {
                let command = if before {
                    MediaCommand::Play
                } else {
                    MediaCommand::Pause
                };
                send(&mut self.controls, &session, command)
            },
            Change::Volume {
                control,
                before,
                after,
            } if before != after => Ok(self.controls.set_volume(&control, before)?),
            Change::Media { .. } | Change::Volume { .. } => Ok(()),
        }
    }

    fn pick_sessions(
        &mut self,
        target: &MediaTarget,
        command: MediaCommand,
    ) -> Result<Vec<MediaSession<C::Media>>, ActionError> {
        let mut sessions = match target {
            MediaTarget::Current => self.controls.current_media()?.into_iter().collect(),
            MediaTarget::App(app) => {
                let mut sessions = self.controls.media_sessions()?;
                sessions.retain(|session| app.matches(&session.app));
                sessions
            },
        };
        let first_playing = sessions.iter().position(|session| session.playing);
        match (command, first_playing) {
            (MediaCommand::Pause | MediaCommand::Play, Some(_)) => {
                sessions.retain(|session| session.playing);
            },
            // Nothing plays, so a pause changes nothing, but it holds every
            // session so that another rule's release cannot resume one.
            (MediaCommand::Pause, None) => {},
            (MediaCommand::Next | MediaCommand::Previous, Some(first)) => {
                sessions.drain(..first);
                sessions.truncate(1);
            },
            (MediaCommand::Play | MediaCommand::Next | MediaCommand::Previous, None) => {
                sessions.truncate(1);
            },
        }
        if sessions.is_empty() {
            return Err(ActionError::NoMediaSession);
        }
        Ok(sessions)
    }

    fn pick_volumes(
        &mut self,
        target: &VolumeTarget,
    ) -> Result<Vec<Volume<C::Volume>>, ActionError> {
        let volumes: Vec<_> = match target {
            VolumeTarget::System => self.controls.system_volume()?.into_iter().collect(),
            VolumeTarget::App(app) => self
                .controls
                .app_volumes()?
                .into_iter()
                .filter_map(|(executable, volume)| app.matches(&executable).then_some(volume))
                .collect(),
        };
        if volumes.is_empty() {
            return Err(ActionError::NoAudio);
        }
        Ok(volumes)
    }
}

fn send<C: Controls>(
    controls: &mut C,
    session: &C::Media,
    command: MediaCommand,
) -> Result<(), ActionError> {
    if controls.media_command(session, command)? {
        Ok(())
    } else {
        Err(ActionError::Refused)
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

    /// Controls over a few media sessions and volume controls that apply every
    /// command, and log each one.
    #[derive(Default)]
    struct Fake {
        sessions: Vec<MediaSession<u32>>,
        current: Option<u32>,
        refuse: bool,
        system: Option<Volume<&'static str>>,
        apps: Vec<(String, Volume<&'static str>)>,
        broken: Option<&'static str>,
        log: Vec<String>,
    }

    impl Controls for Fake {
        type Media = u32;
        type Volume = &'static str;

        fn current_media(&mut self) -> io::Result<Option<MediaSession<u32>>> {
            let current = self.current;
            Ok(self
                .sessions
                .iter()
                .find(|session| Some(session.key) == current)
                .cloned())
        }

        fn media_sessions(&mut self) -> io::Result<Vec<MediaSession<u32>>> {
            Ok(self.sessions.clone())
        }

        fn media_command(&mut self, session: &u32, command: MediaCommand) -> io::Result<bool> {
            self.log.push(format!("{command:?} {session}"));
            if self.refuse {
                return Ok(false);
            }
            let found = self
                .sessions
                .iter_mut()
                .find(|known| known.key == *session)
                .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
            match command {
                MediaCommand::Play => found.playing = true,
                MediaCommand::Pause => found.playing = false,
                MediaCommand::Next | MediaCommand::Previous => {},
            }
            Ok(true)
        }

        fn system_volume(&mut self) -> io::Result<Option<Volume<&'static str>>> {
            Ok(self.system.clone())
        }

        fn app_volumes(&mut self) -> io::Result<Vec<(String, Volume<&'static str>)>> {
            Ok(self.apps.clone())
        }

        fn set_volume(&mut self, control: &&'static str, level: VolumeLevel) -> io::Result<()> {
            if self.broken == Some(*control) {
                return Err(io::Error::other("device gone"));
            }
            self.log.push(format!("{control} {}", level.scalar()));
            let found = self
                .system
                .iter_mut()
                .chain(self.apps.iter_mut().map(|(_, volume)| volume))
                .find(|volume| volume.key == *control)
                .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
            found.level = level;
            Ok(())
        }
    }

    impl Fake {
        fn playing(&self, key: u32) -> bool {
            self.sessions
                .iter()
                .any(|session| session.key == key && session.playing)
        }

        fn level(&self, control: &str) -> Option<VolumeLevel> {
            self.system
                .iter()
                .chain(self.apps.iter().map(|(_, volume)| volume))
                .find(|volume| volume.key == control)
                .map(|volume| volume.level)
        }
    }

    fn session(key: u32, app: &str, playing: bool) -> MediaSession<u32> {
        MediaSession {
            key,
            app: app.to_owned(),
            playing,
        }
    }

    /// A media player as session 1, current, and playing or not.
    fn player(playing: bool) -> Actions<char, Fake> {
        Actions::new(Fake {
            sessions: vec![session(1, "Player.exe", playing)],
            current: Some(1),
            ..Fake::default()
        })
    }

    fn percent(level: u8) -> VolumeLevel {
        VolumeLevel::from_percent(level).expect("at most 100")
    }

    fn app(name: &str) -> AppName {
        AppName::new(name).expect("valid")
    }

    fn media(target: MediaTarget, command: MediaCommand) -> Action {
        Action::Media { target, command }
    }

    fn current(command: MediaCommand) -> Action {
        media(MediaTarget::Current, command)
    }

    fn volume(target: VolumeTarget, level: u8) -> Action {
        Action::SetVolume {
            target,
            level: percent(level),
        }
    }

    /// Speakers at 80% as the system volume.
    fn speakers() -> Actions<char, Fake> {
        Actions::new(Fake {
            system: Some(Volume {
                key: "speakers",
                level: percent(80),
            }),
            ..Fake::default()
        })
    }

    #[test]
    fn a_pause_is_undone_by_playing_and_a_play_by_pausing() {
        let mut actions = player(true);
        let pause = current(MediaCommand::Pause);
        actions
            .perform('a', &pause, Restore::OnRelease)
            .expect("paused");
        assert!(!actions.controls().playing(1));
        actions.release('a').expect("resumed");
        assert!(actions.controls().playing(1));

        let mut actions = player(false);
        let play = current(MediaCommand::Play);
        actions
            .perform('a', &play, Restore::OnRelease)
            .expect("played");
        assert!(actions.controls().playing(1));
        actions.release('a').expect("paused");
        assert!(!actions.controls().playing(1));
        assert_eq!(actions.controls().log, ["Play 1", "Pause 1"]);
    }

    #[test]
    fn a_session_already_in_the_wanted_state_is_left_alone() {
        let mut actions = player(false);
        let pause = current(MediaCommand::Pause);
        actions
            .perform('a', &pause, Restore::OnRelease)
            .expect("nothing to do");
        actions.release('a').expect("nothing to undo");
        assert_eq!(actions.controls().log, [] as [String; 0]);
    }

    #[test]
    fn overlapping_pauses_resume_only_after_the_last_release() {
        let pause = current(MediaCommand::Pause);
        for order in [['a', 'b'], ['b', 'a']] {
            let mut actions = player(true);
            actions
                .perform('a', &pause, Restore::OnRelease)
                .expect("paused");
            actions
                .perform('b', &pause, Restore::OnRelease)
                .expect("already paused");
            actions.release(order[0]).expect("first release");
            assert!(!actions.controls().playing(1), "{order:?}");
            actions.release(order[1]).expect("last release");
            assert!(actions.controls().playing(1), "{order:?}");
        }
    }

    #[test]
    fn overlapping_volumes_unwind_to_the_original_level() {
        for order in [['a', 'b'], ['b', 'a']] {
            let mut actions = speakers();
            let system = || volume(VolumeTarget::System, 0);
            actions
                .perform('a', &volume(VolumeTarget::System, 20), Restore::OnRelease)
                .expect("set");
            actions
                .perform('b', &system(), Restore::OnRelease)
                .expect("set");
            actions.release(order[0]).expect("first release");
            let held = if order[0] == 'a' { 0 } else { 20 };
            assert_eq!(
                actions.controls().level("speakers"),
                Some(percent(held)),
                "{order:?}"
            );
            actions.release(order[1]).expect("last release");
            assert_eq!(actions.controls().level("speakers"), Some(percent(80)));
        }
    }

    #[test]
    fn rules_on_different_targets_restore_independently() {
        let mut actions = Actions::new(Fake {
            sessions: vec![session(1, "Player.exe", true)],
            current: Some(1),
            system: Some(Volume {
                key: "speakers",
                level: percent(80),
            }),
            apps: vec![(
                "Player.exe".to_owned(),
                Volume {
                    key: "music",
                    level: percent(70),
                },
            )],
            ..Fake::default()
        });
        actions
            .perform('a', &volume(VolumeTarget::System, 20), Restore::OnRelease)
            .expect("set");
        actions
            .perform('a', &current(MediaCommand::Pause), Restore::OnRelease)
            .expect("paused");
        let player = VolumeTarget::App(app("Player.exe"));
        actions
            .perform('b', &volume(player, 10), Restore::OnRelease)
            .expect("set");
        actions.release('a').expect("released");
        assert_eq!(actions.controls().level("speakers"), Some(percent(80)));
        assert!(actions.controls().playing(1));
        assert_eq!(actions.controls().level("music"), Some(percent(10)));
    }

    #[test]
    fn a_volume_already_at_its_level_is_not_set_again_on_release() {
        let mut actions = speakers();
        actions
            .perform('a', &volume(VolumeTarget::System, 80), Restore::OnRelease)
            .expect("set");
        actions.release('a').expect("nothing to undo");
        assert_eq!(actions.controls().log, ["speakers 0.8"]);
    }

    #[test]
    fn one_rule_undoes_its_actions_newest_first() {
        let mut actions = speakers();
        for level in [20, 10] {
            actions
                .perform(
                    'a',
                    &volume(VolumeTarget::System, level),
                    Restore::OnRelease,
                )
                .expect("set");
        }
        actions.release('a').expect("released");
        assert_eq!(actions.controls().level("speakers"), Some(percent(80)));
        assert_eq!(
            actions.controls().log,
            [
                "speakers 0.2",
                "speakers 0.1",
                "speakers 0.2",
                "speakers 0.8"
            ]
        );
    }

    #[test]
    fn a_permanent_change_is_not_undone_by_an_earlier_rule() {
        let mut actions = speakers();
        actions
            .perform('a', &volume(VolumeTarget::System, 20), Restore::OnRelease)
            .expect("set");
        actions
            .perform('n', &volume(VolumeTarget::System, 50), Restore::Never)
            .expect("set");
        actions.release('a').expect("nothing to undo");
        assert_eq!(actions.controls().level("speakers"), Some(percent(50)));
    }

    #[test]
    fn an_app_target_picks_its_sessions_among_others() {
        let tabs = || Fake {
            sessions: vec![
                session(1, "Player.exe", true),
                session(2, "Chrome", false),
                session(3, "Chrome", true),
                session(4, "Chrome", true),
            ],
            current: Some(1),
            ..Fake::default()
        };
        let chrome = || MediaTarget::App(app("chrome"));
        let mut actions = Actions::new(tabs());
        actions
            .perform(
                'a',
                &media(chrome(), MediaCommand::Pause),
                Restore::OnRelease,
            )
            .expect("paused");
        assert_eq!(actions.controls().log, ["Pause 3", "Pause 4"]);
        assert!(actions.controls().playing(1));
        actions.release('a').expect("resumed");
        assert_eq!(
            actions.controls().log,
            ["Pause 3", "Pause 4", "Play 4", "Play 3"]
        );
        assert!(!actions.controls().playing(2));

        // Play leaves the app alone while a session plays, and starts the first otherwise.
        let mut actions = Actions::new(tabs());
        actions
            .perform('a', &media(chrome(), MediaCommand::Play), Restore::Never)
            .expect("already playing");
        assert_eq!(actions.controls().log, [] as [String; 0]);
        actions
            .controls()
            .sessions
            .retain(|session| !session.playing);
        actions
            .perform('a', &media(chrome(), MediaCommand::Play), Restore::Never)
            .expect("played");
        assert_eq!(actions.controls().log, ["Play 2"]);
    }

    #[test]
    fn a_pause_that_changes_nothing_still_holds_every_session_of_the_app() {
        let mut actions = Actions::new(Fake {
            sessions: vec![session(1, "Chrome", false), session(2, "Chrome", true)],
            ..Fake::default()
        });
        let pause = media(MediaTarget::App(app("Chrome")), MediaCommand::Pause);
        actions
            .perform('a', &pause, Restore::OnRelease)
            .expect("paused");
        actions
            .perform('b', &pause, Restore::OnRelease)
            .expect("already paused");
        actions.release('a').expect("released");
        assert!(!actions.controls().playing(2));
        actions.release('b').expect("released");
        assert!(actions.controls().playing(2));
        assert!(!actions.controls().playing(1));
        assert_eq!(actions.controls().log, ["Pause 2", "Play 2"]);
    }

    #[test]
    fn skipping_goes_to_the_first_playing_session_and_is_never_undone() {
        let mut actions = Actions::new(Fake {
            sessions: vec![session(1, "Chrome", false), session(2, "Chrome", true)],
            ..Fake::default()
        });
        let next = media(MediaTarget::App(app("Chrome")), MediaCommand::Next);
        actions
            .perform('a', &next, Restore::OnRelease)
            .expect("skipped");
        actions
            .controls()
            .sessions
            .iter_mut()
            .for_each(|session| session.playing = false);
        let previous = media(MediaTarget::App(app("Chrome")), MediaCommand::Previous);
        actions
            .perform('a', &previous, Restore::OnRelease)
            .expect("skipped");
        actions.release('a').expect("nothing to undo");
        assert_eq!(actions.controls().log, ["Next 2", "Previous 1"]);
    }

    #[test]
    fn media_actions_fail_without_a_session_or_when_refused() {
        let mut actions = Actions::<char, _>::new(Fake::default());
        for target in [MediaTarget::Current, MediaTarget::App(app("Player.exe"))] {
            assert!(matches!(
                actions.perform('a', &media(target, MediaCommand::Pause), Restore::Never),
                Err(ActionError::NoMediaSession)
            ));
        }
        let mut actions = player(true);
        actions.controls().refuse = true;
        assert!(matches!(
            actions.perform('a', &current(MediaCommand::Pause), Restore::OnRelease),
            Err(ActionError::Refused)
        ));
    }

    #[test]
    fn an_app_volume_restores_each_session_to_its_own_level() {
        let mut actions = Actions::new(Fake {
            apps: vec![
                (
                    "Discord.exe".to_owned(),
                    Volume {
                        key: "call",
                        level: percent(100),
                    },
                ),
                (
                    "Player.exe".to_owned(),
                    Volume {
                        key: "music",
                        level: percent(70),
                    },
                ),
                (
                    "discord.EXE".to_owned(),
                    Volume {
                        key: "ping",
                        level: percent(40),
                    },
                ),
            ],
            ..Fake::default()
        });
        let discord = || VolumeTarget::App(app("Discord.exe"));
        actions
            .perform('a', &volume(discord(), 20), Restore::OnRelease)
            .expect("set");
        assert_eq!(actions.controls().level("call"), Some(percent(20)));
        assert_eq!(actions.controls().level("ping"), Some(percent(20)));
        assert_eq!(actions.controls().level("music"), Some(percent(70)));
        actions.release('a').expect("restored");
        assert_eq!(actions.controls().level("call"), Some(percent(100)));
        assert_eq!(actions.controls().level("ping"), Some(percent(40)));
    }

    #[test]
    fn changes_made_before_a_failure_are_still_restored() {
        let mut actions = Actions::new(Fake {
            apps: vec![
                (
                    "Discord.exe".to_owned(),
                    Volume {
                        key: "call",
                        level: percent(100),
                    },
                ),
                (
                    "Discord.exe".to_owned(),
                    Volume {
                        key: "ping",
                        level: percent(40),
                    },
                ),
            ],
            broken: Some("ping"),
            ..Fake::default()
        });
        let discord = VolumeTarget::App(app("Discord.exe"));
        assert!(matches!(
            actions.perform('a', &volume(discord, 20), Restore::OnRelease),
            Err(ActionError::Os(_))
        ));
        actions.release('a').expect("restored");
        assert_eq!(actions.controls().level("call"), Some(percent(100)));
    }

    #[test]
    fn a_failed_restore_does_not_stop_the_others() {
        let mut actions = Actions::new(Fake {
            system: Some(Volume {
                key: "speakers",
                level: percent(80),
            }),
            apps: vec![(
                "Player.exe".to_owned(),
                Volume {
                    key: "music",
                    level: percent(70),
                },
            )],
            ..Fake::default()
        });
        actions
            .perform('a', &volume(VolumeTarget::System, 20), Restore::OnRelease)
            .expect("set");
        let player = VolumeTarget::App(app("Player.exe"));
        actions
            .perform('a', &volume(player, 10), Restore::OnRelease)
            .expect("set");
        actions.controls().broken = Some("music");
        assert!(matches!(actions.release('a'), Err(ActionError::Os(_))));
        assert_eq!(actions.controls().level("speakers"), Some(percent(80)));
        // A failed entry is forgotten, so releasing again does nothing.
        actions.controls().broken = None;
        actions.release('a').expect("nothing left");
        assert_eq!(actions.controls().level("music"), Some(percent(10)));
    }

    #[test]
    fn volume_actions_fail_without_audio() {
        let mut actions = Actions::<char, _>::new(Fake::default());
        for target in [VolumeTarget::System, VolumeTarget::App(app("Player.exe"))] {
            assert!(matches!(
                actions.perform('a', &volume(target, 20), Restore::OnRelease),
                Err(ActionError::NoAudio)
            ));
        }
    }

    #[test]
    fn errors_say_what_failed_and_keep_their_cause() {
        let error = ActionError::from(io::Error::other("device gone"));
        assert!(error.to_string().contains("device gone"));
        assert!(error.source().is_some());
        for (error, words) in [
            (ActionError::NoMediaSession, "no media session"),
            (ActionError::Refused, "did not take"),
            (ActionError::NoAudio, "no audio session"),
        ] {
            assert!(error.to_string().contains(words), "{error}");
            assert!(error.source().is_none());
        }
    }
}
