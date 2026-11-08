//! The action thread: runs the actions of each automation that fires, and
//! undoes them when it is released.
//!
//! It owns the [`Actions`] journal, so every change and its undo happen on
//! one thread, in the order the owner sent them. The operating system's
//! media and audio controls are made on this thread too, because Windows
//! ties them to the thread that made them.
//!
//! When the controls cannot be made, such as while Windows is still
//! starting up after sign-in, the thread makes them again when a later
//! automation fires, waiting longer after each failure. A restore that
//! fails is tried again: a media restore once, after a short wait, and a
//! volume restore on each later job and every few seconds until it works.

use std::io;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use appytappy_actions::{Actions, Controls, Watched};
use appytappy_config::action::Action;
use appytappy_config::rule::Restore;
use appytappy_engine::RuleIndex;

use crate::workers::reader::SessionId;

/// How long the thread waits before it makes the controls again after
/// they first fail; each failure doubles it up to [`CONTROLS_WAIT_MAX`].
const CONTROLS_WAIT: Duration = Duration::from_secs(1);

/// The longest wait between two tries to make the controls.
const CONTROLS_WAIT_MAX: Duration = Duration::from_secs(60);

/// How long a failed media restore waits before its one more try.
const MEDIA_RETRY: Duration = Duration::from_secs(2);

/// How often a failed volume restore is tried again while no job comes.
const VOLUME_RETRY: Duration = Duration::from_secs(10);

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

/// A note from the action thread for the log: an action that could not
/// run, or that actions can run again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ActionFailed(pub(crate) String);

/// Runs `jobs` until the owner stops sending them. `controls` makes the
/// operating system's controls on this thread; when it fails, the jobs that
/// come meanwhile are reported as not run, and it is called again when a
/// job comes after a wait that grows with each failure.
pub(crate) fn run<C, E>(
    controls: impl FnMut() -> io::Result<C>,
    jobs: &Receiver<Job>,
    failures: &Sender<E>,
) where
    C: Controls,
    E: From<ActionFailed>,
{
    let report = |notes: Vec<String>| {
        notes
            .into_iter()
            .all(|note| failures.send(E::from(ActionFailed(note))).is_ok())
    };
    let (mut worker, notes) = Worker::start(controls, Instant::now());
    if !report(notes) {
        return;
    }
    loop {
        let job = match worker.retry_in() {
            Some(wait) => match jobs.recv_timeout(wait) {
                Ok(job) => Some(job),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            },
            None => match jobs.recv() {
                Ok(job) => Some(job),
                Err(_) => return,
            },
        };
        let notes = match job {
            Some(job) => worker.job(job, Instant::now()),
            None => worker.retry(),
        };
        if !report(notes) {
            return;
        }
    }
}

/// The action thread's state: the journal once the controls are made, and
/// what it needs to make them again until then.
struct Worker<C: Controls, F> {
    /// Makes the controls.
    make: F,
    /// The journal, or why the controls are missing.
    actions: Result<Actions<Owner, C>, Missing>,
}

/// Why the controls are missing, and when to make them again.
struct Missing {
    /// The last error making them.
    error: String,
    /// The earliest time to make them again.
    retry_at: Instant,
    /// How long the next failure waits.
    wait: Duration,
}

