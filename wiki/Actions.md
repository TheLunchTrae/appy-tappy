# Actions

An action is what a rule does when it fires. The types live in
`appytappy-config` (`crates/config/src/action.rs`), and the Windows code that
performs them lives in `appytappy-actions`. The runtime's action worker calls
that crate, and the engine only says which rule fired. The setup app calls it
for one thing, a click the user asks for ([Trying a click](#trying-a-click)).

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
- **Click in another application:** a left, right, or double click at a spot
  in a saved application's window, or on a picture wherever it shows in an
  area of that window, with the cursor or sent to the window as messages. See
  [Clicks](#clicks).

An app is named the way Windows reports it, ignoring ASCII case. For media, that
is the session's app ID: usually the executable name, such as `Player.exe`, for
a desktop app, unless the app sets its own ID; the package name for a Microsoft
Store app; or the browser's name. For volume, it is the executable name, such as
`Player.exe`. Naming an audio session's executable lists the processes through
`appytappy-sources`, the same snapshot call detection uses, which never opens a
process, so no action opens a watched application's process. The action worker
hands that lister to the controls, so `appytappy-actions` keeps no process code
of its own.

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

## Clicks

A click names the saved application it clicks in, by its ID, and says where
in that application's window it lands:

- **A spot:** a distance across and down from one corner of the window, so a
  spot near the right edge stays put when the window gets wider.
- **A picture:** a PNG in the `clicks` folder beside `automations.json`, with
  the area of the window it is looked for in, the window's size when it was
  taken, where on it to press, and a tolerance. Its transparent pixels are
  not compared. The pictures cannot live in the target application's folder,
  because saving that application removes the pictures its own `app.json`
  does not name.

A click also says how it reaches the window, its delivery:

- **With the cursor,** the default: the window comes to the front, and the
  real cursor moves there and presses, so it works wherever a person can
  click. See [A click with the cursor](#a-click-with-the-cursor).
- **As messages:** the click is posted to the window as mouse messages, so
  neither the cursor nor the focus moves, and a game in front, including one
  in exclusive full screen, keeps running undisturbed. Many applications
  ignore such a click. See
  [A click sent as messages](#a-click-sent-as-messages).

The owner decided that the delivery is each click's own choice and that the
cursor stays the default. `automations.json` keeps it on the click action as
`delivery`, `cursor` or `message`, and a click that names none clicks with the
cursor. Giving the focus back belongs to a click with the cursor alone:
`ClickDelivery` in `appytappy-config` holds it only in its `Cursor` variant, so
a click sent as messages cannot carry one, and the file's `returnFocus` is not
read for it. The setup app calls the two **Move the cursor** and **Send to the
window**, and shows **Go back to the window that was in front** only for the
first.

The runtime places a click just before it runs, on the action thread, because
where it lands depends on where the window is now. It takes the target's
window that a new session would read, as
[Lifecycle](Architecture.md#lifecycle) describes, with the application's
window match, but never a window taken only as the fallback: that may be
another of the application's windows, where the spot or the picture means
something else. A window that is not open, is minimized, or is hidden is not
clicked. The
place is measured in the window's frame as the screen shows it, which the
desktop window manager reports without the invisible borders that the
window's own bounds take in, so it matches what a capture of the window
shows. For a picture, the window must have the size the picture was taken
at; the runtime then grabs one frame of the area, waits up to a second for
it, and looks for the picture there (see
[Screen regions](Screen-regions.md)). It reads the window's frame again after
the search, and a window that moved since the grab is not clicked. Nothing is
clicked when the picture is not found, and the log says why. Placing is the
same for both deliveries. `place` in `appytappy-sources`
(`crates/sources/src/screen/place.rs`) owns it, for the runtime and for the
click the setup app sends on request. `ClickSearch` in
`appytappy-sources` owns reading a click's picture, checking it against the
click, and turning where it is found into where the click lands. The runtime
places clicks with it, and the setup app's **Find it now** and its check
before saving an automation use it too, so the preview lands where the
runtime clicks.

The search compares the picture's kept pixels with each place it fits in the
area and takes the closest place within its tolerance: the average distance
of each compared pixel's red, green, and blue from the picture's. Before it
compares a place pixel by pixel, it checks a bound from each row's summed
channels, which running sums of the area give in a few steps, and it stops
comparing a place once that place cannot beat the best so far. A 40×20
picture in a 180×120 area takes about 8 ms, so the setup app draws the area
around where the picture shows rather than across the whole window. An area
holds at most 160,000 pixels, such as 400 by 400, because other actions wait
for the search.

`click` in `appytappy-actions` (`crates/actions/src/click.rs`) runs a placed
click either way and makes every choice. It reaches Windows through the
`Input` trait, whose adapter only makes the calls, so the setup app can send a
click without the media and audio controls that the other actions need.

### A click with the cursor

A click with the cursor clicks only while the watched application or the
application it
clicks in has the keyboard focus, because anti-cheat was checked for those
two, and only while no mouse button or modifier key is held, so it is never
a drag or a Ctrl-click. It brings the click's window to the front first with
`SetForegroundWindow`, because a game in front may keep the cursor inside
its window or hold the mouse, and would then get the press itself. Windows
lets only the program in front move the foreground, so the adapter attaches
its thread to the input of the window in front for the moment it asks
(`AttachThreadInput`), unless that window is hung, since sharing a hung
window's input could wait on it. Windows may still refuse, and the click is then
skipped. It clicks only when the click's window is the one that
shows at its place, so it never lands in a window that covers it. It then
moves the cursor there with `SetCursorPos` and reads the cursor back, since
Windows keeps the cursor inside a window that confines it without failing
the move; a cursor that did not get there skips the press. It presses
through `SendInput`, one button event at a time, 20 ms apart so an
application that reads the buttons once a frame sees each button go down,
and for a double click up again between the two, waits
40 ms so one that reads the cursor once a frame still sees it there, and
moves the cursor back. A double click stays well inside the double-click
time. Injected events name the physical buttons, so with the buttons
swapped in Windows' settings the adapter swaps them back. When Windows drops
a press, a button that went down goes up again. With return focus on, which
is the default, the window that was in front asks for the focus back;
Windows may refuse that too, and the log then says the click landed but the
focus did not go back. A right click that opens a menu usually wants return
focus off, since the menu closes when the focus goes back. Windows does not
let a program send input to a program running as administrator, and
`SendInput` does not say when it was blocked, so a click in such a window
is logged as done although nothing happened.

### A click sent as messages

A click sent as messages is placed like any click: in the window a new
session would read, never one taken only as the fallback, that is open and
neither minimized nor hidden; at the same spot, or where the same picture
search finds the picture, while the window has the size the picture was taken
at; and not in a window that moved since the grab. What it needs after that
differs:

- **No focus.** A click with the cursor needs the watched application or the
  one it clicks in to be in front because `SendInput` goes to whichever window
  is in front. A posted message goes only to the window it names, so the
  click is sent whichever window is in front. Nothing comes to the front, and
  there is no focus to give back.
- **No look at what shows at its place.** A message cannot land in a window
  that covers the click's window, so a covered window is still clicked.
- **No wait for held buttons or keys.** Its messages name the one button it
  presses and no modifier key, so it is never a Ctrl-click by what the message
  says. An application that asks Windows which keys are down while it handles
  the message still sees the keys the user holds.

The messages go to the deepest window under the click's place, among the
click's window and its child windows, because the mouse itself goes to the
child under it. At each window, `click` asks where the place is in that
window's client area (`ScreenToClient`), checks that it lies inside it
(`GetClientRect`), and asks for the child window there
(`ChildWindowFromPointEx`), which passes over a child that is hidden,
disabled, or transparent to the mouse, as the mouse does. It stops at a window
with no child there, and gives up after 64 windows, far more than Windows
nests. A place outside a client area, such as on a title bar, a border, or a
scroll bar, is not sent: those places get other messages, which a click does
not send. Windows stretches a window whose program does not follow its
monitor's scale, and such a window counts in its own pixels, so the place is
scaled by the window's dots per inch over its monitor's (`GetDpiForWindow`
and `GetDpiForMonitor`).

The click is then posted with `PostMessageW`: `WM_MOUSEMOVE`, the button's
down message with its button flag (`MK_LBUTTON` or `MK_RBUTTON`), and its up
message, each with the place in its `lParam`, 20 ms apart so an application
that reads its messages once a frame sees each. A double click goes down, up,
down again as `WM_LBUTTONDBLCLK`, and up. When Windows refuses a message after
the button went down, the up message is posted once more. `PostMessageW`
returns without waiting for the window, so a window that hangs does not hold
up the actions behind the click.

What a click sent as messages cannot promise:

- **Many applications ignore it.** A game that reads the mouse through raw
  input or DirectInput never sees window messages, and some applications that
  draw their own controls, or that ask where the real cursor is, do not act on
  them. Which kinds of applications take such a click is not tested yet: it is
  an open question, and this page does not answer it.
  [Checking on Windows](#checking-on-windows) says how to check an
  application.
- **Nothing reports whether it worked.** A posted message has no answer, so
  AppyTappy knows only that Windows took the messages. Each click that is sent
  gets a line in the log, which says that it was sent and that nothing tells
  whether the application acted on it. The log never says that such a click
  worked.
- **A window that runs as administrator refuses it.** Windows blocks messages
  from a program with fewer rights, and for a posted message, unlike
  `SendInput`, it says so. The click fails, and the log says why.
- **The application may think the pointer stays.** The messages say that the
  pointer came to the click's place, and nothing says that it left, so an
  application that highlights what the pointer is over may keep the highlight
  until the real pointer moves over its window.

### Anti-cheat and clicks

Both applications need the anti-cheat class `none`, the watched one and the
one it clicks in, because a click is injected input too. Saving an
automation checks this, and so does the runtime whenever it reads the
automations. A click cannot be undone, so a rule that restores needs another
action that can.

The owner decided that a click sent as messages does not relax this rule. A
posted message is not system-wide input, but it is still input that AppyTappy
makes for another program, and anti-cheat can notice mouse messages that no
mouse made and treat them as cheating. So `Action::input_sent` counts a click
whatever its delivery, and both applications need the class `none` for either.
Letting a click sent to another application run while a protected application
is watched is a separate decision, which needs the owner.

### Trying a click

On the click target screen, **Click it now** sends one real click with the
button and the delivery chosen there, when the user presses it and at no
other time. The setup app's owner checks the anti-cheat class of the
application it clicks in, and of the watched application once the automation
watches a value. It then places the click with the same `place` and sends it
with the same `click` and the same adapter as the runtime, which is what the
setup app depends on `appytappy-actions` for. The setup app stands where the
runtime has the watched application: a click with the cursor clicks only
while the setup app's window or the click's window is in front, as it is when
the user has just pressed the button, and the focus goes back to the setup
app when the click asks for that. The screen says that the click was sent,
not that it worked.

## Restoring

A rule that restores on release undoes its actions when it is released (see
[Rules](Rules.md)). Each action records the state of each session or volume
it touched, just before it acted:

- **Pause** and **play** put back whether each session was playing. A session
  that was already in the wanted state is left alone, and so is its restore.
- **Set volume** puts back each audio session's own level, or the device's.
  A restore goes to the same device or session the action changed, even if
  the default device has changed since.
- **Next**, **previous**, **press keys**, **open**, **play a sound**, and
  **click** cannot be undone. A rule that restores needs at least one action that can.

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

A click with the cursor is checked by hand. Save a click on a spot and one on
a picture in
another application's window, with both applications' anti-cheat set to
none, and check that each lands where the setup app showed it, on a window
with and without a title bar, maximized, and on a second monitor with
another scale; that a window covering the place stops the click; that a
game that keeps the cursor inside its window gets no click; and that the
focus goes back to the watched application.

For a click sent as messages, the adapter's tests make a hidden window with
child windows, post each kind of click to it, and read the messages, their
button flags, and their places back from its queue. They run in the `windows`
CI job. Whether a real application acts on such a click is checked by hand,
one application at a time: set up a target in it with **Send to the window**,
put another window in front, select **Click it now**, and see whether the
application did what a click there does, while the cursor and the window in
front stay as they were. Check too a place inside a child control, a window
that Windows stretches on a monitor with another scale, and a window that runs
as administrator, which the log says it cannot click.
