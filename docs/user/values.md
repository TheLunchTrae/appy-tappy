# Values

A value is something AppyTappy reads from one part of an application's window,
such as a number, how full a bar is, or whether an icon shows. Automations
watch values and act on them.

The **Values** page lists each saved value under its application, with how
it reads and the automations that use it. A green dot marks an application
whose window is open now.

## Make a value

On the Values page, select **New value**.

1. **Pick the window.** Choose the application in **Save to**, then
   pick its window in the list of open windows; type in the filter to find
   one. Picking a window a saved application finds chooses that application.
   If the application isn't saved yet, select **Add application** to
   [add it](applications.md#add-change-or-remove-an-application) first. Then
   select **Capture this window**.
2. **Take screenshots.** Select **Take screenshot**, or press the screenshot
   shortcut, `F9` at first. The shortcut works while the other application
   has the keyboard, so you can carry on in it until it shows what you want
   and press `F9` then. To use other keys, select **Change shortcut** and
   press them. A letter, a digit, or Space needs Ctrl, Alt, Shift, or Win
   with it, so the key keeps working everywhere else.
3. **Where to read.** Type a **Value name**, such as `Progress`, and choose
   the **Check**, one of the four [checks](#checks) below. On a screenshot,
   drag to draw a box around what to read. Drag inside the box to move it, or
   drag a corner to resize it. The **X**, **Y**, **Width**, and **Height**
   fields set it to the exact pixel, counted in the window's own pixels from
   its top-left corner. Then select **Next: how to read it**.
4. **How to read it.** Set up the check, as [Checks](#checks) describes. The
   box under it then shows what AppyTappy reads on the live window, a few
   times a second. If the check cannot work, such as a bar whose filled and
   empty colors are the same, it says why there.
5. **Save.** Select **Save value**. **Back** returns to the first step.

AppyTappy keeps the screenshots until you select **Change window** or leave
the new value. Before either drops work you have not saved, it asks first:
select **Keep this window** or **Stay** to carry on, as
[Leave before you save](README.md#leave-before-you-save) describes.

## Checks

- **Number** reads a number, such as `87%` or `-12.5`, with a
  [reference](references.md): the color and shapes of the application's
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
- **Image** is yes when the region looks like it does on a screenshot you
  choose, such as an icon that appears. Select that screenshot, then
  **Use this one** next to **Compare against**. **Tolerance** says how much
  it may differ on average. To compare only the thing itself and not what is
  behind it, [select it](#select-the-thing-in-an-image).

## Select the thing in an image

An **Image** check compares every pixel of its region, so a background that
changes, such as a sky behind an icon or a scene behind a word, can stop it
from matching. **Select with**, under **Where to read**, marks just the thing
instead:

- **Box** draws the region by hand, as for other checks.
- **Snap** takes a rough box. Drag one around the thing with some plain
  background on every side, and it shrinks to fit. The background around the
  thing and between its parts, such as inside the letter O, is left out.
- **Click** takes the thing under the pointer. Click it, and the selection
  spreads over the colors like the one you clicked and fills any holes, such
  as a star inside a shield.

Hold **Shift** while you drag or click to add a part, or **Alt** to take one
away. **Color match** says how close a color must be to count as the same:
raise it if the selection stops short, and lower it if it spills over. The
region fits the selection, and the screenshot you select on becomes the one
the check compares against. Selecting on another screenshot, or choosing
another with **Use this one**, starts the selection over.

Under **How to read it**, while **Ignore the background** is on, the pixels left out are shaded over the
screenshot, and the check skips them. The note under it says how many pixels
it compares. Turn it off to compare the whole region again. You can still
adjust the region by hand with **Box**; anything you add to it counts as
background.

The **Number** check cannot read rotated text, characters that touch each
other, or text on a background of the same color. For those, a **Color** or
**Image** check on the part that matters often works instead.

## Window size

A value reads only while the window is the size it was when you drew the
region. If you use the window at more than one size, make a value at each
size, with an automation for each; AppyTappy reads the values made at the
window's current size.

While the window is closed or at a size no value was made at, AppyTappy reads
nothing. Automations neither run nor undo their changes until it can read
again.

## Delete a value

Select **Delete** next to a value, then confirm. A value that an automation
watches cannot be deleted: the Values page names the automations to change or
delete first.