impl<C, F> Worker<C, F>
where
    C: Controls,
    F: FnMut() -> io::Result<C>,
{
    /// Makes the controls, and says so in the log when they cannot be made.
    fn start(mut make: F, now: Instant) -> (Self, Vec<String>) {
        let (actions, notes) = match make() {
            Ok(controls) => (Ok(Actions::new(controls)), Vec::new()),
            Err(error) => (
                Err(Missing {
                    error: error.to_string(),
                    retry_at: now.checked_add(CONTROLS_WAIT).unwrap_or(now),
                    wait: CONTROLS_WAIT.saturating_mul(2).min(CONTROLS_WAIT_MAX),
                }),
                vec![format!(
                    "actions cannot run yet, so they are tried again when an automation fires: {error}"
                )],
            ),
        };
        (Self { make, actions }, notes)
    }

    /// Runs `job`, after making the controls again if they are missing and
    /// their wait is over, and after trying the pending restores again.
    fn job(&mut self, job: Job, now: Instant) -> Vec<String> {
        let mut notes = self.make_again(now);
        let actions = match &mut self.actions {
            Ok(actions) => actions,
            Err(missing) => {
                // A job that comes while the controls are missing changes
                // nothing, so an undo has nothing to put back.
                if let Job::Run { automation, .. } = job {
                    notes.push(format!(
                        "{automation}: its actions did not run, because the controls cannot be made: {}",
                        missing.error
                    ));
                }
                return notes;
            },
        };
        notes.extend(given_up(actions.retry_restores()));
        match job {
            Job::Run {
                owner,
                automation,
                watched,
                actions: steps,
                restore,
            } => notes.extend(steps.iter().enumerate().filter_map(|(position, step)| {
                let error = actions.perform(owner, step, restore, &watched).err()?;
                Some(format!(
                    "{automation}: action {} failed: {error}",
                    position.saturating_add(1)
                ))
            })),
            Job::Undo { owner, automation } => {
                notes.extend(actions.release(owner).err().map(|error| {
                    format!("{automation}: undoing failed, so it is tried again: {error}")
                }));
            },
        }
        notes
    }

    /// Tries the pending restores again, when no job came in time.
    fn retry(&mut self) -> Vec<String> {
        match &mut self.actions {
            Ok(actions) => given_up(actions.retry_restores()).into_iter().collect(),
            Err(_) => Vec::new(),
        }
    }

    /// How long to wait for a job before [`Worker::retry`], or `None` to
    /// wait for one however long it takes.
    fn retry_in(&self) -> Option<Duration> {
        let actions = self.actions.as_ref().ok()?;
        if actions.retries_soon() {
            Some(MEDIA_RETRY)
        } else if actions.has_pending() {
            Some(VOLUME_RETRY)
        } else {
            None
        }
    }

    /// Makes the controls again if they are missing and their wait is over.
    fn make_again(&mut self, now: Instant) -> Vec<String> {
        let Err(missing) = &mut self.actions else {
            return Vec::new();
        };
        if now < missing.retry_at {
            return Vec::new();
        }
        match (self.make)() {
            Ok(controls) => {
                self.actions = Ok(Actions::new(controls));
                vec!["actions can run now".to_owned()]
            },
            Err(error) => {
                missing.error = error.to_string();
                missing.retry_at = now.checked_add(missing.wait).unwrap_or(now);
                missing.wait = missing.wait.saturating_mul(2).min(CONTROLS_WAIT_MAX);
                Vec::new()
            },
        }
    }
}

