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
- **Open:** a program or a file by its full path, or an `http` or `https`
  link. See [Opening](#opening).
- **Play a sound:** a WAV file by its full path. See [Sounds](#sounds).

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
so the action first checks that the window in front belongs to the process the
runtime follows for the watched application. If another window is in front,
even one of another process with the same executable name, it presses nothing
and the log says so; anti-cheat was checked only for the watched application,
so keys are pressed only into the watched application. An application whose
window belongs to a second process of its own cannot be pressed into. Windows does not deliver
keys to a window that runs as administrator unless AppyTappy does too, and says
nothing when it drops them. When it takes only some of the keys, every key that
went down is let go again.

Anti-cheat can see injected input and treat it as cheating, so an automation
with a key press action runs only for an application whose anti-cheat class
is `none` (see [#11](https://github.com/TheLunchTrae/appy-tappy/issues/11)). A
new application starts as `unknown`, so the user has to set it first. Saving
an automation checks this, and so does the runtime whenever it reads the
automations. Changing the class in the setup app makes the runtime read them
again at once, so it stops running that automation; a class changed by
editing `app.json` by hand counts from the next reload or restart.

## Opening

An open action names a full path on a drive, such as `C:\Tools\Timer.exe`,
or a link that starts with `http://` or `https://`, up to 2048 bytes. A
relative path would depend on the runtime's working folder, which the user
never sees, so it is refused. A network path, which starts with `\\`, is
refused too: reaching one sends the user's Windows sign-in to that server, and
a file there, such as a shortcut or a saved search, can hand a link of any kind
to its program.

It uses `ShellExecuteExW` with the default verb, as opening the target from
File Explorer does: a program runs in its own folder, a file opens in the
program its type is set to, and a link opens in the default browser. It passes
nothing to what it opens, so no reading of the watched application can reach a
program's command line. It asks Windows to show the window without taking the
keyboard focus, though a program may take it anyway, and it shows no error
dialog; a failure, such as a file that is not there, goes to the log.

The shell asks its callers for a single-threaded COM apartment, while Core
Audio's controls need the action thread in the multithreaded one, so each open
runs on a short-lived thread of its own. The action thread waits for it for at
most 2 seconds, so a program that is slow to start cannot hold up the actions
behind it. After that the log says it may still open, and it may. While a
thread that timed out is still running, a later open is skipped and the log
says so, so a target that never answers leaves one waiting thread, not one for
each time the automation runs. An open that answered in time holds up nothing.

Links are only `http` and `https`, because a link of another kind goes to
whichever program registered that kind, and some such programs have run
commands from a crafted link. An automations file from someone else can still
open any program or file on the computer and any web page, so read its open
actions before you use it.

## Sounds

A sound action names a WAV file by its full path on a drive, which ends in
`.wav`. It uses `PlaySoundW`, which returns once the sound is queued, so the
action thread does not wait for it to end. A new sound stops one that is still
playing. Played this way, the call does not report a missing file, so the
adapter checks that the file is there first; when it is not, the sound does not
play and the log says so. A file that is there but that Windows cannot play
fails without a sign. It plays on the default playback device, at the volume
Windows gives AppyTappy in the volume mixer.

Neither opening nor a sound is restricted by anti-cheat class: neither sends
input or touches the watched application's process. A program an open action
starts can send input of its own, which anti-cheat may count against the user;
the user guide says so.

## Restoring

A rule that restores on release undoes its actions when it is released (see
[Rules](Rules.md)). Each action records the state of each session or volume
it touched, just before it acted:

- **Pause** and **play** put back whether each session was playing. A session
  that was already in the wanted state is left alone, and so is its restore.
- **Set volume** puts back each audio session's own level, or the device's.
  A restore goes to the same device or session the action changed, even if
  the default device has changed since.
- **Next**, **previous**, **press keys**, **open**, and **play a sound**
  cannot be undone. A rule that restores needs at least one action that can.

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

A restore that fails is kept and tried again, and the log says so:

- **Volume:** tried again after each later job and every 10 seconds, until it
  works. A device that was unplugged gets a new volume control from Windows
  when it comes back, so the retry finds the device again by its endpoint ID,
  or the audio session again by its session ID on that device. An audio
  session that is no longer on its device ended with its app, so its restore
  is dropped.
- **Media:** tried once more after 2 seconds, then given up, because a player
  that refused twice is unlikely to take a third request, and playing music
  long after the release would surprise the user. Other jobs that come
  meanwhile do not shorten the wait.

A restoring rule that changes a target with a pending restore takes the
restore over, so its own release puts back the state from before the first
change, and the pending restore is not sent just before the rule's action; a
rule that does not restore drops it. When an earlier rule that changed the
same target is released, its undo takes the pending restore in: it puts back
the earlier rule's state, from the state the failed restore left, so the
pending restore is not sent after it. When the runtime stops, each restore
still pending gets one last try.

## When the controls cannot start

The action worker makes the media and audio controls when the runtime starts.
Right after sign-in, Windows may not have the media session manager ready yet,
so the controls start without it and ask for it again when a media action
needs it, with the same growing wait as below, because each request can take
2 seconds; volume and key press actions run meanwhile.

If the controls cannot be made at all, the log says so, and each automation
that fires meanwhile logs that its actions did not run. The worker makes the
controls again a second after the first failure and twice as long after each
one after it, up to a minute, or when an automation fires after that wait.
The worker tells the owner when actions cannot run and when they can again,
and the [tray](Architecture.md#tray) shows it; the log says so too. An
automation that fired while the controls were missing changed nothing, so its
release has nothing to undo.

## Not in v1

- **Sending keys to another application:** keys go only to the watched
  application while it is in front, so a hotkey meant for another application
  cannot be sent.
- **Arguments for a program, and links other than `http` and `https`:** see
  [Opening](#opening).
- **HTTP and WebSocket calls** (to streaming or smart-home software): useful,
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
