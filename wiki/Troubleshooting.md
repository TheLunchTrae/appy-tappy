# Troubleshooting

## AppyTappy's log

The background part writes what it does to `runtime.log`, one line for each
start, stop, change, and problem. To open it, paste
`%LOCALAPPDATA%\AppyTappy\logs` into File Explorer's address bar. The log
names each automation that cannot run, and why.

With **Show debug details** on in [Settings](Settings.md#debug), the
[Debug page](Debug-page.md) shows the end of the log, and AppyTappy Setup's own
log beside it. Its **Export debug bundle** saves the logs, your settings, and
the newest trace in one zip file to share with a developer.

## Windows warns about the installer

AppyTappy is not signed, so Windows warns the first time you run the installer.
[Install](Getting-started.md#install) says what to select.

## I can't find the tray icon

Select the arrow next to the clock that shows hidden icons. If AppyTappy is not
there, it is not running: you quit it from the menu, or it could not start.
Start it again as [The tray icon](The-tray-icon.md#the-menu) describes. If it
still doesn't appear, check the log.

## An automation doesn't run

Check these in order:

1. **Is it on?** Its switch on the Automations page is on, and **Enabled** is
   checked in the [tray menu](The-tray-icon.md#the-menu).
2. **Can it run?** If it cannot, the Automations page says why under its name.
3. **Is the application running?** Point at the tray icon: it says
   **AppyTappy: attached** while one of your applications runs. The tray's
   **Active automations** lists the automations that run now.
4. **Does the value read?** Turn on the [overlay](Applications.md#overlay) to
   see what AppyTappy reads, or look at the Values page while the application
   runs. At a size the value wasn't drawn at, it reads as
   [Window size](Values.md#window-size) describes, and the log says why a
   value doesn't read there. A value also reads only while its window shows:
   while the window is minimized, AppyTappy waits, and reads the values again
   when it shows.
5. **Did it already run?** An automation runs once each time its condition
   becomes true. It runs again only after the condition has stopped being
   true, and after its wait.

If the log says the actions thread panicked, AppyTappy has a bug. It starts
running actions again with the next one, but a change made before that, such
as a volume change, isn't put back when its automation stops. Include the log
line when you report the problem.

If the log says an application's reader ended and that a reader thread
panicked, AppyTappy has a bug too. It reads the application's values again
after a wait, which starts at 1 second and gets longer, up to 30 seconds,
each time it happens again soon after. Its automations stay as
they were meanwhile. Include both log lines when you report the problem.

## A number reads in setup but the automation does nothing

What you see while you make a reference isn't all the reference's reading.
**Characters it knows** lists the characters your boxes and kept readings
show, and the text in a recording's review comes from a separate text reader
that fills in each reading's text.
Only the box that shows a reading **on the live window** is what the
reference reads, and the automation reads the same way.

Check these in order:

1. **See what AppyTappy reads while the application runs.** Turn on the
   application's [overlay](Applications.md#overlay), or look at the Values
   page. Each value shows its reading, or why it has none. The
   [log](#appytappys-log) says the same in a line such as
   `Example App: Progress cannot be read: …`, once for each reason until the
   value reads again.
2. **Find the reason below.** They are in the order AppyTappy reads a number,
   so the first one that applies is the one to fix.
3. **If the value reads right,** the automation is what to check:
   [An automation doesn't run](#an-automation-doesnt-run). A condition that a
   number **is** a target is true only while the value reads exactly that
   number, and a percentage with decimals can pass 50 between two reads. Use
   **is at least** or **is at most** instead.

The reasons a number has no reading:

- **It is not read at the window's size now.** The window isn't the size it
  was when you drew the value, and windowed and fullscreen are different
  sizes. [Window size](Values.md#window-size) says how to resize the window
  or fix the value at this size.
- **No pixel in the region has the characters' color.** The region isn't on
  the number now, or the number is another color than the reference's. When
  the log line says **where its region scales to**, the window isn't at a
  size the value was drawn at, and the scaled region can lie beside the
  number: [fix the value at this size](Values.md#fix-a-value-at-one-size). Otherwise
  [edit the reference](References.md#make-a-reference): pick the color on the
  middle of a character, or raise **Tolerance**.
- **The region holds more than 24 characters.** Something else in the
  characters' color is in the region. Draw the region around the number
  alone, or lower **Tolerance**.
- **A character has a shape the reference has not learned.** The message
  shows what it read, with a `?` for each character it couldn't, such as
  `8 7 ?`, and the character it learned that the first `?` looks most like.
  If that is the character the application shows there, the application drew
  it a little differently this time: draw a [box](References.md#boxes) around
  this reading on a screenshot, or [record](References.md#record-readings)
  the region until it reads. If it is another character, the reference hasn't
  learned the one shown: draw a box around a reading with it.
  [What a reference can't learn](References.md#what-a-reference-cant-learn)
  lists the characters no box teaches.
- **The characters hold no number.** The message shows what it read, such as
  `%`. The region shows no digit now, or the region cuts the digits off.

If the reason isn't one of these, or the number reads on the live window in
setup at the same window size and not in the overlay, that is a fault in
AppyTappy. Report it with the log line, the application's folder in
`%APPDATA%\AppyTappy\apps`, which holds the reference's pictures, and a
screenshot of the window at the size you play at.

## A number reads wrong

[Edit its reference](References.md#make-a-reference) and draw more
[boxes](References.md#boxes), or record more readings, so that every digit
the number can show appears in at least one of them, and check the
characters' color.
[**Characters still missing**](References.md#choose-what-it-needs) lists what
is left to teach. If it reads a digit as a letter that looks like it, such as
`0` as `O`, set the value's
[**Characters it reads**](Values.md#characters-a-number-reads) to the
characters it shows. Some text the **Number**
check cannot read; [Checks](Values.md#checks) says what it can't, and what to
use instead.

## A value read right before an update

When the application updates, the Values page says its values were checked
against another build. [When an application updates](Values.md#when-an-application-updates)
says what to do.

## A key press does nothing

- The application's anti-cheat must be **None** on the Applications page. See
  [Anti-cheat](Applications.md#anti-cheat).
- AppyTappy presses keys only while the watched application's window is in
  front. The log says when it skipped a press for that reason.
- Windows drops keys sent to a program that runs as administrator.

## A click does nothing

- Both the watched application and the one clicked in must have anti-cheat
  set to **None** on the Applications page. See
  [Anti-cheat](Applications.md#anti-cheat).
- The log says why AppyTappy skipped a click, such as a window that is
  minimized, a picture that was not found, or another window in front.
- With **Send to the window**, the log says each time that the click was sent.
  If the application still did nothing, it ignores clicks sent this way:
  change the click to **Move the cursor**. See
  [How to click](Automations.md#how-to-click).
- Windows does not let AppyTappy click in a program that runs as
  administrator.

## The overlay doesn't show over a game

The game probably runs in exclusive fullscreen, which draws over every other
window. Switch it to borderless or windowed fullscreen, or use
**Picture in picture** on another monitor. See
[Fullscreen](Applications.md#fullscreen).

## Media or volume actions do nothing

- Check that something is playing, and that **One application** names the
  executable that plays it. A browser or a Microsoft Store app can report
  another name than its executable; try **Whatever is playing** instead.
- Volume actions change only the default playback device.
- The log names each action that failed, and why.
- Right after you sign in to Windows, the media controls may not be ready
  yet. Media actions then fail, and the log names each one, while volume and
  key press actions still run. AppyTappy asks for the media controls again,
  waiting longer between tries.
- If the tray icon says **actions cannot run yet**, AppyTappy could not start
  the audio controls in Windows, so no action runs. It keeps trying, and the
  text goes away once actions can run.
- When putting the volume back fails, for example because your headset was
  unplugged, AppyTappy keeps trying and puts it back once the headset returns.