/// The log line for a restore that failed again and is given up.
fn given_up(retried: Result<(), appytappy_actions::ActionError>) -> Option<String> {
    let error = retried.err()?;
    Some(format!(
        "a media restore failed again, so it is given up: {error}"
    ))
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

    /// One media session that plays until paused, and a system volume when
    /// `volume` holds one.
    struct OnePlayer {
        playing: bool,
        commands: Sender<MediaCommand>,
        /// How many play commands it refuses before it obeys.
        refusals: u32,
        volume: Option<VolumeLevel>,
        /// Whether setting the volume fails, as on a disconnected device.
        unplugged: bool,
    }

    impl OnePlayer {
        fn new(commands: Sender<MediaCommand>) -> Self {
            Self {
                playing: true,
                commands,
                refusals: 0,
                volume: None,
                unplugged: false,
            }
        }
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
            if command == MediaCommand::Play
                && let Some(left) = self.refusals.checked_sub(1)
            {
                self.refusals = left;
                return Ok(false);
            }
            self.playing = command == MediaCommand::Play;
            Ok(true)
        }

        fn system_volume(&mut self) -> io::Result<Option<Volume<()>>> {
            Ok(self.volume.map(|level| Volume { key: (), level }))
        }

        fn app_volumes(&mut self) -> io::Result<Vec<(String, Volume<()>)>> {
            Ok(Vec::new())
        }

        fn set_volume(&mut self, (): &(), level: VolumeLevel) -> io::Result<()> {
            if self.unplugged {
                return Err(io::Error::other("the device is gone"));
            }
            self.volume = Some(level);
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

    const AUTOMATION: &str = "Pause near the end";

    fn percent(percent: u8) -> VolumeLevel {
        VolumeLevel::from_percent(percent).expect("in range")
    }

    fn pause_then_mute() -> Box<[Action]> {
        Box::new([
            Action::Media {
                target: MediaTarget::Current,
                command: MediaCommand::Pause,
            },
            Action::SetVolume {
                target: VolumeTarget::System,
                level: percent(0),
            },
        ])
    }

    fn fire(actions: Box<[Action]>) -> Job {
        Job::Run {
            owner: OWNER,
            automation: AUTOMATION.to_owned(),
            watched: Watched {
                process: ProcessId::new(7),
                executable: "Example.exe".to_owned(),
            },
            actions,
            restore: Restore::OnRelease,
        }
    }

    fn release() -> Job {
        Job::Undo {
            owner: OWNER,
            automation: AUTOMATION.to_owned(),
        }
    }

    /// Makes `player` once; any later call fails.
    fn once(player: OnePlayer) -> impl FnMut() -> io::Result<OnePlayer> {
        let mut player = Some(player);
        move || player.take().ok_or_else(|| io::Error::other("made twice"))
    }

    #[test]
    fn jobs_run_in_order_and_failures_name_the_automation_and_action() {
        let (jobs, received) = mpsc::channel();
        let (failures, failed) = mpsc::channel::<ActionFailed>();
        jobs.send(fire(pause_then_mute())).expect("send");
        jobs.send(release()).expect("send");
        drop(jobs);
        let (commands, sent) = mpsc::channel();
        run(once(OnePlayer::new(commands)), &received, &failures);
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
    fn controls_that_cannot_start_are_made_again_after_a_growing_wait() {
        let (commands, sent) = mpsc::channel();
        let mut player = Some(OnePlayer::new(commands));
        let mut tries = Vec::new();
        let start = Instant::now();
        let at = |seconds: u64| start + Duration::from_secs(seconds);
        let (mut worker, notes) = Worker::start(
            || {
                tries.push(());
                match tries.len() {
                    1..=3 => Err(io::Error::other("no audio")),
                    _ => player.take().ok_or_else(|| io::Error::other("made twice")),
                }
            },
            start,
        );
        assert_eq!(
            notes,
            ["actions cannot run yet, so they are tried again when an automation fires: no audio"]
        );
        assert_eq!(worker.retry_in(), None, "nothing waits for a timer");
        let pause = || {
            fire(Box::new([Action::Media {
                target: MediaTarget::Current,
                command: MediaCommand::Pause,
            }]))
        };
        let not_run = [
            "Pause near the end: its actions did not run, because the controls cannot be made: no audio",
        ];
        // The first wait is a second; each failure doubles it.
        for (seconds, expected) in [
            (0, &not_run[..]),
            (1, &not_run[..]),
            (2, &not_run[..]),
            (3, &not_run[..]),
            (6, &not_run[..]),
        ] {
            assert_eq!(worker.job(pause(), at(seconds)), expected, "at {seconds}s");
        }
        assert_eq!(
            worker.job(release(), at(6)),
            Vec::<String>::new(),
            "nothing to undo"
        );
        assert_eq!(worker.retry(), Vec::<String>::new());
        assert_eq!(
            worker.job(pause(), at(7)),
            ["actions can run now"],
            "the fourth try works, at 1 + 2 + 4 seconds"
        );
        assert_eq!(sent.try_iter().collect::<Vec<_>>(), [MediaCommand::Pause]);
        drop(worker);
        assert_eq!(tries.len(), 4, "made at 0, 1, 3 and 7 seconds");
    }

    #[test]
    fn the_wait_between_tries_stops_growing_at_its_longest() {
        let start = Instant::now();
        let (mut worker, _notes) =
            Worker::start(|| Err::<OnePlayer, _>(io::Error::other("no audio")), start);
        let mut now = start;
        for _ in 0..10 {
            now += CONTROLS_WAIT_MAX;
            worker.job(release(), now);
        }
        let Err(missing) = &worker.actions else {
            panic!("the controls are never made");
        };
        assert_eq!(missing.wait, CONTROLS_WAIT_MAX);
        assert_eq!(missing.retry_at, now + CONTROLS_WAIT_MAX);
    }

    #[test]
    fn a_failed_media_restore_is_tried_once_more_soon() {
        let (commands, sent) = mpsc::channel();
        let mut player = OnePlayer::new(commands);
        player.refusals = 2;
        let (mut worker, notes) = Worker::start(once(player), Instant::now());
        assert_eq!(notes, Vec::<String>::new());
        let pause = Box::new([Action::Media {
            target: MediaTarget::Current,
            command: MediaCommand::Pause,
        }]);
        assert_eq!(
            worker.job(fire(pause), Instant::now()),
            Vec::<String>::new()
        );
        assert_eq!(
            worker.job(release(), Instant::now()),
            [
                "Pause near the end: undoing failed, so it is tried again: the media session did not take the command"
            ]
        );
        assert_eq!(worker.retry_in(), Some(MEDIA_RETRY));
        assert_eq!(
            worker.retry(),
            [
                "a media restore failed again, so it is given up: the media session did not take the command"
            ]
        );
        assert_eq!(worker.retry_in(), None);
        assert_eq!(
            sent.try_iter().collect::<Vec<_>>(),
            [MediaCommand::Pause, MediaCommand::Play, MediaCommand::Play]
        );
    }

    /// The controls of a worker that made them.
    fn controls<F>(worker: &mut Worker<OnePlayer, F>) -> &mut OnePlayer {
        let Ok(actions) = &mut worker.actions else {
            panic!("the controls were made");
        };
        actions.controls()
    }

    #[test]
    fn a_failed_volume_restore_is_tried_on_later_jobs_and_now_and_then() {
        let (commands, _sent) = mpsc::channel();
        let mut player = OnePlayer::new(commands);
        player.volume = Some(percent(80));
        let (mut worker, _notes) = Worker::start(once(player), Instant::now());
        let mute = || {
            Box::new([Action::SetVolume {
                target: VolumeTarget::System,
                level: percent(0),
            }])
        };
        assert_eq!(
            worker.job(fire(mute()), Instant::now()),
            Vec::<String>::new()
        );
        controls(&mut worker).unplugged = true;
        assert_eq!(
            worker.job(release(), Instant::now()).len(),
            1,
            "the failed undo is logged"
        );
        assert_eq!(worker.retry_in(), Some(VOLUME_RETRY));
        assert_eq!(
            worker.retry(),
            Vec::<String>::new(),
            "a volume restore is never given up"
        );
        assert_eq!(controls(&mut worker).volume, Some(percent(0)));
        controls(&mut worker).unplugged = false;
        let next = Job::Undo {
            owner: (SessionId(2), RuleIndex(0)),
            automation: "Another".to_owned(),
        };
        assert_eq!(worker.job(next, Instant::now()), Vec::<String>::new());
        assert_eq!(
            controls(&mut worker).volume,
            Some(percent(80)),
            "a later job puts it back"
        );
        assert_eq!(worker.retry_in(), None);
    }

    #[test]
    fn a_failed_media_restore_is_tried_again_when_no_job_comes() {
        let (jobs, received) = mpsc::channel();
        let (failures, failed) = mpsc::channel::<ActionFailed>();
        let (commands, sent) = mpsc::channel();
        let mut player = OnePlayer::new(commands);
        player.refusals = 1;
        let pause = Box::new([Action::Media {
            target: MediaTarget::Current,
            command: MediaCommand::Pause,
        }]);
        jobs.send(fire(pause)).expect("send");
        jobs.send(release()).expect("send");
        let actor = std::thread::spawn(move || run(once(player), &received, &failures));
        let patience = MEDIA_RETRY.saturating_mul(3);
        for expected in [MediaCommand::Pause, MediaCommand::Play, MediaCommand::Play] {
            assert_eq!(sent.recv_timeout(patience), Ok(expected));
        }
        drop(jobs);
        actor.join().expect("the thread ends when the jobs stop");
        assert_eq!(
            failed.try_iter().collect::<Vec<_>>(),
            [ActionFailed(
                "Pause near the end: undoing failed, so it is tried again: the media session did not take the command".to_owned()
            )],
            "the retry worked, so only the first failure is logged"
        );
    }
}
