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
2. **Where to read.** Type a **Value name**, such as `Progress`, and choose
   the **Check**, one of the four [checks](#checks) below. Then show
   AppyTappy where the value is, in one of two ways:
   - **Drag over the live window.** While the window shows what to read, drag
     a box around it. The drag takes the screenshot as it starts, the
     picture holds still while you drag and gives way to the screenshot once
     it is taken, and the region goes on that screenshot.
   - **Take a screenshot and draw on it.** Select **Take screenshot**, or
     press the screenshot shortcut, `F9` at first, then drag a box on the
     screenshot, as [Draw the region](#draw-the-region) describes. The
     shortcut works while the other application has the keyboard, so you can
     carry on in it until it shows what you want and press `F9` then.

   The value's place goes on one screenshot, the only one the strip shows in
   this step. Another screenshot takes its place: the region moves onto it
   when the window has the same size, and at another size you draw the
   region again. The **X**, **Y**, **Width**, and **Height** fields set the
   region to the exact pixel, counted in the window's own pixels from its
   top-left corner. Turn on **This content may move** if the value shows in
   different places inside the window, as
   [Content that moves](#content-that-moves) describes. **Stays at** says
   which side of the window the region keeps to as the window changes size,
   as [Window size](#window-size) describes; it follows the region, so you
   can leave it alone. Then select **Next: characters** for a **Number**, or
   **Next: how to read it** for the other checks.

3. **Characters,** for a **Number**. Pick the reference the number reads
   with, make one with **New reference from this region**, or teach one with
   **Record readings**, as [Checks](#checks) describes. Then select
   **Next: check and save**.
4. **Check and save,** for a **Number**, or **How to read it** for the other
   checks, which set up the check here, as [Checks](#checks) describes, and
   take more screenshots where the check needs them, such as one for each
   look of a **State**. AppyTappy can also take those for you while you use
   the application: [record them](#record-screenshots). The box in this
   step shows what AppyTappy reads on the live window, a few times a second,
   where and how an automation reads the value once it is saved. While the
   window has a size the value isn't read at, the box says so, as
   [Window size](#window-size) describes.
   If the check cannot work, such as a bar whose filled and empty colors are
   the same, it says why there, and so it does when it can't read the live
   window, such as a number with a character its reference hasn't learned.
5. **Save.** Select **Save value**. **Back** returns to the step before. To
   finish it later, select **Save as draft** on any step
   ([Drafts](Drafts.md)).

The steps show above the fields, each with its number, and a tick once it has
what it sets. Select a step to go to it.

To use other keys for the screenshot shortcut, select **Change shortcut** and
press them, or change it in [Settings](Settings.md). F1 to F24, Print Screen,
and Pause work alone. A letter, a digit, Space, an arrow, Insert, Delete,
Home, End, Page Up, or Page Down needs Ctrl, Alt, Shift, or Win with it, so
it keeps working everywhere else. Other keys can't be the shortcut.

AppyTappy keeps the screenshots until you remove them, select
**Change window**, or leave the new value, and saves only the part inside the
box you drew, as
[What AppyTappy keeps](Getting-started.md#what-appytappy-keeps) describes.
Only a [draft](Drafts.md) keeps the screenshots themselves, until you finish
or delete it.
Before a change of window or leaving drops work you have not saved, it asks
first: select **Keep this window** or **Stay** to carry on, as
[Leave before you save](Getting-started.md#leave-before-you-save) describes.
**Change window** returns to the first step with the same application chosen.

To remove a screenshot, select it in the strip, then select
**Remove screenshot** with its number. The other screenshots keep their
numbers. A screenshot stays while the region is drawn on it or a state is taken
from it: draw the region on another screenshot, or remove the state, first.

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

## Draw the region

The region is the box that a value reads or a click looks for. You draw it on
a screenshot. A new value's region can also be dragged over the live window,
which takes the screenshot as the drag starts, as
[Make a value](#make-a-value) describes. A reference's
[boxes](References.md#boxes) are drawn, moved, and resized the same way.

- **Draw.** Drag from one corner of the thing to the opposite corner. The
  drag can start in the empty space around the screenshot, such as above its
  top edge, so a box that touches an edge is easy to draw: the box holds the
  part of the screenshot that the drag covers. A drag whose box covers none
  of the screenshot draws nothing, and so does a click.
- **Move.** Drag inside the box. It stops at the edges of the screenshot.
- **Resize.** Drag one of the four squares at its corners.

A click changes nothing, so clicking the box or a corner leaves a region you
typed in the **X**, **Y**, **Width**, and **Height** fields as it is. The
squares lie outside the box, so even a very small box can be moved by
dragging inside it.

The box changes when you let go of the mouse button. A drag that is
interrupted, such as by a screenshot that arrives while you drag, leaves the
box as it was, and you can drag again.

The box's outline and its label, which shows the region's size in the
window's pixels, lie outside the box, so nothing covers what you select. The
rest of the screenshot is dimmed a little.

To select something small, zoom in. Under the screenshot, **Zoom in** (**+**)
and **Zoom out** (**−**) change how large the screenshot shows, and the
percentage between them says how large that is against the window's own
pixels. **Fit** shows the whole screenshot again. Hold Ctrl and turn the mouse
wheel to zoom around the pointer. While the screenshot is larger than the
view, turn the wheel or drag the scroll bars to move around it. The wheel
also works in the middle of a drag, and the box follows the screenshot as it
scrolls, to draw a box larger than the view shows.
Zoom changes only how you see the screenshot: the region keeps its pixels.
The live window always shows whole, and a screenshot of another size starts
whole again.

## Record screenshots

A value needs screenshots of the right moments: each look of a **State**, a bar
partly full, a light while it is on. Catching them by hand while you use the
other application is hard, so AppyTappy can watch the value's region and take
them itself.

1. **Draw the region,** as in **Where to read**.
2. **Start recording.** Select **Start recording** under
   **Record screenshots**, in the step that sets up the check. The region turns red while it
   records, and the bar shows how long it has recorded.
3. **Use the application.** Each time the region shows a look that no
   screenshot shows yet, AppyTappy keeps a screenshot of the whole window. A
   look that comes back is not kept again.
4. **Stop and look through them.** Come back and select **Stop recording**.
   The recorded screenshots are in the strip with the ones you took, each
   marked with a red dot and the time it was kept, and the first one shows.
   They work like any screenshot: add a state from one, pick a color on one,
   or draw the region on one. Remove the ones you do not need one by one, or
   select **Remove all recorded** to remove every recorded screenshot that
   nothing uses. While it records, no screenshot can be removed: stop
   recording first.

A recording compares the whole region. It keeps a screenshot when the region
differs from every screenshot you have by more than a tolerance, on average
over all its pixels. So draw the region close around what changes:

- Where the background behind the thing changes, such as a scene behind an
  icon, most moments count as new looks, and the recording soon stops at its
  limit. A recording does not use what you
  [select in a state](#select-the-thing-in-a-state).
- Where only a small part of a large region changes, the average barely
  moves, and the change may never give a screenshot.

For those, take the screenshots by hand.

What a recording gives each check:

- **State.** A screenshot of each look, to add a state from. The tolerance is
  the check's **Tolerance** as it is when you start recording: lower it first
  to keep looks that differ less, or raise it to keep fewer. Changing it while
  a recording runs changes nothing until you record again.
- **Bar fill.** The bar at different fills. Pick the **Filled color** and the
  **Empty color** on one that shows both. The tolerance is fixed at 16, on
  the scale of 0 to 255 that a color's red, green, and blue use, so a bar that
  moves only a little may not give a new screenshot.
- **Color.** The region with and without the color. Pick the
  **Color to look for** on one that shows it. The tolerance is the same fixed
  16, so a small light in a large region may not give a new screenshot when
  it turns on.
- **Number.** A number learns characters, not looks, so it records readings
  instead. Under **Characters**, select **Record readings**. The
  reference you picked opens with its
  [recording](References.md#record-readings) already running on the value's
  region, unless it has a
  [draft of changes](Drafts.md#finish-or-delete-a-draft) to finish first.
  With no reference picked, a new one opens: click the characters on
  the screenshot to pick their color, then select **Start recording**.
  **Save reference** brings you back to the value.

When you [edit a value](#edit-a-value), a **State**, a **Bar fill**, or a
**Color** offers recording as a new value does, to take a state's picture
again or pick a color again, while the window has the size the value was drawn
at; at another size it says which size it needs. A **Number** offers
**Record readings**.

When you [fix a value at one size](#fix-a-value-at-one-size), a **State**
offers recording, with the tolerance of its first drawing. A **Bar fill** or a
**Color** does not, because its fix reads with the colors of its first drawing
and needs no other screenshot.

One recording keeps at most 16 screenshots, then stops and says so: remove the
ones you do not need and start again to keep more. Something that fades in or
out can leave a screenshot or two of the fade. Each screenshot is the whole
window, kept in memory until you remove it, change window, or leave. AppyTappy
writes one to disk only when you select **Save as draft**, which keeps the
recorded screenshots with the ones you took.

Keep the window the size it was when you took the screenshot you drew the
region on: at another size, recording pauses and says to resize the window. If
the window's capture fails for a moment, recording stops and says why; select
**Start recording** again once the window shows. If the window closes, nothing more can be
recorded from it, and the screenshots stay until you change window. Recording
again keeps only looks that no screenshot shows, so nothing is kept twice. A
look you removed the screenshot of is kept again if the region shows it.

With a large region that shows many looks, AppyTappy compares fewer moments,
so the rest of AppyTappy Setup stays quick. A look that shows only for an
instant can then be missed.

## Checks

- **Number** reads a number, such as `87%` or `-12.5`, with a
  [reference](References.md): the color and shapes of the application's
  characters. Pick one of the application's references, or select
  **New reference from this region** to make one from the region you drew.
  **Edit** changes the picked one, such as to teach it a digit it does not
  know yet. A reference that has yet to learn characters it needs lists them
  after **Missing**; it still reads the characters it knows.
  **Record readings** teaches the picked one, or a new one, from
  what the region shows while you use the application, as
  [Record screenshots](#record-screenshots) describes.
  **Characters it reads** says which of the reference's characters the value
  reads, as [Characters a number reads](#characters-a-number-reads)
  describes. Turn on
  **Whole number only** to ignore every digit after the
  decimal point, so `87.5%` reads as 87. The reference still has to know the
  point, so teach it on a screenshot that shows one.
  To read numbers with a thousands separator, such as `1,234`, teach the
  reference the `,` too. Turning on **Decimal point** and **Comma** under the
  reference's [**Characters it needs**](References.md#choose-what-it-needs)
  lists them as missing until it has learned them. That list only says what
  is left to teach: it does not change how a number reads. A `.` is always
  the decimal point, and a `,` only
  separates thousands: it counts only between groups of three digits, so
  `12,5` reads as 12. A number written with its point first, such as `.5`,
  reads as 0.5, and a `.` with no digit after it, such as `...`, is not a
  number.
- **Bar fill** reads how full a bar is, from 0 to 100. Draw the region
  closely around the bar, then select **Detect the bar**: AppyTappy finds the
  bar's two colors on your screenshots and fills in **Filled color** and
  **Empty color**, as [Detect a bar's colors](#detect-a-bars-colors)
  describes. To set a color by hand instead, use **Pick** next to it and
  click a filled or an empty part of the bar. Borders and labels inside the
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

A state name has 1 to 32 characters and no control character, such as a
tab. Spaces at either end don't count, and saving drops them. No state can be
named `none`, in any capitals, since the value reads `none` when no state
shows. Two states cannot share a name, ignoring case, so `Ready` and `ready`
are one name: each of them says **Another state has this name, or one that
differs from it only in capitals**. A name that breaks a rule says which under
it, and the value waits with **Fix each state name that has a message under
it** until every name can be saved.

### Characters a number reads

A reference can know a whole font: digits, signs, and letters. A value that
shows only some of them reads better when it reads only those, since it can
then no longer take one character for a lookalike, such as a `0` for an `O`
or a `2` for a `Z`. Under the reference you picked, **Characters it reads**
chooses them:

- **All it knows** reads every character the reference knows, and any it
  learns later. A new value starts with it.
- **Digits** reads `0` to `9` and nothing else. A digit the reference learns
  later is read too.
- **Digits and symbols** reads the digits and each character the reference
  knows that is neither a digit nor a letter, such as `%`, `.`, `,`, `:`,
  `/`, `-`, and `+`. It shows while the reference knows such a character. A
  symbol the reference learns later from another value or the References
  page is not added: the value then shows
  **Choose…** with the characters it reads turned on, and choosing
  **Digits and symbols** again adds the new symbol.
- **Choose…** shows each character the reference knows with a box, with the
  ones the value reads so far turned on. Turn on the ones the value shows, at
  least one. A character the value reads that the reference has yet to learn
  shows too, outlined: it reads once the reference learns it, and you can
  turn it off.

Picking another reference keeps **All it knows** and **Digits**, takes
**Digits and symbols** from the new reference's symbols, and takes out of a
**Choose…** set the characters that only the old reference knows. Teaching
the reference from this value, with **Edit** or **Record readings**, adds a
new symbol to **Digits and symbols** and changes no other choice.

Under the list, the value shows the characters it reads. The choice belongs
to the value. The reference keeps every character it knows, so another value
can read more of the same reference, and the References page still lists them
all.

The value takes its number from the characters it reads alone. A character
it does not read, and its reference knows, is text to it: it ends the number
or comes before it, and is never part of it. So **Digits** reads `87%` as 87
and `Lv.5` as 5.

The value reads nothing while a character it leaves out is a part of the
number its region shows, and says that it leaves out a character its number
holds, since the number would be another number without it. So **Digits**
reads none of `-5`, `.5`, `87.5%`, and `1,234`: add the `-`, the `.`, or the
`,`. With **Whole number only** on, the number ends at its `.` anyway, so
**Digits** reads `87.5%` as 87. A digit the value leaves out fails the read
the same way. A shape the reference has not learned still fails the read
before the number ends, whatever the value reads: text needs no teaching
only after a character the reference knows has ended the number, such as the
`HP` in `37% HP`.

A [fix](#fix-a-value-at-one-size) that reads with its own reference reads the
same characters of that reference.

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
asks you to select one. While you edit a saved value, it needs a screenshot
at the size the value was drawn at, as the region does.

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

## Detect a bar's colors

**Detect the bar** looks for a bar of two plain colors: one color from the
left of the region and the other after it, or from the bottom up in a region
that is taller than wide. Around the two the region may hold a border, a
rounded end, or a little background, and between them a soft edge at most.
It takes the left color, or the bottom one, as the filled color. If the bar
fills from the other end, select **Swap colors**.

It looks at the screenshot you drew the region on and at the newest other
screenshots of the same window size, and says so when it left older ones
out. It takes the colors from the first of them that shows both parts of the
bar, and proposes them only when they read every screenshot it looked at:

- When two screenshots show the bar at different fills, it names them, as in
  **Screenshots 1 and 2 show the bar in these two colors, at different
  fills**. A picture in two colors would not do that.
- Otherwise it says which screenshot it found the colors on. One fill cannot
  tell a bar from a picture in two colors, so compare the readings with what
  you see, or take a screenshot at another fill and detect again.
- A screenshot that shows neither color in the region, such as one of a
  menu, is named and not compared.
- When only some strips of the region show the bar from end to end, because
  a label or an icon lies over it or background beside it, it goes by those
  strips and says so. The rest counts only where it has one of the two
  colors.
- When the colors read a screenshot a few points off where the bar ends on
  it, as a label or a soft edge on the bar causes, it says so.

Under the colors, **These colors read** lists what each screenshot it looked
at reads with them, such as **Screenshot 1: 72**. Compare each number with
how full the bar looks on that screenshot. The list follows the colors, so
it shows for colors you pick by hand too. To see which pixels count, turn on
[**Show matching pixels**](#show-matching-pixels): if anything beside the
bar is tinted as filled or empty, such as a second bar or a background in
the same colors, draw the region more tightly around the bar.

If you already picked a color by hand, **Detect the bar** asks before it
replaces it, and the list shows what the detected colors read meanwhile:
select **Use detected colors** or **Keep my colors**.

While you [edit a value](#edit-a-value), **Detect the bar** looks at the
screenshots at the size the value was drawn at, whichever screenshot shows.
Until there is one, it asks you to take a screenshot with the window at that
size.

It proposes no colors, and says why and what to do, when:

- No screenshot it looked at shows the bar partly filled, and the region is
  one color on the screenshot you drew it on. Take a screenshot where the
  bar is partly filled.
- The two colors are too close to tell apart.
- The region does not look like a bar of two plain colors. That goes for a
  bar made of separate segments, a striped or textured fill, a fill that
  fades along the bar, and a label, an icon, or a busy background that
  leaves too few strips of the bar free from end to end. Around a label or
  an icon, draw the region over a strip of the bar above or below it. Over
  a busy background, draw the region closer to the bar.
- Something lies over part of the bar, such as a large label or an icon at
  one end, or one part of the bar has a shine in another shade over half its
  length or more. The colors read the screenshot right, but would read other
  fills wrong, because the pixels that count are not spread evenly along the
  bar, or are fewer in one part than in the other. Draw the region over a
  strip of the bar that is clear from end to end, or pick the colors by
  hand.
- The colors found on one screenshot do not read another that shows one of
  them. The bar changes color as it fills or from one screenshot to the
  next, or a label, an icon, or something else such as a tooltip covers most
  of its height there. Two colors cannot read a bar that changes color,
  whoever picks them. Around a label, draw the region over a strip of the
  bar above or below it.
- The region is too short to hold a bar.

Where two colors can read the bar, pick them by hand then: the check reads
any bar whose filled and empty parts each keep one color, whichever way it
fills.

Text or marks drawn over the bar in one of its own two colors cannot be told
from the bar on a screenshot where they blend into it, whoever picks the
colors. A shine or pattern in another shade over less than half of a part's
length can also be proposed, and then reads some points off at other fills.
So check the list of readings at more than one fill.

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
[What a reference can't learn](References.md#what-a-reference-cant-learn)
says what else gets in a number's way, and what to type for it.

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
  value whose region would reach past the window's edge isn't read. A
  screenshot shows only what the application draws, without the title bar
  and borders that Windows draws around it, so a title bar doesn't move a
  value between windowed and fullscreen. If a scaled region lands beside its
  value, [fix the value](#fix-a-value-at-one-size) at the other size.
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
read again. [AppyTappy's log](Troubleshooting.md#appytappys-log) says
which values it doesn't read at the window's size and why, each reason once
until the value reads again.

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
3. **How to read it.** Change the colors, the states' names, or the
   **Tolerance**. A **Number** has two steps here, as a new one does:
   **Characters** changes the reference and **Whole number only**, and
   **Check and save** follows. The box in the last step shows what the value
   would read on the live window once saved.
4. Select **Save changes**. **Discard**, or leaving the editor, keeps the
   value as it was saved. Leaving asks first only once you changed
   something. An edit can't be saved as a [draft](Drafts.md): it changes a
   value your automations already use, so it is either saved or dropped.

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

The box in the last step shows what the edited value reads at the
window's size now: scaled, at its fix, or
**Not read at 2560 × 1440, the window's size now**, as
[Window size](#window-size) describes.

### States in an edit

A **State** check keeps the picture saved for each state while the region
stays where it was. **States** then lists each state with
**Saved picture**: rename a state there, or change the **Tolerance**. A
new name follows the same [rules](#checks) as a new value's.

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

## How a value was made

The Values page says under a value's name how it was made, unless you made
it by hand on screenshots you took:

- **Made with a recording.** A [recording](#record-screenshots) kept a
  screenshot the value is set up on.
- **Imported from** a file, with the file's name. The value came with an
  application you [imported](Applications.md#share-an-application).

A value you edit keeps what it says while its region stays where it is.
Draw the region again and the value says how you make it now.

## Delete a value

Select **Delete** next to a value, then confirm. A value that an automation
watches cannot be deleted: the Values page names the automations to change or
delete first.
