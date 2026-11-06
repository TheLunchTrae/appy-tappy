//! Manual checks against the Spotify desktop app on Windows.
//!
//! They change what is playing, so they are ignored by default. Start
//! Spotify, play a song, and run:
//!
//! ```sh
//! cargo test -p appytappy-actions --test spotify -- --ignored --test-threads=1
//! ```

#![cfg(windows)]

use std::io;
use std::thread;
use std::time::Duration;

use appytappy_actions::win::WindowsControls;
use appytappy_actions::{Controls, Restore, perform, restore};
use appytappy_config::action::{
    Action, AppName, InvalidAppName, MediaCommand, MediaTarget, VolumeLevel, VolumeTarget,
};

/// How long Spotify takes to report a new playback state.
const SETTLE: Duration = Duration::from_secs(2);

fn spotify() -> Result<AppName, InvalidAppName> {
    AppName::new("Spotify.exe")
}

/// Whether Spotify is playing, or `None` when it has no media session.
fn playing(controls: &mut WindowsControls) -> io::Result<Option<bool>> {
    let target = MediaTarget::App(spotify().map_err(io::Error::other)?);
    Ok(controls
        .media_session(&target)?
        .map(|session| session.playing))
}

#[test]
#[ignore = "needs Spotify playing; pauses and resumes it"]
fn pauses_and_resumes_spotify() {
    let mut controls = WindowsControls::new().expect("the controls connect");
    assert_eq!(
        playing(&mut controls).expect("readable"),
        Some(true),
        "start a song in Spotify first"
    );
    let pause = Action::Media {
        target: MediaTarget::App(spotify().expect("valid")),
        command: MediaCommand::Pause,
    };
    let undo = perform(&mut controls, &pause)
        .expect("Spotify pauses")
        .expect("a playing session restores");
    thread::sleep(SETTLE);
    assert_eq!(playing(&mut controls).expect("readable"), Some(false));
    restore(&mut controls, &undo).expect("Spotify resumes");
    thread::sleep(SETTLE);
    assert_eq!(playing(&mut controls).expect("readable"), Some(true));
}

#[test]
#[ignore = "needs Spotify playing; lowers its volume and puts it back"]
fn lowers_and_restores_spotify_volume() {
    let mut controls = WindowsControls::new().expect("the controls connect");
    let target = VolumeTarget::App(spotify().expect("valid"));
    let before = controls
        .volume(&target)
        .expect("the volume can be read")
        .expect("Spotify has an audio session");
    let lower = Action::SetVolume {
        target: target.clone(),
        level: VolumeLevel::from_percent(20).expect("at most 100"),
    };
    let undo = perform(&mut controls, &lower)
        .expect("the volume changes")
        .expect("a volume change restores");
    assert_eq!(
        undo,
        Restore::Volume {
            target: target.clone(),
            level: before
        }
    );
    let lowered = controls.volume(&target).expect("read").expect("session");
    assert!((lowered.scalar() - 0.2).abs() < 0.01, "Spotify is at 20%");
    thread::sleep(SETTLE);
    restore(&mut controls, &undo).expect("the volume goes back");
    assert_eq!(controls.volume(&target).expect("read"), Some(before));
}
