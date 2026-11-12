# Values

A value is something AppyTappy reads from one part of an application's window,
such as a number, how full a bar is, or whether an icon shows. Automations
watch values and act on them.

The **Values** page lists each saved value under its application, with how
it reads and the automations that use it. A green dot marks an application
whose window is open now. **Edit** and **Delete** sit at the end of each
value's row.

## Make a value

On the Values page, select **New value**. Or select **New value** while you
make an automation: it starts with the automation's
[application](Automations.md#what-to-watch), and saving the value brings you
back to the automation with the value picked.

1. **Choose the application.** Select the application the value belongs to
   in the list, which says whether each one is running. If it isn't in the
   list yet, select **Add application** to
   [add it](Applications.md#add-change-or-remove-an-application); saving or
   canceling it brings you back here. AppyTappy then finds the application's
   window the way it does when it runs your automations, and names it. Select
   **Capture this window**. [When the window isn't found](#when-the-window-isnt-found)
   says what to do when it has no window to capture.
2. **Take screenshots.** Select **Take screenshot**, or press the screenshot
   shortcut, `F9` at first. The shortcut works while the other application
   has the keyboard, so you can carry on in it until it shows what you want
   and press `F9` then. To use other keys, select **Change shortcut** and
   press them, or change it in [Settings](Settings.md). F1 to F24, Print Screen,
   and Pause work alone. A letter, a digit, Space, an arrow, Insert, Delete,
   Home, End, Page Up, or Page Down needs Ctrl, Alt, Shift, or Win with it, so
   it keeps working everywhere else. Other keys can't be the shortcut.
3. **Where to read.** Type a **Value name**, such as `Progress`, and choose
   the **Check**, one of the four [checks](#checks) below. On a screenshot,
   drag to draw a box around what to read. Drag inside the box to move it, or
   drag a corner to resize it. The **X**, **Y**, **Width**, and **Height**
   fields set it to the exact pixel, counted in the window's own pixels from
   its top-left corner. Turn on **This content may move** if the value shows
   in different places inside the window, as
   [Content that moves](#content-that-moves) describes. **Stays at** says
   which side of the window the region keeps to as the window changes size,
   as [Window size](#window-size) describes; it follows the region, so you
   can leave it alone. Then select **Next: how to read it**.
4. **How to read it.** Set up the check, as [Checks](#checks) describes. The
   box under it then shows what AppyTappy reads on the live window, a few
   times a second. If the check cannot work, such as a bar whose filled and
   empty colors are the same, it says why there.
5. **Save.** Select **Save value**. **Back** returns to the first step.

AppyTappy keeps the screenshots until you select **Change window** or leave
the new value. Before either drops work you have not saved, it asks first:
select **Keep this window** or **Stay** to carry on, as
[Leave before you save](Getting-started.md#leave-before-you-save) describes.
**Change window** returns to the first step with the same application chosen.

### When the window isn't found

AppyTappy captures only a window it would read for the application, so what you
draw is what your automations see.

- **The application isn't open, or its window doesn't match.** Open the
  application and select **Refresh**. If it is open, the way AppyTappy finds
  its window no longer fits, such as after its title changed: select
  **Change how it's found** to
  [change it](Applications.md#how-appytappy-finds-the-window), and saving or
  canceling brings you back.
- **Several windows fit equally well.** AppyTappy reads one of them, as
  [How AppyTappy finds the window](Applications.md#how-appytappy-finds-the-window)
  describes, so it lists them and asks which to capture. Each shows its size,
  window class, and executable, and **Read if it starts now** marks the one
  AppyTappy would read if the application started now. Pick one, or select
  **Change how it's found** so that only one fits.
- **The window is the fallback.** AppyTappy says so when no open window
  matches everything that must match and it reads the
  [fallback](Applications.md#how-appytappy-finds-the-window) instead. You can
  capture it; check that it is the window you mean.
- **The applications couldn't be read.** AppyTappy says why. Select
  **Refresh** to read them again.

## Checks

- **Number** reads a number, such as `87%` or `-12.5`, with a
  [reference](References.md): the color and shapes of the application's
  characters. Pick one of the application's references, or select
  **New reference from this region** to make one from the region you drew.
  **Edit** changes the picked one, such as to teach it a digit it does not
  know yet. Turn on **Whole number only** to ignore every digit after the
  decimal point, so `87.5%` reads as 87. The reference still has to know the
  point, so teach it on a screenshot that shows one.
  To read numbers with a thousands separator, such as `1,234`, teach the
  reference the `,` too. A `.` is always the decimal point, and a `,` only
  separates thousands: it counts only between groups of three digits, so
  `12,5` reads as 12. A number written with its point first, such as `.5`,
  reads as 0.5, and a `.` with no digit after it, such as `...`, is not a
  number.
- **Bar fill** reads how full a bar is, from 0 to 100. Use **Pick** next to
  **Filled color** and click a filled part of the bar, then next to
  **Empty color** and click an empty part. Borders and labels inside the
  region don't count.
- **Color** is yes when the region's average color is close to the
  **Color to look for**, such as a light that turns red. Use **Pick** and
  click the color on a screenshot. **Tolerance** says how close: drag the
  slider or type a number.
- **State** reads which of a few named looks the region shows, such as a
  corner that says `PAUSED` or `LIVE`. Take a screenshot of each look. Select
  one, then **Add a state from this screenshot** under **States**, and name
  the state, such as `Paused`. A screenshot gives at most one state, and a
  value can have up to 16. The value reads as the state whose screenshot the
  region looks most like, or `none` when it is like none of them, such as
  while the corner is empty. **Tolerance** says how much, on average, the
  region may differ from a state's screenshot and still read as it. Each
  state shows how many pixels it compares, and **matches now** while the
  live window reads as it. To compare only the thing itself and not what is
  behind it, [select it](#select-the-thing-in-a-state).

State names are up to 32 characters, with no space at either end. Two states
cannot share a name, ignoring case, and none can be named `none`.

## Select the thing in a state

A **State** check compares every pixel of its region, so a background that
changes, such as a sky behind an icon or a scene behind a word, can stop a
state from matching. **Select with**, under **Where to read**, marks just the
thing on the screenshot shown instead:

- **Box** draws the region by hand, as for other checks.
- **Snap** takes a rough box. Drag one around the thing with some plain
  background on every side, and it shrinks to fit. The background around the
  thing and between its parts, such as inside the letter O, is left out.
- **Click** takes the thing under the pointer. Click it, and the selection
  spreads over the colors like the one you clicked and fills any holes, such
  as a star inside a shield.

Hold **Shift** while you drag or click to add a part, or **Alt** to take one
away. **Color match** says how close a color must be to count as the same:
raise it if the selection stops short, and lower it if it spills over.
Selecting on a screenshot with no state yet adds one for it, so you can name
it under **How to read it**. Each state keeps its own selection, made on its
own screenshot, and the region fits around what every state selects.
**Clear selection** on a state compares its whole region again.

While **Ignore the background** is on, the pixels a state's selection leaves
out are shaded over its screenshot, and the check skips them. Turn it off to
compare the whole region again. You can still adjust the region by hand with
**Box**; anything you add to it counts as background.

The **Number** check cannot read rotated text, characters that touch each
other, or text on a background of the same color. For those, a **Color** or
**State** check on the part that matters often works instead.

## Content that moves

AppyTappy reads the same part of the window every time. It doesn't look for a
value or follow it. If what you want to read shows in different places inside
the window, such as a number that shifts sideways as it gains digits, turn on
**This content may move** under **Where to read**. What it can do depends on
the check:

- For a **Number** or a **Bar fill** check, draw the box around everywhere
  the value can show. The box is then a reading area, drawn with a dashed
  outline, and AppyTappy reads the number or the bar wherever it is inside
  it. A **Number** check reads everything in the area that has the
  characters' color, so keep other text and shapes of that color out. A
  **Bar fill** check counts everything in the area with a color near the
  bar's filled or empty color, so keep anything else of those colors out.
  Neither looks outside the area. The value keeps that its region is a
  reading area: the Values page shows it under **Region**, such as
  **Reading area drawn at 1920 × 1080, pinned top right**, and a
  [fix](#fix-a-value-at-one-size) of the value asks for a reading area
  again.
- A **Color** check averages its whole region, and a **State** check
  compares its region pixel by pixel with each state's screenshot. Both need
  what they read to stay in the same place, so the option only says so, and
  the value is saved with a plain region. Make a value for each place it can
  show instead.

This is separate from the window changing size, which
[Window size](#window-size) covers.

## Window size

A value is drawn at the size the window was on the screenshot you drew its
region on. Each state of a **State** check comes from a screenshot of that
same size, and trying or saving it says so if one is not. At any other size,
what AppyTappy reads depends on how often the application's window
[changes size](Applications.md#how-often-the-window-changes-size):

- While it changes size **often**, the value scales with the window. Its
  region grows or shrinks by how much taller or shorter the window is, and
  keeps its scaled distance from the side it stays at. A **State** check's
  pictures scale with it: shrinking averages the pixels each new pixel
  covers, and growing repeats the nearest one, so sharp edges stay sharp. A
  value whose region would reach past the window's edge isn't read.
- While it changes size **rarely**, the value is read only at the sizes it
  was drawn at.

**Stays at** is the side or corner of the window, or its center, that the
region keeps its scaled distance from. A value that stays at the right keeps
its distance from the right edge as the window gets wider, and one that stays
at the center keeps its distance from the middle. Under the control, a
sentence says what the region keeps its distance from, and dashed lines on
the screenshot run from the region to those edges, or to the middle of the
window. The lines show which edges the region is measured from. They don't
show where the region lands at another window size.

AppyTappy picks the side for you: the third of the window the region's center
is in, so a health bar in the bottom left stays at the bottom left. The
control shows it, such as **Top right (automatic)**, and it follows the
region each time you draw, move, or resize it, or type its pixels.

To choose the side yourself, select **Change** and pick one of the nine
positions on the grid. The control then shows it as chosen, such as
**Top right (chosen by hand)**, and keeps it while you move or resize the
region. If the region ends up nearest another side, the control says so, such
as **Chosen by hand: top right. The region is now nearest the bottom left.**
Select **Back to automatic** to follow the region again. Drawing a new region
goes back to automatic too.

While the application's window changes size **rarely**, the side isn't used.
The Values page shows each value's size and side under **Region**, such as
**Drawn at 1920 × 1080, pinned bottom left**.

### Fix a value at one size

While the application runs, the Values page adds a column for the window's
size now, such as **At 3440 × 1440 now**, with what each value reads there.
Each value has **Fix at this size** beside it there, unless the window is at
the size the value was first drawn at. At that size, [edit the
value](#edit-a-value) instead. **Matches no state** isn't always wrong: the
region may show none of the states right now.

1. AppyTappy finds the application's window and names it; select
   **Capture this window**. If it has none to capture, see
   [When the window isn't found](#when-the-window-isnt-found). AppyTappy
   takes a screenshot of the window at its size now, with the region where
   the value would be read there: it **Starts from the scaled region**.
2. Move the region onto the value, then select **Next: how to read it**.
   For a value drawn as a [reading area](#content-that-moves), the region
   **Starts from the scaled reading area**, with a dashed outline: move or
   resize it so it covers everywhere the value can show at this size,
   rather than fitting it to the value.
3. For a **State** check, take a screenshot of each of its states at this
   size and add them in the same order; each takes the next state's name.
   The fix compares with the first drawing's tolerance.
   For a **Number** check, **Read with** keeps the reference the value reads
   with at every size, or **Learn a reference at this size** teaches a new
   one from this size's screenshots. A **Bar fill** or **Color** check keeps
   its colors.
4. Select **Save fix**.

At that size AppyTappy then reads the fix exactly, instead of scaling. The
**Region** column lists each fix, such as **· fixed at 3440 × 1440**. A value
can have up to 8 fixes. Fixing it again at the same size replaces the fix,
and **Remove fix** beside a reading made with one removes it.

While you resize the window, AppyTappy waits until the window keeps one size
for a quarter of a second before it reads at that size. While the window is
being resized, is closed or is at a size where no value reads, AppyTappy
reads nothing. Automations neither run nor undo their changes until it can
read again. [AppyTappy's log](Troubleshooting.md#appytappys-log) says once
which values it doesn't read at the window's size and why, and says it again
only when the reason changes.

## Edit a value

Select **Edit** next to a value to change it without making it again. The
value stays the same value, whatever you change, its name included, so the
automations that use it keep using it.

An edit changes the value as it was first drawn, which every window size
reads or scales from. A [fix](#fix-a-value-at-one-size) leaves that alone
and draws the value again at one other size. The editor is headed
**Edit** with the value's name and saves with **Save changes**; a fix is
headed **Fix** and saves with **Save fix**.

1. AppyTappy finds the application's window and names it; select
   **Capture this window**. If it has none to capture, see
   [When the window isn't found](#when-the-window-isnt-found). AppyTappy
   takes a screenshot of the window and opens the value as it is saved. The
   application has to be running, as for a fix, whatever you change.
2. **Where to read.** Change the **Value name**, the **Check**,
   **This content may move**, or **Stays at**, or move the region.
   **Stays at** opens as automatic unless the value was saved with another
   side than the one its region is nearest, and
   **This content may move** opens on for a value drawn as a reading area.
   A **Color** or **State** check cannot have a reading area, so changing
   the check to one of them turns it off.
3. **How to read it.** Change the reference, **Whole number only**, the
   colors, the states' names, or the **Tolerance**. The box under them shows
   what the value would read on the live window once saved.
4. Select **Save changes**. **Discard**, or leaving the editor, keeps the
   value as it was saved. Leaving asks first only once you changed
   something.

### The window's size while you edit

A value's region is in the pixels of the window size it was drawn at. While
the window has that size, the region starts exactly where it was saved, and
you can move it.

At any other size, the editor says so, such as **Progress was drawn at
1920 × 1080, and the window is 2560 × 1440 on this screenshot**, and shows no
region on that screenshot. The region and a **State** check's pictures stay
as they are saved; everything else can still change. To change them, do one
of these:

- Resize the window to the size the value was drawn at and take a
  screenshot. The region then shows on it.
- Select **Discard**, then **Fix at this size** on the Values page, to
  change how the value reads at the window's size now only.

The box under **How to read it** shows what the edited value reads at the
window's size now: scaled, at its fix, or
**Not read at 2560 × 1440, the window's size now**, as
[Window size](#window-size) describes.

### States in an edit

A **State** check keeps the picture saved for each state while the region
stays where it was. **States** then lists each state with
**Saved picture**: rename a state there, or change the **Tolerance**.

To change a state's picture, or to add or remove a state, take every state
again. Select a screenshot that shows the first state and select
**Take the states again**, then add the others as for a new value; each
takes the next state's name. Every screenshot has to be at the size the
value was drawn at. Moving the region needs the same, since the saved
pictures no longer fit it: the editor says
**The region changed, so each state needs a new screenshot**, and saves once
each state has one. Removing every state you took brings the saved pictures
back, while the region is where it was saved.

### What an edit will not do silently

- **Automations.** An edit that would stop an automation is not saved: one
  that makes the value read another kind of thing, such as yes or no instead
  of a number, or that renames or removes a state an automation's condition
  names. The editor names those automations as soon as the change is made.
  Change or delete them first, then edit the value.
- **Fixes.** A fix reads the value the way it was first drawn, so the value's
  fixes stay while the check keeps its kind and a **State** check keeps its
  states in order: renamed with their saved pictures, or taken again under
  the names they had. Any other change leaves the fixes unusable. The editor
  then lists them, such as
  **Saving this removes its fix at 3440 × 1440**, and saves only once you
  turn on **Remove its fix**. If the fixes it would remove are not the ones
  you agreed to lose, the save is refused and lists them again.
- **What a fix reads with.** A fix that stays keeps its region, its
  pictures, and the reference it reads with. When you pick another reference
  for a **Number** value, a fix that read with the old reference keeps
  reading with it, so the old reference still cannot be deleted while that
  fix uses it. One thing does follow the edit: **Whole number only** is the
  value's setting at every size, so changing it changes how its fixes read
  too.

## When an application updates

An update can move what a value reads, so AppyTappy remembers which build of
the application its values were checked against: the version of its
executable, such as `1.4.2.0`, its size, and when it was written. It records
the build that runs when you save the application's first value. Deleting every value, or changing which executable
the application is, forgets it. The Values page shows it next to the
application's name, such as **checked against 1.4.2.0 on Nov 8, 2026**.

When the application runs a different build, the Values page says so under the
application's name, such as **Changed since its values were checked (1.4.2.0
then, 1.5.0.0 now)**. AppyTappy still reads the values. Check what each one
reads now, such as with the application's [overlay](Applications.md#overlay),
and [edit](#edit-a-value) any that read wrong. Then select **Mark as checked**, which
records the build that runs now. The application has to be running for that.
While it runs and no build is recorded, such as for values made before AppyTappy
recorded builds, the Values page offers **Mark as checked** too.

[AppyTappy's log](Troubleshooting.md#appytappys-log) says so as well, each time
the application starts while it has an automation that can run. AppyTappy finds
the executable without opening the application, so this works whatever the
application's [anti-cheat](Applications.md#anti-cheat) setting.

## Delete a value

Select **Delete** next to a value, then confirm. A value that an automation
watches cannot be deleted: the Values page names the automations to change or
delete first.
