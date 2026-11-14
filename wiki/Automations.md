# Automations

An automation watches one [value](Values.md) and runs its actions, in order,
when a condition on that value becomes true. For example: when **Progress**
is at least 90, pause the music.

An automation is separate from its value, so any number of automations can
watch the same value.

The **Automations** page lists each automation with a switch that turns it on
or off, its condition, its actions, and the application that must be running
for it to run. If an automation can no longer run, the reason shows under its
name. A warning at the top says when
[AppyTappy's background part isn't running](#when-appytappys-background-part-isnt-running),
since then no automation runs.

## Make an automation

On the Automations page, select **New automation**. To change one, select
**Edit**.

1. **Name** it, such as `Pause music near the end`.
2. **When.** Choose the application and pick one of its values. If the
   application or the value isn't saved yet, make it from here, as
   [What to watch](#what-to-watch) describes. Until there is a value to
   watch, the editor says which step comes next. Then choose how the value
   compares:
   - A value that reads a number: **is at least**, **is above**,
     **is at most**, **is below**, **is**, or **is not** a number you type.
   - A value that reads yes or no: **is** or **is not**, then **Yes** or
     **No**.
   - A value that reads a state: **is** or **is not**, then one of its
     states or `none`, such as `Status is Paused`.

   An automation watches one value.

   While the value's application is open, the editor reads the value about
   twice a second and says what it reads now and whether the condition is
   true, so you can try a target before you save it. It says so when the
   application is not open, or when the value doesn't read at the window's
   [size](Values.md#window-size) now.

3. **Do, in order.** Add any number of [actions](#actions), and move them up
   or down.
4. **Afterwards.** Choose whether AppyTappy undoes the actions when the
   condition stops being true, and how long to wait before the automation can
   run again. See [Undo and running again](#undo-and-running-again).
5. **Save automation.** Until the automation is complete, the editor says
   what is still missing. To finish it later, select **Save as draft**
   ([Drafts](Drafts.md)).

Saving, deleting, or switching an automation takes effect at once, and so
does any other change that affects an application's automations, such as
changing its anti-cheat setting. A change to what runs, such as an
automation's condition or actions, or a value's region, starts all of that
application's automations over: they undo what they changed first, and one
whose condition still holds runs again. Renaming an application, a value or an
automation starts nothing over.

## What to watch

An automation watches one value of one application. The editor lists the
saved applications and, for the chosen one, its values. With one saved
application, it is chosen for you.

- **Add application** adds an application that isn't in the list yet, as
  [Applications](Applications.md#add-change-or-remove-an-application)
  describes. Saving it brings you back to the automation with that
  application chosen.
- **New value** makes a value for the chosen application, as
  [Make a value](Values.md#make-a-value) describes. It starts with that
  application chosen. A number's reference is made there too, with
  **New reference from this region**.
  Saving the value brings you back to the automation with that value picked.

Whichever you open, the automation keeps its name, condition, actions, and
the rest as you left them, and **Cancel** there returns to it unchanged. When
you come back, the keyboard focus is on the next step. For a moment the
automation can say **Reading what you saved…**, and it can be saved once the
value is picked. If the application or the value is gone by then, the
automation says so and keeps what you entered. So a
first automation needs no other page: select **New automation**, add the
application, make the value, and then say what happens.

Selecting a page while one of these is open leaves the automation too, as
[Leave before you save](Getting-started.md#leave-before-you-save) describes.

## Actions

- **Pause media**, **Play media**, **Next track**, and **Previous track**
  control music or video the way your keyboard's media keys do, so they work
  with most music players and browsers.
- **Set volume** sets a volume, from 0 to 100 percent.
- **Press keys** presses keys such as `Ctrl+Shift+M`. See
  [Key presses](#key-presses).
- **Open a program, file, or link** opens a program or a file, as
  double-clicking it does, or a web page. See
  [Opening and sounds](#opening-and-sounds).
- **Play a sound** plays a WAV file. See
  [Opening and sounds](#opening-and-sounds).
- **Click in another application** clicks a spot or a picture, such as a
  button, in the window of a saved application. See [Clicks](#clicks).

Media and volume actions choose what they change:

- **Whatever is playing**, for media, or **The whole system**, for volume.
- **One application**, named by its executable, such as `Player.exe`. The
  editor suggests the executables of the open windows. For volume, this sets
  that application's own volume, as the Windows volume mixer does.

Media actions find most programs by their executable, but a browser or a
Microsoft Store app can report another name. For those, **Whatever is
playing** is the simplest choice. When one application plays several things
at once, such as several browser tabs, **Pause media** pauses all of them.

Volume actions act on your default playback device.

## Key presses

A key press is one key, with any of Ctrl, Alt, Shift, and Win held. The key
can be a letter, a digit, `F1` to `F24`, `Space`, `Enter`, `Tab`, `Escape`,
`Backspace`, `Insert`, `Delete`, `Home`, `End`, `PageUp`, `PageDown`, or an
arrow key: `Up`, `Down`, `Left`, or `Right`.

Type the keys, such as `Ctrl+Shift+M`, or select **Record** and press them.
Recording takes the next keys you press, Tab included. Escape stops
recording, so to press Escape, type `Escape`.

AppyTappy presses keys only into the application the automation watches, and
only while that application's window is in front. If another window is in
front, it presses nothing.

Key presses work only for an application whose anti-cheat is set to **None**,
on the Applications page or when you add or edit the application. Every
application starts at **Not set**, so set it first.
[Anti-cheat](Applications.md#anti-cheat) explains why.

Windows does not let AppyTappy press keys into a program that runs as
administrator, and it gives no sign that it dropped them.

## Opening and sounds

**Open a program, file, or link** takes the full path of a program or a file
on one of your drives, such as `C:\Tools\Timer.exe`, or a web link that
starts with `http://` or `https://`. A file opens in the program it usually
opens in, and a program starts in its own folder. **Play a sound** takes the
full path of a WAV file, such as `C:\Sounds\Ding.wav`. Network paths, which
start with `\\`, are not accepted.

To get a file's full path, right-click it in File Explorer and select
**Copy as path**, then paste it. On Windows 10, hold Shift as you right-click.
The quotes that Copy as path adds are fine.

A program opens without arguments, and a window that opens can take you out of
a full-screen game. If a file is missing, AppyTappy notes it in
[its log](Troubleshooting.md#appytappys-log) and goes on with the next action.
A new sound stops one that is still playing.

Opening and sounds work whatever the application's anti-cheat setting is,
because AppyTappy presses no keys for them. A program that AppyTappy opens can
still press keys of its own, and anti-cheat can treat that as cheating.

An automations file from someone else can open any program or file on your
computer, so check its actions before you use it.

## Clicks

**Click in another application** clicks in the window of a saved application,
such as a scene button in a streaming app when a value in another application
changes. Pick the application, then select **Set up the target**. The
application must be open, because you set up the target on a screenshot of
its window.

On the target screen, take a screenshot when the window shows what to click,
then choose **Where to click**:

- **A spot** clicks the same place every time. Click where the click should
  land. AppyTappy measures the spot from the window corner nearest it, so it
  stays put when the window is resized.
- **A picture** clicks only when the picture is found near where it was. Mark
  the picture, such as a button, with **Box**, **Snap**, or **Click**, then
  select **Where to click on it** and click the place. It starts in the
  middle. With **Snap** or **Click**, only what they select is compared, so
  the background around a round button does not count.

The screenshot zooms, and the picture's box is drawn, moved, and resized, as
[Draw the region](Values.md#draw-the-region) describes.

If the window closes while you set up the target, open it again and select
**Look again**. Looking again clears the screenshots taken so far.

For a picture, **How alike it must look** sets how far its colors may be from
the window's. Raise it if the button changes a little, such as when it is
highlighted; lower it if a similar button could be clicked instead. AppyTappy
looks for the picture up to 50 pixels around where it was, in an area of at
most 400 by 400 pixels, and only while the window has the size it had when
the picture was taken.

**Click** sets the button: **Left**, **Right**, or **Double**, which clicks the
left button twice. **Use this target** keeps the target.

### How to click

**How to click** sets how the click gets to the window:

- **Move the cursor** presses the way a real mouse does. AppyTappy brings the
  application's window to the front for a moment, moves the cursor there,
  clicks, and puts the cursor back. It takes the cursor for a moment, and a
  game in full screen may minimize when another window comes to the front.
- **Send to the window** sends the click straight to the window. AppyTappy
  does not move the cursor or bring the window to the front. What the
  application does with the click is up to it: some ignore a click sent this
  way, and nothing tells AppyTappy whether it worked. Try it before you rely
  on it.

With **Move the cursor**, the action has **Go back to the window that was in
front**. While it is on, the window you were using gets the focus back after
the click. Choosing a right click turns it off, since going back would close
the menu a right click usually opens. A click sent to the window moves no
focus, so it has no such setting.

### Try the target

Under **Try it**, **Find it now** looks for the target on the live window and
shows where the click would land, without clicking. **Click it now** sends one
real click to the window, the way the automation would, so you can see whether
the application takes it. Nothing is clicked until you select **Click it
now**. Afterwards, AppyTappy says that it sent the click and names the window
it went to. It cannot tell whether the application did anything, so look at
the application. If a click sent to the window does nothing, choose **Move
the cursor**.

**Click it now** clicks the window the automation would click: the one the
application's window rules find at that moment. When several of the
application's windows are open, that can be another window than the one the
screen shows. AppyTappy then says so, and **Look again** shows that window.

**Click it now** follows the same rules as the automation's click. It needs
anti-cheat set to **None** for the application it clicks in and for the one
the automation watches, and says so when one is not.

### When a click is skipped

With **Move the cursor**, AppyTappy clicks only when it is safe to:

- only while the watched application or the one it clicks in is the window in
  front;
- only while you hold no mouse button and no Ctrl, Alt, Shift, or Win, so it
  never turns into a drag or a Ctrl-click;
- only when the target's window is the one showing at that place, so it never
  clicks a window that covers it.

With **Send to the window**, none of those matter, because the click reaches
only its window: it is sent whichever window is in front, and even when
another window covers the place. It is skipped when the place is on the
window's title bar, border, or a scroll bar, which a click sent this way
cannot reach, and while the window takes no clicks, such as while it shows a
dialog of its own.

Either way, the window must be open and not minimized, and a click on a
picture happens only when the picture is found. When AppyTappy skips a click,
it notes why in [its log](Troubleshooting.md#appytappys-log). For each click
sent to the window, the log says that it was sent.

Clicks work only when both the watched application and the one clicked in
have anti-cheat set to **None**, on the Applications page or when you add or
edit them ([Anti-cheat](Applications.md#anti-cheat)), because anti-cheat can
see clicks that AppyTappy makes, whichever way they are sent. A click
cannot be undone, so an automation that only clicks needs undo turned off.

Windows does not let AppyTappy click in a program that runs as administrator.
With **Move the cursor**, it gives no sign that it dropped the click. With
**Send to the window**, the click fails, and the log says so.

## Undo and running again

With **Undo these when the condition stops being true** on, AppyTappy puts
things back when the condition stops being true:

- Paused media plays again, and started media pauses again.
- Each volume goes back to what it was.

A skipped track cannot be unskipped, pressed keys cannot be unpressed, what
opened stays open, a played sound cannot be unplayed, and a click cannot be
undone, so an automation needs a pause, a play, or a volume action to undo. With undo on,
AppyTappy also puts things back when the application closes, when you turn
automations off from the [tray icon](The-tray-icon.md), and when AppyTappy
quits. It quits when you sign out or shut down too, and asks Windows to wait
while it puts things back. If that takes more than a few seconds, Windows may
list AppyTappy as **Restoring what automations changed** among the apps holding
up the sign-out or shutdown, and you can go on anyway.

When several automations change the same thing, such as the volume of one
player, the original comes back only once all of them have undone their
changes.

Whether undo is on or not, an automation runs once each time its condition
becomes true. It runs again only after the condition has stopped being true
and then becomes true again. **Wait before it can run again, in seconds**
also keeps it from running again until that many seconds after it last ran.
If its condition became true during the wait and is still true when the wait
ends, it runs then.

## Delete an automation

Select **Delete**, then **Delete automation** to confirm.

## When AppyTappy's background part isn't running

AppyTappy's background part runs every automation, whether AppyTappy Setup is
open or not. While it isn't running, such as after you select **Quit** in the
[tray menu](The-tray-icon.md#the-menu) or when it couldn't start, the
Automations page and the automation editor show a warning at the top that no
automation runs, and how to start it again:

- It starts each time you sign in to Windows while
  **Start AppyTappy when I sign in to Windows** is on in
  [Settings](Settings.md#general).
- To start it now, open `appytappy-runtime.exe` in the folder AppyTappy is
  installed in, `%LOCALAPPDATA%\AppyTappy` unless you picked another folder.

The warning goes away within a few seconds of the background part starting.
Your automations are saved either way, and the background part runs them once
it starts.
