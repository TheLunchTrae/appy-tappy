# Actions

An action is what a rule does when it fires. The types live in
`appytappy-config` (`crates/config/src/action.rs`), and the Windows code that
performs them lives in `appytappy-actions`. Only the runtime's action worker
calls that crate; the engine only says which rule fired.

## The v1 set

- **Media:** play, pause, next track, or previous track, for the current
  media session or one app's session. It uses the Windows media transport
  controls, the same ones the media keys and the volume overlay use, so it
  works for Spotify and browsers without any app's own API.
- **Set volume:** the default playback device's volume, or one app's volume
  on that device. It uses Core Audio, the API behind the Windows volume
  mixer.

An app is named the way Windows reports it, ignoring ASCII case. For media,
that is the session's app ID: `Spotify.exe` for the desktop app, or the
package name for the Microsoft Store version. For volume, it is the
executable name, such as `Spotify.exe`, and the action sets every audio
session of that executable. Naming an audio session's executable takes a
process snapshot, which never opens a process, so no action opens a game's
process.

Volume actions use the default playback device only. Choosing another device
can come later.

## Restoring

A rule that restores on release undoes its actions when it is released (see
[Rules](rules.md)):

- **Pause** resumes the session it paused. If the session was already
  paused, the action does nothing, and so does its restore.
- **Play** pauses the session it started, under the same rule.
- **Set volume** puts back the level it read just before the change.
- **Next** and **previous** cannot be undone. A rule that restores needs at
  least one action that can.

Restore is per rule: all of a rule's actions that can be undone are undone
together.

## Not in v1

- **Keypresses:** input goes to the focused window, which is the game, so a
  hotkey meant for another app would reach the game instead. Injected input
  is also visible to anti-cheat (see
  [#11](https://github.com/TheLunchTrae/appy-tappy/issues/11)).
- **Running a command or opening a URL:** configs may be shared, and a
  shared config should not be able to run programs.
- **HTTP and WebSocket calls** (OBS, Streamer.bot, Home Assistant): useful,
  but each needs its own connection settings and error handling, so they can
  come after the core set works.

## Checking on Windows

CI compiles and lints the Windows code, and the logic is tested with fake
controls on every platform. Two tests drive the real Spotify desktop app;
they change what is playing, so they only run when asked. Start Spotify,
play a song, and run:

```sh
cargo test -p appytappy-actions --test spotify -- --ignored --test-threads=1
```

One test pauses Spotify and resumes it; the other lowers Spotify's volume and
puts it back.
