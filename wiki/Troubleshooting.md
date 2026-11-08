# Troubleshooting

## AppyTappy's log

The background part writes what it does to `runtime.log`, one line for each
start, stop, change, and problem. To open it, paste
`%LOCALAPPDATA%\AppyTappy\logs` into File Explorer's address bar. The log
names each automation that cannot run, and why.

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
   see what AppyTappy reads. A value reads only while the window is the [size it
   was made at](Values.md#window-size). If the window has a size no value was
   made at, the log says so. A value also reads only while its window shows:
   while the window is minimized, AppyTappy waits, and reads the values again
   when it shows.
5. **Did it already run?** An automation runs once each time its condition
   becomes true. It runs again only after the condition has stopped being
   true, and after its wait.

## A number reads wrong

[Edit its reference](References.md#make-a-reference) and label more screenshots,
or record more readings, so that every digit the number can show appears on at
least one of them, and check the characters' color. Some text the **Number**
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
