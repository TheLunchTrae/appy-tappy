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
that is the session's app ID: `Spotify.exe` for the desktop app, the
package name for the Microsoft Store version, or the browser's name. For
volume, it is the executable name, such as `Spotify.exe`. Naming an audio
session's executable takes a process snapshot, which never opens a process,
so no action opens a game's process.

An app can have several sessions at once, such as one per browser tab:

- **Pause** pauses every playing session of the app.
- **Play** does nothing while one of its sessions plays, and otherwise starts
  the first.
- **Next** and **previous** go to the first playing session, or else the
  first session.
- **Set volume** sets every audio session of the executable.

Volume actions use the default playback device. Choosing another device can
come later.

A media request that gets no answer within two seconds fails, so an app
that hangs cannot hold up the actions behind it.

## Restoring

A rule that restores on release undoes its actions when it is released (see
[Rules](rules.md)). Each action records the state of each session or volume
it touched, just before it acted:

- **Pause** and **play** put back whether each session was playing. A session
  that was already in the wanted state is left alone, and so is its restore.
- **Set volume** puts back each audio session's own level, or the device's.
  A restore goes to the same device or session the action changed, even if
  the default device has changed since.
- **Next** and **previous** cannot be undone. A rule that restores needs at
  least one action that can.

A rule's actions are undone newest first. When several restoring rules change
the same session or volume, the original state comes back only when the last
of them is released; until then, releasing one leaves the others' changes in
place. If a rule pauses Spotify at a boss and another lowers its volume
while the game has the focus, alt-tabbing during the boss puts the volume
back but keeps Spotify paused.

A rule that does not restore makes a permanent change: no earlier rule's
release undoes it. If an action fails partway, such as on the second of two
audio sessions, what it changed before the failure is still restored.

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
puts it back. Each puts Spotify back before checking anything, so a failed
check leaves it as it was.
