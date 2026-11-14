# The tray icon

AppyTappy's background part shows an icon in the taskbar's notification area,
next to the clock. If you don't see it, select the arrow that shows hidden
icons. The icon is the same as AppyTappy Setup's.

Point at the icon to see what AppyTappy is doing:

- **AppyTappy: idle.** None of the applications your automations watch is
  running.
- **AppyTappy: attached.** At least one of them is running, and AppyTappy is
  watching it.
- **AppyTappy: degraded.** AppyTappy cannot check which programs are running.
  It keeps trying, and recovers by itself once Windows answers again.
- **AppyTappy: disabled.** You turned automations off from the menu.

If AppyTappy cannot start the audio controls in Windows, no action can run,
and the tooltip adds **actions cannot run yet**, as in
**AppyTappy: attached, actions cannot run yet**. Your automations still watch
their applications, but their actions do nothing. AppyTappy keeps trying, and
the text goes away by itself once actions can run. While automations are off,
the tooltip says only **AppyTappy: disabled**, and the menu still shows the
warning.

## The menu

Right-click the icon to open its menu. While actions cannot run, a grayed
**Actions cannot run yet** line comes first.

- **Enabled** turns all automations off and on. Turning them off undoes what
  they changed, such as a paused song or a lowered volume. AppyTappy starts
  with automations on each time it starts, unless **Remember whether
  automations are on** is on in [Settings](Settings.md#general); then it
  starts with them as you last left them here.
- **Active automations** lists, by name, the automations that run now: those
  of the applications that are running, while automations are on. With none,
  it shows **None**.
- **Quit** closes the background part, after it undoes what your automations
  changed. It starts again the next time you sign in.

To start it again without signing out, see
[When AppyTappy's background part isn't running](Automations.md#when-appytappys-background-part-isnt-running).

You don't need to restart AppyTappy after a change in AppyTappy Setup. Each
change takes effect at once.
