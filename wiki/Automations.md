# Automations

An automation watches one [value](Values.md) and runs its actions, in order,
when a condition on that value becomes true. For example: when **Progress**
is at least 90, pause the music.

An automation is separate from its value, so any number of automations can
watch the same value.

The **Automations** page lists each automation with a switch that turns it on
or off, its condition, its actions, and the application that must be running
for it to run. If an automation can no longer run, the reason shows under its
name.

## Make an automation

On the Automations page, select **New automation**. To change one, select
**Edit**.

1. **Name** it, such as `Pause music near the end`.
2. **When.** Pick the value, then how it compares:
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
   what is still missing.

Saving, deleting, or switching an automation takes effect at once, and so
does any other change that affects an application's automations, such as
changing its anti-cheat setting. A change to what runs, such as an
automation's condition or actions, or a value's region, starts all of that
application's automations over: they undo what they changed first, and one
whose condition still holds runs again. Renaming an application, a value or an
automation starts nothing over.

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

Key presses work only for an application whose anti-cheat is set to **None**
on the Applications page. Every application starts at **Not set**, so set it
first. [Anti-cheat](Applications.md#anti-cheat) explains why.

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
left button twice. **Find it now** looks for the target on the live window and
shows where the click would land, without clicking. **Use this target** keeps
it.

When the click runs, AppyTappy brings the application's window to the front
for a moment, moves the cursor there, clicks, and puts the cursor back. With
**Go back to the window that was in front** on, the window you were using gets
the focus back. Choosing a right click turns it off, since going back would close the
menu a right click usually opens.

AppyTappy clicks only when it is safe to:

- only while the watched application or the one it clicks in is the window in
  front;
- only while you hold no mouse button and no Ctrl, Alt, Shift, or Win, so it
  never turns into a drag or a Ctrl-click;
- only when the target's window is the one showing at that place, so it never
  clicks a window that covers it;
- for a picture, only when the picture is found.

Otherwise it skips the click and notes why in
[its log](Troubleshooting.md#appytappys-log).

Clicks work only when both the watched application and the one clicked in
have anti-cheat set to **None** on the Applications page, because anti-cheat
can see clicks that AppyTappy makes. A click cannot be undone, so turn undo
off for an automation that clicks.

Windows does not let AppyTappy click in a program that runs as administrator,
and it gives no sign that it dropped the click.

## Undo and running again

With **Undo these when the condition stops being true** on, AppyTappy puts
things back when the condition stops being true:

- Paused media plays again, and started media pauses again.
- Each volume goes back to what it was.

A skipped track cannot be unskipped, pressed keys cannot be unpressed, what
opened stays open, and a played sound cannot be unplayed, so an automation
needs a pause, a play, or a volume action to undo. With undo on,
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
