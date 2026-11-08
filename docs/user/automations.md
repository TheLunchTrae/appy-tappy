# Automations

An automation watches one [value](values.md) and runs its actions, in order,
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

   An automation watches one value.

   While the value's application is open, the editor reads the value about
   twice a second and says what it reads now and whether the condition is
   true, so you can try a target before you save it. It says so when the
   application is not open, or when its window is not the
   [size the value was made at](values.md#window-size).

3. **Do, in order.** Add any number of [actions](#actions), and move them up
   or down.
4. **Afterwards.** Choose whether AppyTappy undoes the actions when the
   condition stops being true, and how long to wait before the automation can
   run again. See [Undo and running again](#undo-and-running-again).
5. **Save automation.** Until the automation is complete, the editor says
   what is still missing.

Saving, deleting, or switching an automation takes effect at once, and so
does any other change that affects an application's automations, such as
changing its anti-cheat setting. Such a change starts all of that
application's automations over: they undo what they changed first, and one
whose condition still holds runs again.

## Actions

- **Pause media**, **Play media**, **Next track**, and **Previous track**
  control music or video the way your keyboard's media keys do, so they work
  with most music players and browsers.
- **Set volume** sets a volume, from 0 to 100 percent.
- **Press keys** presses keys such as `Ctrl+Shift+M`. See
  [Key presses](#key-presses).

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
first. [Anti-cheat](applications.md#anti-cheat) explains why.

Windows does not let AppyTappy press keys into a program that runs as
administrator, and it gives no sign that it dropped them.

## Undo and running again

With **Undo these when the condition stops being true** on, AppyTappy puts
things back when the condition stops being true:

- Paused media plays again, and started media pauses again.
- Each volume goes back to what it was.

A skipped track cannot be unskipped, and pressed keys cannot be unpressed, so
an automation that only skips tracks or presses keys cannot undo. With undo
on, AppyTappy also puts things back when the application closes, when you
turn automations off from the [tray icon](tray.md), and when AppyTappy quits.
It quits when you sign out or shut down too, and asks Windows to wait while
it puts things back. If that takes more than a few seconds, Windows may list
AppyTappy as **Restoring what automations changed** among the apps holding up
the sign-out or shutdown, and you can go on anyway.

When several automations change the same thing, such as the volume of one
player, the original comes back only once all of them have undone their
changes.

Whether undo is on or not, an automation runs once each time its condition
becomes true. It runs again only after the condition has stopped being true
and then becomes true again. **Wait before it can run again, in seconds**
also keeps it from running again until that many seconds after it last ran.
If its condition became true during the wait, it runs the next time the value
changes and still makes the condition true.

## Delete an automation

Select **Delete**, then **Delete automation** to confirm.
