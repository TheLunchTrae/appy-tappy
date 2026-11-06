//! Manual checks against the Spotify desktop app on Windows.
//!
//! They change what is playing, so they are ignored by default. Start
//! Spotify, play a song, and run:
//!
//! ```sh
//! cargo test -p appytappy-actions --test spotify -- --ignored --test-threads=1
//! ```
//!
//! Each test restores Spotify before it checks anything, so a failed check
//! leaves it as it was.

#![cfg(windows)]

use std::io;
use std::thread;
use std::time::Duration;

use appytappy_actions::win::{MediaKey, WindowsControls};
use appytappy_actions::{Actions, Controls};
use appytappy_config::action::{
    Action, AppName, InvalidAppName, MediaCommand, MediaTarget, VolumeLevel, VolumeTarget,
};
use appytappy_config::rule::Restore;

/// How long Spotify takes to report a new playback state.
const SETTLE: Duration = Duration::from_secs(2);

/// How far a volume read back may be from the level set, because Windows
/// stores levels with less precision.
const TOLERANCE: f32 = 0.01;

fn spotify() -> Result<AppName, InvalidAppName> {
    AppName::new("Spotify.exe")
}

/// Whether a Spotify session is playing, or `None` when it has none.
fn playing(controls: &mut WindowsControls) -> io::Result<Option<bool>> {
    let spotify = spotify().map_err(io::Error::other)?;
    let sessions: Vec<bool> = controls
        .media_sessions()?
        .into_iter()
        .filter(|session| spotify.matches(&session.app))
        .map(|session| session.playing)
        .collect();
    Ok((!sessions.is_empty()).then(|| sessions.contains(&true)))
}

/// Spotify's media sessions.
fn keys(controls: &mut WindowsControls) -> io::Result<Vec<MediaKey>> {
    let spotify = spotify().map_err(io::Error::other)?;
    Ok(controls
        .media_sessions()?
        .into_iter()
        .filter(|session| spotify.matches(&session.app))
        .map(|session| session.key)
        .collect())
}

/// The levels of Spotify's audio sessions, sorted.
fn levels(controls: &mut WindowsControls) -> io::Result<Vec<f32>> {
    let spotify = spotify().map_err(io::Error::other)?;
    let mut levels: Vec<f32> = controls
        .app_volumes()?
        .into_iter()
        .filter(|(executable, _)| spotify.matches(executable))
        .map(|(_, volume)| volume.level.scalar())
        .collect();
    levels.sort_by(f32::total_cmp);
    Ok(levels)
}

#[test]
#[ignore = "needs Spotify playing; pauses and resumes it"]
fn pauses_and_resumes_spotify() {
    let mut actions = Actions::new(WindowsControls::new().expect("the controls connect"));
    assert_eq!(
        playing(actions.controls()).expect("readable"),
        Some(true),
        "start a song in Spotify first"
    );
    // Overlapping rules rely on Windows handing out the same session object
    // each time, which keeps their restores in order.
    let first = keys(actions.controls()).expect("readable");
    let second = keys(actions.controls()).expect("readable");
    assert_eq!(first, second, "a session keeps its identity");

    let pause = Action::Media {
        target: MediaTarget::App(spotify().expect("valid")),
        command: MediaCommand::Pause,
    };
    let paused = actions.perform(1_u8, &pause, Restore::OnRelease);
    let held = actions.perform(2_u8, &pause, Restore::OnRelease);
    thread::sleep(SETTLE);
    let while_paused = playing(actions.controls());
    let first_release = actions.release(1);
    thread::sleep(SETTLE);
    let while_held = playing(actions.controls());
    actions.release(2).expect("Spotify resumes");
    thread::sleep(SETTLE);
    paused.expect("Spotify pauses");
    held.expect("the second pause holds Spotify");
    first_release.expect("the first release");
    assert_eq!(while_paused.expect("readable"), Some(false));
    assert_eq!(
        while_held.expect("readable"),
        Some(false),
        "the second rule keeps Spotify paused"
    );
    assert_eq!(playing(actions.controls()).expect("readable"), Some(true));
}

#[test]
#[ignore = "needs Spotify playing; lowers its volume and puts it back"]
fn lowers_and_restores_spotify_volume() {
    let mut actions = Actions::new(WindowsControls::new().expect("the controls connect"));
    let before = levels(actions.controls()).expect("readable");
    assert!(!before.is_empty(), "start a song in Spotify first");
    let lower = Action::SetVolume {
        target: VolumeTarget::App(spotify().expect("valid")),
        level: VolumeLevel::from_percent(20).expect("at most 100"),
    };
    let lowered = actions.perform(0_u8, &lower, Restore::OnRelease);
    let while_lowered = levels(actions.controls());
    thread::sleep(SETTLE);
    actions.release(0).expect("the volume goes back");
    lowered.expect("the volume changes");
    let while_lowered = while_lowered.expect("readable");
    assert!(
        while_lowered
            .iter()
            .all(|level| (level - 0.2).abs() < TOLERANCE),
        "Spotify is at 20%: {while_lowered:?}"
    );
    let after = levels(actions.controls()).expect("readable");
    assert_eq!(after.len(), before.len());
    for (after, before) in after.iter().zip(&before) {
        assert!(
            (after - before).abs() < TOLERANCE,
            "{after} is back to {before}"
        );
    }
}
