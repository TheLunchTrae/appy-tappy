# Actions

An action is what a rule does when it fires. The types live in
`appytappy-config` (`crates/config/src/action.rs`), and the Windows code that
performs them lives in `appytappy-actions`. Only the runtime's action worker
calls that crate; the engine only says which rule fired.

## The v1 set

- **Media:** play, pause, next track, or previous track, for the current
  media session or one app's session. It uses the Windows media transport
  controls, the same ones the media keys and the volume overlay use, so it
  works for music players and browsers without any app's own API.
- **Set volume:** the default playback device's volume, or one app's volume
  on that device. It uses Core Audio, the API behind the Windows volume
  mixer.
- **Press keys:** one key, with any of Ctrl, Alt, Shift, and Win held, such
  as `Ctrl+Shift+M`. See [Key presses](#key-presses).

An app is named the way Windows reports it, ignoring ASCII case. For media, that
is the session's app ID: usually the executable name, such as `Player.exe`, for
a desktop app, unless the app sets its own ID; the package name for a Microsoft
Store app; or the browser's name. For volume, it is the executable name, such as
`Player.exe`. Naming an audio session's executable takes a process snapshot,
which never opens a process, so no action opens a watched application's process.

An app can have several sessions at once, such as one per browser tab:

- **Pause** pauses every playing session of the app. When none plays, it
  changes nothing but still holds them all for restoring (see below).
- **Play** does nothing while one of its sessions plays, and otherwise starts
  the first.
- **Next** and **previous** go to the first playing session, or else the
  first session.
- **Set volume** sets every audio session of the executable.

Volume actions use the default playback device. Choosing another device can
come later.

A media request that gets no answer within two seconds fails, so an app
that hangs cannot hold up the actions behind it. The app may still act on
a request after it times out; such a late change is not restored.

## Key presses

A key press action is saved as text: any of `Ctrl`, `Alt`, `Shift`, and `Win`,
in that order, then one key, joined by `+`. The key is a letter, a digit,
`F1` to `F24`, `Space`, `Enter`, `Tab`, `Escape`, `Backspace`, `Insert`,
`Delete`, `Home`, `End`, `PageUp`, `PageDown`, `Up`, `Down`, `Left`, or
`Right`. Reading it ignores case and spaces around each part, and the setup
app saves it in the spelling above.

The action holds the modifiers down in order, presses the key, holds them
for 40 ms so an application that checks the keyboard once a frame still sees
them, then lets the key and the modifiers go in reverse. A modifier the user is
holding already is left alone, so it stays down for them; a held modifier that
the keys do not name joins the press. The modifiers are the left-hand ones, and
the navigation and arrow keys are sent as the dedicated keys, not the number
pad's.

It uses `SendInput`, which types into whichever window has the keyboard focus,
so the action first checks that the window in front belongs to the watched
application's executable, the one the runtime follows. If another window is in
front, it presses nothing and the log says so; anti-cheat was checked only for
the watched application, so keys never go anywhere else. Naming the window's
executable takes a process snapshot, so it never opens that process. Windows
drops keys meant for a window that runs as administrator unless AppyTappy runs
as administrator too; when it takes only some of the keys, every key that went
down is let go again.

Anti-cheat can see injected input and treat it as cheating, so an automation
with a key press action runs only for an application whose anti-cheat class
is `none` (see [#11](https://github.com/TheLunchTrae/appy-tappy/issues/11)). A
new application starts as `unknown`, so the user has to set it first. Saving
an automation checks this, and so does the runtime whenever it reads the
automations. Changing the class in the setup app makes the runtime read them
again at once, so it stops running that automation; a class changed by
editing `app.json` by hand counts from the next reload or restart.

## Restoring

A rule that restores on release undoes its actions when it is released (see
[Rules](rules.md)). Each action records the state of each session or volume
it touched, just before it acted:

- **Pause** and **play** put back whether each session was playing. A session
  that was already in the wanted state is left alone, and so is its restore.
- **Set volume** puts back each audio session's own level, or the device's.
  A restore goes to the same device or session the action changed, even if
  the default device has changed since.
- **Next**, **previous**, and **press keys** cannot be undone. A rule that
  restores needs at least one action that can.

A rule's actions are undone newest first. When several restoring rules change
the same session or volume, the original state comes back only when the last
of them is released; until then, releasing one leaves the others' changes in
place. If one rule pauses the music while an icon is visible and another
lowers its volume while the watched application has the focus, switching to
another window while the icon shows puts the volume back but keeps the music
paused.

A rule that does not restore makes a permanent change: no earlier rule's
release undoes it. If an action fails partway, such as on the second of two
audio sessions, what it changed before the failure is still restored.

## Not in v1

- **Sending keys to another application:** keys go only to the watched
  application while it is in front, so a hotkey meant for another application
  cannot be sent.
- **Running a command or opening a URL:** configs may be shared, and a
  shared config should not be able to run programs.
- **HTTP and WebSocket calls** (OBS, Streamer.bot, Home Assistant): useful,
  but each needs its own connection settings and error handling, so they can
  come after the core set works.

## Checking on Windows

CI compiles and lints the Windows code, and the logic is tested with fake
controls on every platform. Two tests drive a real media player; they change
what is playing, so they only run when asked. Start a desktop app that plays
audio and whose media session reports its executable name as its app ID,
which most desktop apps do. Play something, and run with
`APPYTAPPY_TEST_PLAYER` set to that executable name:

```sh
APPYTAPPY_TEST_PLAYER=Player.exe cargo test -p appytappy-actions --test media_player -- --ignored --test-threads=1
```

One test pauses the player with two overlapping rules and checks that it
resumes only after both are released, which also checks that Windows keeps
identifying the player's session the same way. The other lowers the player's
volume and puts it back. Each puts the player back before checking anything,
so a failed check leaves it as it was.
