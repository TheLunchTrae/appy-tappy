# Values

A value is something AppyTappy reads from one part of an application's window,
such as a number, how full a bar is, or whether an icon shows. Automations
watch values and act on them.

The **Values** page lists each saved value under its application, with how
it reads and the automations that use it. A green dot marks an application
whose window is open now.

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
   its top-left corner. **Stays at** picks the side of the window the value
   keeps to as the window changes size, as [Window size](#window-size)
   describes. Then select **Next: how to read it**.
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

### Show matching pixels

A **Bar fill** and a **Color** check each have a switch under their colors
that tints the pixels the colors match on the screenshot shown, so you can
see what a color or a tolerance takes in instead of guessing. The tint
follows while you drag **Tolerance**, and when you pick another color or
move the region. While it catches up, the counts under the switch dim and
say **Updating…**. The tint is magenta or green, whichever stands out more
against the color you picked, and the screenshot shows through it. It is
only a guide: it changes neither the screenshot nor the value, and turning
the switch off shows the plain screenshot again. Each switch stays as you
left it until you save or leave the value, so it is still on when you come
back to its step. It needs a screenshot, so while the live view shows, it
asks you to select one.

- **Show matching pixels**, for a bar fill, tints the region's filled pixels
  in one color and its empty pixels in stripes of the other. Under the
  switch, AppyTappy counts the filled pixels, the empty ones, and those that
  are neither. A pixel is neither when it is too far from both colors, such
  as a border or a label, and the reading leaves it out. If parts of the bar
  stay untinted, pick the color again on that part.
- **Show pixels within the tolerance**, for a color check, tints each pixel of
  the region that is within the tolerance on its own, and counts them. The
  value itself does not go by single pixels: it compares the region's average
  color. So under the switch AppyTappy also shows what the value goes by on
  this screenshot: the region's average color, its distance from the
  **Color to look for**, and whether the value is yes. The distance is the
  largest difference in red, green, or blue between the two colors, and the
  value is yes while the distance is at most the tolerance. A region can read
  yes with few pixels tinted, or no with many, when its colors average out.

A [reference](References.md#make-a-reference) has the same switch for its
characters' color.

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
To see what it selects, turn on **Show selected pixels** under
**Color match**. It tints the selected pixels on the screenshot shown and
counts them. The selection is made again each time the slider rests, and the
tint and the count follow then; until they do, the count says **Updating…**.
The tint shows the selection, not what the value compares. While
**Ignore the background** is on, the value compares the tinted pixels that
lie in the region, so if you make the region smaller by hand, the tinted
pixels outside it are not compared. While it is off, the value compares its
whole region, tinted or not, and the selection only fits the region around
the thing.
Selecting on a screenshot with no state yet adds one for it, so you can name
it under **How to read it**. Each state keeps its own selection, made on its
own screenshot, and the region fits around what every state selects.
**Clear selection** on a state compares its whole region again.

While **Ignore the background** is on, the pixels a state's selection leaves
out are shaded over its screenshot, once **Color match** rests, and the
check skips them. Turn it off to
compare the whole region again. You can still adjust the region by hand with
**Box**; anything you add to it counts as background.

The **Number** check cannot read rotated text, characters that touch each
other, or text on a background of the same color. For those, a **Color** or
**State** check on the part that matters often works instead.

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

**Stays at** picks one of nine sides and corners of the window, on a 3 by 3
grid. At first it is the third of the window the region's center is in, so a
health bar in the bottom left stays at the bottom left. A value pinned to the
right keeps its distance from the right edge as the window gets wider, and
one pinned to the center keeps its distance from the middle. The Values page
shows each value's size and side under **Region**, such as
**Drawn at 1920 × 1080, pinned bottom left**.

### Fix a value at one size

While the application runs, the Values page adds a column for the window's
size now, such as **At 3440 × 1440 now**, with what each value reads there.
Each value has **Fix at this size** beside it there, unless the window is at
the size the value was first drawn at. At that size, a value that reads wrong
is deleted and made again. **Matches no state** isn't always wrong: the
region may show none of the states right now.

1. AppyTappy finds the application's window and names it; select
   **Capture this window**. If it has none to capture, see
   [When the window isn't found](#when-the-window-isnt-found). AppyTappy
   takes a screenshot of the window at its size now, with the region where
   the value would be read there: it **Starts from the scaled region**.
2. Move the region onto the value, then select **Next: how to read it**.
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
and make any that read wrong again. Then select **Mark as checked**, which
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
