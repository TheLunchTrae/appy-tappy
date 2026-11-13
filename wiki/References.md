# References

A reference is how AppyTappy knows an application's characters: their color,
and the shape of each digit and sign. A [Number](Values.md#checks) value reads
its region with one of its application's references, so several values in the
same font share one reference, and teaching it a new digit teaches all of
them. A reference can know a whole font, and each value says which of its
characters it reads, as
[Characters a number reads](Values.md#characters-a-number-reads) describes.

The **References** page lists each application's references, with the
characters each knows and the values that read with it. A reference that has
yet to learn characters it needs lists them after **Missing**, and so does
the list of references you pick from for a Number value.

A reference reads its characters at any size, so a number drawn larger than
the ones it learned from still reads.

## Make a reference

Select **New reference** on the References page, or select
**New reference from this region** while you
[make a value](Values.md#make-a-value). From the References page, first choose
the application the reference belongs to, or select **Add application**, then
select **Capture this window**, as the first step of
[making a value](Values.md#make-a-value) describes.

1. **Name it,** such as `Percent font`.
2. **Pick the color.** Next to **Characters' color**, select **Pick**, then
   click the characters on a screenshot. **Tolerance** says how far a pixel's
   color may be from it and still count: drag the slider or type a number.
   Turn on **Show matching pixels** to tint every pixel on the screenshot
   that counts as part of the characters, and watch the tint as you drag
   **Tolerance**. Raise it until the characters are tinted whole, and lower it
   if the background around them is tinted too. Under the switch, AppyTappy
   counts the tinted pixels in the region and on the rest of the screenshot.
   The tint is only a guide, as
   [Show matching pixels](Values.md#show-matching-pixels) describes.
3. **Choose the characters it needs.** Under **Characters it needs**, turn on
   what the numbers can show. A new reference starts with **Digits** on.
   [Choose what it needs](#choose-what-it-needs) describes the choices.
4. **Teach it the characters.** On a screenshot, drag to draw a box tightly
   around some characters, then type what the box shows, such as `87%`, in
   its row under **Boxes**. AppyTappy learns the shape of each character from
   what you type. [Boxes](#boxes) describes the boxes. Draw boxes until
   **Characters still missing** says it knows every character it needs,
   [find more like it](#find-more-like-it) on the same screenshot, or
   [record readings](#record-readings) to collect them as they show.
5. **Save.** Select **Save reference**. A reference with characters still
   missing saves too: the editor says so next to the button, and you can
   teach them later. To finish it later instead, select **Save as draft**
   ([Drafts](Drafts.md)). A draft doesn't keep a recording.

The box at the bottom of the panel shows what the reference reads on the
live window, a few times a second. When it can't read the window, it says why
instead, in the words
[A number reads in setup but the automation does nothing](Troubleshooting.md#a-number-reads-in-setup-but-the-automation-does-nothing)
explains. That box is the reference's own reading of the region you drew, at the
window's size now, as [Window size](Values.md#window-size) describes for a
value. A reference you make or edit from a number reads here as that number
will: only the characters the number reads, and without the digits after
the decimal point when the number ignores them. The automation reads with
the same reference, where its value's region is at the window's size.

To teach a saved reference more, select **Edit** next to it on the References
page, or **Edit** while you pick it for a value. What it learned before stays;
new boxes and readings add to it. Changing its color or tolerance changes how
every value that reads with it reads, and the editor names them.

## Boxes

A box is a part of a screenshot that shows some characters, with the text you
type for it. A reference learns from every box that has text. Draw as many as
you like, on any screenshot, and several on one screenshot too.

- **Draw.** Drag on a screenshot, away from the boxes there. The new box is
  selected, and the keyboard moves to its row under **Boxes**, so you can
  type what it shows straight away.
- **Select.** Press a box on the screenshot, or its number under **Boxes**,
  which also shows the screenshot the box is on. Each row says which
  screenshot that is, such as **On screenshot 2**.
- **Move and resize.** Drag the selected box to move it, or drag one of the
  squares at its corners to resize it, as
  [Draw the region](Values.md#draw-the-region) describes.
- **Type what it shows.** Type exactly what the box shows in its row. A box
  with no text teaches nothing.
- **Delete.** Select **Delete** in the box's row, or press Delete while the
  box is selected on the screenshot.

A reference you teach by itself reads the window, and
[records](#record-readings), at its first box, whose row says
**Reads here**. While you teach a reference for a value, it reads at the
value's region instead, and no box says so.

Every box goes on a screenshot of one window size, the size the reference
reads at: the size of the screenshot its first box is on, or, while you teach
a reference for a value, the size the value's region is drawn at. On a
screenshot of another size, the editor says which size the boxes go on.

While you teach a reference for a value, the screenshots show the boxes, not
the value's region. The region is as you drew it when you go back to the
value.

## Choose what it needs

**Characters it needs** is your list of what to teach, so you do not have to
work out which characters are still to come. Turn on what the numbers this
reference reads can show:

- **Digits** is `0` to `9`.
- **Decimal point**, **Comma**, **Percent**, **Minus**, **Plus**, and
  **Colon** are each one sign. Turn on the ones the application draws.
- **Uppercase** and **Lowercase** are the English letters `A` to `Z` and `a`
  to `z`, each case on its own, and **Question mark** and
  **Exclamation mark** are the punctuation that goes with them. Only
  [Number](Values.md#checks) values read with a reference, and AppyTappy has
  no check that reads text. Turn letters on only when a number's region shows
  them with the number, such as the `Lv` in `Lv.5`.
- **Other characters** takes anything else: type each character the region
  shows, such as `/` or `°`. Delete a character there to stop needing it.
  Spaces do not count. A list with more characters than a reference can
  need turns **Save reference** off until you remove some, and the editor
  says so.

A choice that holds only some of its characters, such as **Digits** with `0`
to `5`, shows as partly on with how many it holds, and its characters show
in **Other characters**, where you can remove each. Select the choice to turn
all of it on, and again to turn all of it off.

The editor compares the list with what the reference learned, and keeps up as
you type what boxes show, add found text, keep recorded readings, or correct
them:

- **Characters still missing** shows each needed character it has yet to
  learn, outlined. This is what is left to teach. Once it has them all, it
  says so.
- **Characters it knows** shows the needed characters it learned. Characters
  it learned that are not on the list show below them. They are fine to keep:
  the list says what you need, not what is allowed.

The list only guides you. AppyTappy learns a character from a box, found
text you add, or a kept reading, never because it is on the list, and a
number reads the same way whatever you turn on. [Checks](Values.md#checks) says how a number reads its
`.` and `,`.

A reference saved without a list shows every character it knows under
**Characters it knows**, and nothing as missing. **Edit** it and turn on what
it needs to see what is left to teach.

## Find more like it

A screen often shows more numbers in the same font than the one you drew a
box around, such as a stats panel. Pick the **Characters' color**, select a
box, and then select **Find more like it**. AppyTappy looks on the box's
screenshot for other text in that color and as tall as the text in the box,
away from the boxes that are there already, and reads each as text.

The list holds only the finds that may show the characters the reference
[needs](#choose-what-it-needs). A letter that is often read in place of a
digit counts as that digit, so a reference that needs digits still lists a
find read as `12O5`, whose `O` may be a `0`. The list says how many finds it
skipped. A find you ticked before its text was read is skipped the same way
once the text shows other characters, so it leaves the list and is not
added. A reference with no list of what it needs lists every find.

1. **Tick the ones to add.** Each find in the list shows outlined on the
   screenshot under the same number, with the text AppyTappy read. A skipped
   find keeps its number, so the numbers can have gaps. A find starts unticked: tick the
   ones to add, or **Select all**, which also clears every tick. Fix any text
   it got wrong; typing a find's text ticks it.
2. **Deal with the flagged ones.** A ticked find is flagged when it can't be
   added as it is: it has no text, AppyTappy read a letter while the reference
   has learned none, such as a `0` read as `O`, or its text gives one shape
   two different characters. Type what it shows, or leave it out.
   **Deselect flagged**, which shows while a ticked find is flagged, clears
   the tick of every flagged find at once, so you can add the rest.
3. **Reread, if the color was off.** **Reread** shows while AppyTappy could
   not read a find, or while a flagged find still has the text AppyTappy read.
   It reads those finds again with the **Characters' color** and
   **Tolerance** as they are set now, and keeps your ticks and the text you
   typed. The same color and tolerance read the same text, so **Reread**
   waits, and says so, until you change one of them, such as a tolerance
   that left parts of the characters out. A find AppyTappy could not read at
   all is read again whenever you select **Reread**.
4. **Add them.** Select the button that says how many you add, such as
   **Add 3 to the reference**. Each becomes a [box](#boxes), which you can
   move, retype, or delete like one you drew. Until you add them,
   **Characters it knows** outlines the characters only the finds would
   teach, and they still count under **Characters still missing**.
   **Not these** closes the list without adding any.

Draw the box tightly around the text first, since the finds' characters must
be about as tall as the text in it; a comma or a mark below them does not
count. Text of another size is skipped, and the list says how much. One search
lists at most 12 finds; add those, then select **Find more like it** again, and
it lists the rest, leaving out what you added.

## What a reference can't learn

A box's text has one character for each character the box shows, and
AppyTappy says so when the two differ:

- **The box shows fewer characters than you typed.** Characters that touch
  count as one, and a reference can't learn them.
- **The box shows more characters than you typed.** A character counts once
  for each of its marks that has blank space on both sides. Some percent
  signs are three such marks: a ring, a slash, and a ring. Type a character
  for each mark, such as `87o/o`; the number still reads as 87. Anything else
  in the characters' color counts too, so draw the box around the number
  alone.

A reference knows a character as it looked beside the others in its box.
A sign that reaches above or below the digits, such as the `,` in `1,234`,
changes how every character beside it looks to AppyTappy. Draw boxes around
readings that show the sign until they show every digit, and around readings
without it too.

Small text takes more boxes. Where the characters are less than about 16
pixels tall, draw boxes around several readings of each character, or
[record](#record-readings) them.

## Record readings

Typing what a counter shows on screenshot after screenshot is slow when it
shows many values. A reference can record instead, while you use the
application, and collect the characters one place in the window shows: the
value's region while you teach a reference for a value, or else the
reference's first box. While you make a **Number** value,
**Record readings** under **Characters** opens this recording on the value's
region. The other checks
[record screenshots](Values.md#record-screenshots) instead.

A recording keeps a reading when it shows a character shape that no kept
reading has. A counter that runs from `0%` to `100%` shows 101 readings but
only eleven characters, the digits `0` to `9` and the `%`, so it leaves about
ten readings to review, or a few more where a character shows as more than
one shape. Every other reading is counted, not kept.

A shape can first show beside something that is no character, such as a `7`
caught while the digit next to it was half drawn. No text fits that reading,
so the recording keeps one more reading for the `7`: the next one whose
other shapes have all shown elsewhere too. **Record more** does the same.

That helps when the stray shape showed in one reading only. A shape that
shows in two different readings counts as a character, so an in-between
shape that a counter draws at every change passes for one. The recording
then keeps no cleaner reading, however long it records, and review cannot
offer one. If a character stays under **Still missing**, or its only reading
cannot be typed, take a screenshot that shows the character without the
stray shape and draw a box around it instead.

1. **Start recording.** Draw a box around the characters and pick the
   **Characters' color** first. Under **Record readings**, choose where to
   **Look in**: **This place**, or **The whole window**, which
   [Record the whole window](#record-the-whole-window) describes. Then
   select **Start recording**. The region turns red while it records.
2. **Use the application.** Go on in it until the region has shown the
   characters you need. AppyTappy reads each kept reading's text for you, and
   **Recording** says what that gives so far:
   - **Still missing** shows each character the reference needs that nothing
     has shown yet. Make the application show these. It shows when you
     [chose what the reference needs](#choose-what-it-needs).
   - **Found, not checked yet** shows the characters AppyTappy read in the
     kept readings. They are its guess until you check their readings in
     review: type in a reading's text, set its **Keep**, or select
     **Keep all** or the button that keeps them. **Keep all** counts every
     reading as checked, so read the texts first.
   - **Found** shows the characters the reference knows already, and the ones
     in readings you checked.
   - **Without text** counts the shapes that no reading's text names, such as
     while a reading is still being read.

   The newest kept readings show below. Moments when the region shows none of
   the characters' color, such as while a menu covers it, are skipped.
   Recording goes on until you stop it.

3. **Stop and review.** Come back and select **Stop and review**. Each kept
   reading shows with the text AppyTappy read, and the readings that add
   characters come first. Fix any text it got wrong, and clear **Keep** on
   any reading that is not the value, such as a paused screen. Under each
   reading:
   - **Adds** lists the characters that no other kept reading teaches, each
     with how often its shape showed, such as `seen 214 times in 37
     readings`. The counts say how much the recording saw of a shape. They do
     not say that its character is right: only the text you check does.
   - **Would add** lists the same for a reading that is left out: what
     keeping it would teach.
   - **Shows 2 shapes no kept reading has**, with the number of shapes,
     stands in for either list while the reading's text does not have one
     character for each of its shapes, so the shapes have no characters to
     show yet.
   - **Adds nothing** marks a reading whose characters other kept readings
     all show. It starts with **Keep** cleared, and you can leave it out.
   - A reading whose text gives a shape another character than another
     reading's text gives it, such as a `1` read as `l`, is flagged, and so
     is the other reading, whether they are kept or left out. Neither starts
     left out. Fix the text that is wrong. While both are kept, **Keep**
     waits.

   A reading's **Keep** is set once, when review first shows the reading
   with its text read, and after that only you change it. A reading still
   being read says **Reading…** and gets its **Keep** when its text arrives.

   The same lists as while recording show above the readings, and follow
   what you type and keep. When the only readings that could teach a
   character are left out, a notice there says **No kept reading teaches**
   it and offers a button for each reading to keep for it, such as
   **Keep 12, first at 0:02**.

4. **Keep them.** Select the button that says how many you keep, such as
   **Keep 12 readings**. The kept readings teach the reference like boxes
   do. **Review recording** opens them again with your choices, and
   **Record more** goes on with the same recording. **Back** leaves review
   without keeping, and your changes stay for next time. To record more, keep
   the place it reads, the characters' color, and the tolerance as they
   were, on a screenshot of the same window size: if you changed them,
   **Use its settings** puts them back. In a reference you teach by itself,
   that moves the box it recorded at back to where it was and to the top of
   the list, or adds an empty box there if you deleted it. Or **Discard**
   the recording to start a new one.
   Changing window also discards it.

A digit, a sign such as `%` or `.`, and a letter are collected the same way.
One character can show as more than one shape, and the recording keeps each:
an edge that blends with the background differently, or a digit beside a `,`
that reaches below the line, is another shape of the same character. Type the
same character for each. A sign that the application draws as separate marks
side by side, with a gap between them, counts as one shape for each mark, and
a reference cannot learn it as one character:
[What a reference can't learn](#what-a-reference-cant-learn) says what to
type for it.

Keep the window the size it was when you took the screenshot the recorded
region is on: at another size, recording pauses and says to resize the
window. Every screenshot with a box, and the recording, must have that size
too, and the reference says which one does not. What a reference learned still reads
at other sizes. If the window's capture fails for a moment, recording stops
and says why; select **Record more** once the window shows again. If the
window closes, nothing more can be recorded from it. Review and keep the
readings you want, then select **Save reference**, which still works after
the window closed: **Change window** drops everything you have not saved. To
record more, open the window again and **Edit** the reference.

A recording finds its shapes with the characters' color and tolerance it was
started with, and a reference learns the kept readings with the ones it has
when you save. Keep them as they were until you save. With another color or
tolerance a kept reading can show other shapes than the recording counted:
its text may not fit it anymore, which review and the save flag, and a shape
the recording took for one it had may be in no kept reading. **Use its
settings** puts the recording's own back.

A recording is full once its region has shown more than 256 different shapes,
more than a reference learns, and says so. It keeps nothing more. That many
shapes mean the region shows more than the value, such as a picture behind
it: draw the region around the value alone and record again. A recording is
also full once it has shown more than a million different readings, which
takes hours of a timer that changes many times a second.

If AppyTappy cannot read a reading's text, review leaves its text empty for
you to type.

The text in review comes from a separate text reader that only fills in the
text for you to check. It doesn't show what the reference reads: the box
at the bottom of the panel does, on the live window.

To remove a screenshot, select it, then select **Remove screenshot** with its
number. One the value's region is drawn on, or that a box is on, stays: draw
the region on another screenshot or delete its boxes first. A screenshot
whose found text is listed stays until you add the finds or select
**Not these**.

### Record the whole window

A counter in one place shows its characters slowly, and the same font is
often elsewhere in the window too: a stats panel, a second counter, a score
on a results screen. With **Look in** set to **The whole window**, a
recording collects characters from all of it. The place the reference
reads, its first box or the value's region, is the example: AppyTappy looks
for text in the **Characters' color** and as tall as the text that place
shows on its screenshot, anywhere in the window, and reads each piece like
the place itself. While it records, the window shows an
outline around each piece it found the last time it looked, and **Recording
the whole window** says in how many places that was.

It has limits, and review is where you deal with them:

- **It can pick up other text.** A menu's label or a name in the same color
  and size is kept too. When you
  [chose what the reference needs](#choose-what-it-needs), a reading whose
  text shows none of those characters starts with **Keep** cleared and says
  so; set its **Keep** if you want it after all. A reading that mixes both,
  such as `Lv 5`, starts kept, since its `5` may be the only one. So does
  one that may be a number misread, such as `SOS` for `505`, when every
  character of it is a letter often read in place of a digit.
- **It takes the text nearest the place first.** Each time it looks, it
  takes up to 32 pieces of text, starting with the ones nearest the place
  the reference reads. A recording is full after 256 different shapes; if a
  window with much other text fills it, record **This place** instead.
- **It looks twice a second at most.** Looking over a whole window takes
  longer than one place, and on a large window one look can take longer
  than half a second, so text that shows only briefly can be missed.
  **This place** looks about 30 times a second.
- **It finds one size.** Text much larger or smaller than the example's is
  not collected. What the reference learns still reads at other
  sizes.
- **It reads text by itself on a line.** Text that touches other things in
  the same color, or a line of more than 24 characters, is skipped.

**Record more** goes on looking where the recording looked before.

## Delete a reference

Select **Delete** next to a reference, then confirm. A reference that a value
reads with, at every size or in a fix at one size, cannot be deleted:
AppyTappy names the values to change or delete first.
