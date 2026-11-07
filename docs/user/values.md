# Values

A value is something AppyTappy reads from one part of an application's window,
such as a number, how full a bar is, or whether an icon shows. Automations
watch values and act on them.

The **Values** page lists each saved value under its application, with how
it reads and the automations that use it. A green dot marks an application
whose window is open now.

## Make a value

On the Values page, select **New value**.

1. **Pick the window.** The list shows the open windows; type in the filter
   to find one. If the window belongs to an application you already saved,
   AppyTappy picks that application, so the new value joins it. Otherwise
   **Save values to** is **A new application**: give it a name, and check how
   AppyTappy will [find its window](applications.md#how-appytappy-finds-the-window).
   Then select **Capture this window**.
2. **Take screenshots.** Select **Take screenshot**, or press the screenshot
   shortcut, `F9` at first. The shortcut works while the other application
   has the keyboard, so you can carry on in it until it shows what you want
   and press `F9` then. To use other keys, select **Change shortcut** and
   press them. A letter, a digit, or Space needs Ctrl, Alt, Shift, or Win
   with it, so the key keeps working everywhere else.
3. **Draw the region.** On a screenshot, drag to draw a box around what to
   read. Drag inside the box to move it, or drag a corner to resize it. The
   **X**, **Y**, **Width**, and **Height** fields set it to the exact pixel,
   counted in the window's own pixels from its top-left corner.
4. **Choose the check.** Pick how AppyTappy reads the region, from the four
   [checks](#checks) below.
5. **Watch the reading.** Once the check is complete, the box under it shows
   what AppyTappy reads on the live window, a few times a second. If the
   check cannot work, such as a bar whose filled and empty colors are the
   same, it says why there.
6. **Name it and save.** Type a **Value name**, such as `Progress`, and
   select **Save value**.

AppyTappy keeps the screenshots until you select **Change window** or leave
the new value.

## Checks

- **Number** reads a number, such as `87%` or `-12.5`. Next to
  **Characters' color**, select **Pick**, then click the characters on a
  screenshot. Then select each screenshot and type what the region shows on
  it, such as `87%`. AppyTappy learns the shape of each character from what
  you type, so label enough screenshots to show every digit it needs to know.
  **Tolerance** says how far a pixel's color may be from the characters'
  color and still count.
- **Bar fill** reads how full a bar is, from 0 to 100. Use **Pick** next to
  **Filled color** and click a filled part of the bar, then next to
  **Empty color** and click an empty part. Borders and labels inside the
  region don't count.
- **Color** is yes when the region's average color is close to the
  **Color to look for**, such as a light that turns red. Use **Pick** and
  click the color on a screenshot. **Tolerance** says how close.
- **Picture** is yes when the region looks like it does on a screenshot you
  choose, such as an icon that appears. Select that screenshot, then
  **Use this one** next to **Compare against**. **Tolerance** says how much
  it may differ on average.

The **Number** check cannot read rotated text, characters that touch each
other, or text on a background of the same color. For those, a **Color** or
**Picture** check on the part that matters often works instead.

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
