//! The action thread: runs the actions of each automation that fires, and
//! undoes them when it is released.
//!
//! It owns the [`Actions`] journal, so every change and its undo happen on
//! one thread, in the order the owner sent them. The operating system's
//! media and audio controls are made on this thread too, because Windows
//! ties them to the thread that made them.

use std::io;
use std::sync::mpsc::{Receiver, Sender};

use appytappy_actions::{Actions, Controls, Watched};
use appytappy_config::action::Action;
use appytappy_config::rule::Restore;
use appytappy_engine::RuleIndex;

use crate::reader::SessionId;

/// Names a fired automation: its session and its rule's position, so two
/// sessions' changes are never mixed up.
pub(crate) type Owner = (SessionId, RuleIndex);

/// What the owner asks the action thread to do.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Job {
    /// Run an automation's actions in order.
    Run {
        /// The fired automation.
        owner: Owner,
        /// Its name, for the log.
        automation: String,
        /// The process its session follows: a key press presses keys only
        /// while that process's window is in front.
        watched: Watched,
        /// Its actions.
        actions: Box<[Action]>,
        /// Whether its changes are undone when it is released.
        restore: Restore,
    },
    /// Undo what an automation changed.
    Undo {
        /// The released automation.
        owner: Owner,
        /// Its name, for the log.
        automation: String,
    },
}

/// An action that could not run, for the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ActionFailed(pub(crate) String);

/// Runs `jobs` until the owner stops sending them. `controls` makes the
/// operating system's controls on this thread; when it fails, that is
/// reported once and every job is dropped.
pub(crate) fn run<C, E>(
    controls: impl FnOnce() -> io::Result<C>,
    jobs: &Receiver<Job>,
    failures: &Sender<E>,
) where
    C: Controls,
    E: From<ActionFailed>,
{
    let report = |message: String| failures.send(E::from(ActionFailed(message))).is_ok();
    let mut actions = match controls() {
        Ok(controls) => Actions::<Owner, C>::new(controls),
        Err(error) => {
            if report(format!("actions cannot run: {error}")) {
                // Drain the jobs so the owner never blocks on a full queue.
                for _job in jobs {}
            }
            return;
        },
    };
    for job in jobs {
        let failed = match job {
            Job::Run {
                owner,
                automation,
                watched,
                actions: steps,
                restore,
            } => steps
                .iter()
                .enumerate()
                .filter_map(|(position, step)| {
                    let error = actions.perform(owner, step, restore, &watched).err()?;
                    Some(format!(
                        "{automation}: action {} failed: {error}",
                        position.saturating_add(1)
                    ))
                })
                .collect(),
            Job::Undo { owner, automation } => actions
                .release(owner)
                .err()
                .map(|error| format!("{automation}: undoing failed: {error}"))
                .into_iter()
                .collect::<Vec<_>>(),
        };
        for message in failed {
            if !report(message) {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use appytappy_actions::{MediaSession, Stroke, Volume};
    use appytappy_config::action::{
        MediaCommand, MediaTarget, Modifier, VolumeLevel, VolumeTarget,
    };
    use appytappy_config::anti_cheat::ProcessId;

    use super::*;

    /// One media session that plays until paused, and no volume controls.
    struct OnePlayer {
        playing: bool,
        commands: Sender<MediaCommand>,
    }

    impl Controls for OnePlayer {
        type Media = ();
        type Volume = ();

        fn current_media(&mut self) -> io::Result<Option<MediaSession<()>>> {
            Ok(Some(MediaSession {
                key: (),
                app: "Player.exe".to_owned(),
                playing: self.playing,
            }))
        }

        fn media_sessions(&mut self) -> io::Result<Vec<MediaSession<()>>> {
            Ok(self.current_media()?.into_iter().collect())
        }

        fn media_command(&mut self, (): &(), command: MediaCommand) -> io::Result<bool> {
            self.commands.send(command).map_err(io::Error::other)?;
            self.playing = command == MediaCommand::Play;
            Ok(true)
        }

        fn system_volume(&mut self) -> io::Result<Option<Volume<()>>> {
            Ok(None)
        }

        fn app_volumes(&mut self) -> io::Result<Vec<(String, Volume<()>)>> {
            Ok(Vec::new())
        }

        fn set_volume(&mut self, (): &(), _level: VolumeLevel) -> io::Result<()> {
            Ok(())
        }

        fn focused_process(&mut self) -> io::Result<Option<ProcessId>> {
            Ok(None)
        }

        fn held_modifiers(&mut self) -> io::Result<Vec<Modifier>> {
            Ok(Vec::new())
        }

        fn send_strokes(&mut self, strokes: &[Stroke]) -> io::Result<usize> {
            Ok(strokes.len())
        }

        fn hold(&mut self, _time: Duration) {}
    }

    const OWNER: Owner = (SessionId(1), RuleIndex(0));

    fn pause_then_mute() -> Box<[Action]> {
        Box::new([
            Action::Media {
                target: MediaTarget::Current,
                command: MediaCommand::Pause,
            },
            Action::SetVolume {
                target: VolumeTarget::System,
                level: VolumeLevel::from_percent(0).expect("in range"),
            },
        ])
    }

    #[test]
    fn jobs_run_in_order_and_failures_name_the_automation_and_action() {
        let (jobs, received) = mpsc::channel();
        let (failures, failed) = mpsc::channel::<ActionFailed>();
        jobs.send(Job::Run {
            owner: OWNER,
            automation: "Pause near the end".to_owned(),
            watched: Watched {
                process: ProcessId::new(7),
                executable: "Example.exe".to_owned(),
            },
            actions: pause_then_mute(),
            restore: Restore::OnRelease,
        })
        .expect("send");
        jobs.send(Job::Undo {
            owner: OWNER,
            automation: "Pause near the end".to_owned(),
        })
        .expect("send");
        drop(jobs);
        let (commands, sent) = mpsc::channel();
        let player = OnePlayer {
            playing: true,
            commands,
        };
        run(|| Ok(player), &received, &failures);
        assert_eq!(
            sent.try_iter().collect::<Vec<_>>(),
            [MediaCommand::Pause, MediaCommand::Play],
            "the undo plays what the run paused"
        );
        assert_eq!(
            failed.try_iter().collect::<Vec<_>>(),
            [ActionFailed(
                "Pause near the end: action 2 failed: the volume target has no audio session"
                    .to_owned()
            )]
        );
    }

    #[test]
    fn controls_that_cannot_start_are_reported_once() {
        let (jobs, received) = mpsc::channel();
        let (failures, failed) = mpsc::channel::<ActionFailed>();
        for _ in 0..2 {
            jobs.send(Job::Undo {
                owner: OWNER,
                automation: "Pause".to_owned(),
            })
            .expect("send");
        }
        drop(jobs);
        run::<OnePlayer, _>(|| Err(io::Error::other("no audio")), &received, &failures);
        assert_eq!(
            failed.try_iter().collect::<Vec<_>>(),
            [ActionFailed("actions cannot run: no audio".to_owned())]
        );
    }
}
