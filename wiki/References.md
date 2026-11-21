# References

A reference is how AppyTappy knows an application's characters: the shape
of each digit and sign, in any colors. A [Number](Values.md#checks) value reads
its region with one of its application's references, so several values in the
same font share one reference, and teaching it a new digit teaches all of
them. A reference can know a whole font, and each value says which of its
characters it needs, as
[Characters a number needs](Values.md#characters-a-number-needs) describes.

The **References** page lists each application's references, with the
characters each knows and the values that read with it. The list of
references you pick from for a Number value also shows, after **Missing**,
the characters that value needs and the reference has yet to learn.

A reference knows each character by its shape alone. It keeps no color, so
a number that turns from white to gold or red still reads, on another
background too. It reads its characters at any size, so a number drawn
larger or smaller than the ones it learned from still reads. One reference
is enough for a font: teach it each character once.
[What a reference can't learn](#what-a-reference-cant-learn) says where
that stops.

## Make a reference

Select **New reference** on the References page, or select
**New reference from this region** while you
[make a value](Values.md#make-a-value). From the References page, first choose
the application the reference belongs to, or select **Add application**, then
select **Capture this window**, as the first step of
[making a value](Values.md#make-a-value) describes.

1. **Name it,** such as `Percent font`, with a name no other reference of
   the application has. Capitals of the letters A to Z don't count:
   `Percent font` and `percent font` are one name. Until it has a name,
   **Save reference** waits, and says `Name the reference.` above the button.
2. **Teach it the characters.** On a screenshot, drag to draw a box tightly
   around some characters, then type what the box shows, such as `87%`, in
   its row under **Boxes**. AppyTappy learns the shape of each character from
   what you type. [Boxes](#boxes) describes the boxes. Draw more boxes,
   [find more like it](#find-more-like-it) on the same screenshot, or
   [record readings](#record-readings) to collect characters as they show.
   While you teach a reference for a value,
   [**Characters still missing**](#what-it-has-learned) says what the value
   needs that is left to teach.
3. **Save.** Select **Save reference**. A reference with characters still
   missing saves too: the editor says so next to the button, and you can
   teach them later. When some of its characters are AppyTappy's guesses
   that nobody has confirmed, **Not confirmed yet** above the button
   [offers to check them](#check-before-you-save) first. To finish it later
   instead, select **Save as draft** ([Drafts](Drafts.md)). A draft doesn't
   keep a recording.

The box at the bottom of the panel shows what the reference reads on the
live window, a few times a second. When it can't read the window, it says why
instead, in the words
[A number reads in setup but the automation does nothing](Troubleshooting.md#a-number-reads-in-setup-but-the-automation-does-nothing)
explains. That box is the reference's own reading of the region you drew, at the
window's size now, as [Window size](Values.md#window-size) describes for a
value. A reference you make or edit from a number reads here as that number
will: only the characters the number needs, and without the digits after
the decimal point when the number ignores them. The automation reads with
the same reference, where its value's region is at the window's size.

To teach a saved reference more, select **Edit** next to it on the References
page, or **Edit** while you pick it for a value. From the References page the
editor opens at once and finds the application's window itself.
[While the application isn't open](Values.md#while-the-application-isnt-open),
you can still rename the reference and save: its samples stay as
they are. Teaching it more needs a box on a screenshot, so screenshots and
recording wait for the window. What it learned before stays; new boxes and
readings add to it. Changing it changes how every value that reads with it
reads, and the editor names them.

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

## What it has learned

**Characters it knows** shows every character the reference learned, and
keeps up as you type what boxes show, add found text, keep recorded
readings, or correct them.

While you teach a reference for a value, the editor also shows
**Characters the value needs**, the value's
[**Characters it needs**](Values.md#characters-a-number-needs), and compares
them with what the reference learned. You can change them there too: the
value takes the change when you save the reference, and **Discard** leaves
the value's list as it was. A [fix](Values.md#fix-a-value-at-one-size)
needs the characters its value does, so its reference's editor shows the
list without the choices; edit the value to change it.

- **Characters still missing** shows each needed character it has yet to
  learn, outlined. This is what is left to teach. Once it has them all, it
  says so.
- **Characters it knows** shows the needed characters it learned. Characters
  it learned that the value does not need show below them. They are fine to
  keep: another value in the same font can need them.

When two characters have the same shape in what the reference learned,
it can't tell one from the other, and reads neither. When the value needs
at least one of them, a warning under **Characters it knows** names them.
On the References page, it goes by what the values that read with the
reference need. Most often a box or a reading has the wrong text: check
that the text typed for each is what it shows, and retype or remove the
wrong one. Two characters that a font draws the same, such as `0` and `O`
in some, stay alike; a value that
[needs](Values.md#characters-a-number-needs) only one of them still reads
it.

When AppyTappy can't check for that, the editor says **Can't check for
characters that look alike.** and why, in place of the warning. No warning
then doesn't mean that the reference has no such characters. The check
runs again when a box or its text changes. A reference that can't learn
from a box, a kept reading, or a saved picture has nothing to check yet:
the reading says which one it can't learn from and why, and the warning
waits until it can.

The References page marks a saved reference that has such characters with
**May misread** beside its name. The **?** beside the mark names them, and
**Edit** shows what to do. The mark shows a moment after the
page does. The warning doesn't hold up **Save reference**.

AppyTappy learns a character from a box, found text you add, or a kept
reading, never because a value needs it. A reference you make or edit from
the References page is taught for no value, so nothing shows as missing
there, and neither does it when the value needs no characters.

## Find more like it

A screen often shows more numbers in the same font than the one you drew a
box around, such as a stats panel. Select a box, and then select
**Find more like it**. AppyTappy looks on the box's screenshot for other
text in the color of the text in the box, at any size, away from
the boxes that are there already, and reads each as text. Text in another
color needs a box of its own first. Where the text in the box is several
colors, such as a gradient, it finds what stands out from the box's
background instead.

Text as tall as the text in the box comes first in the list, then text of
other sizes, starting with the size nearest the box's. A find of another
size says how tall its characters are, such as **Text 28 px tall**. Add
those too when a value shows that size: a reference reads other sizes
than it learned, and reads a size best once it has learned from it.

While you teach a reference for a value, the list holds only the finds that
may show the characters the value
[needs](Values.md#characters-a-number-needs). A letter that is often read in
place of a digit counts as that digit, so a value that needs digits still
lists a
find read as `12O5`, whose `O` may be a `0`. The list says how many finds it
skipped. A find you ticked before its text was read is skipped the same way
once the text shows other characters, so it leaves the list and is not
added. A reference taught for no value, or for one that needs no
characters, lists every find.

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
3. **Add them.** Select the button that says how many you add, such as
   **Add 3 to the reference**. With none ticked it says **Add to the
   reference** and waits. Each becomes a [box](#boxes), which you can
   move, retype, or delete like one you drew. Until you add them,
   **Characters it knows** outlines the characters only the finds would
   teach, and they still count under **Characters still missing**.
   **Not these** closes the list without adding any.

Draw the box tightly around the text first, since the list starts with the
finds whose characters are about as tall as the text in it; a comma or a
mark below them does not count. Text with characters under 8 pixels tall is
found only when the text in the box is that small too, and anything taller
than a quarter of the screenshot is not taken for text. One search
lists at most 12 finds. Add the ones you want, then select
**Find more like it** again, and it lists the rest: it leaves out what you
added, and every find an earlier list on that screenshot had that you
didn't add, including the lists you closed with **Not these** and the finds
it skipped for showing characters the reference does not need. A list that
leaves some out says how many; select **Show them again** to see them.
Closing the reference editor forgets them.

## Check characters one at a time

AppyTappy tends to misread a character the same way wherever it shows, such
as every `7` as a `1`. **Check characters** lets you fix that once instead of
in each text. It shows above the list of finds, in a recording's review, and
under **Not confirmed yet** above **Save reference**. It goes through the
characters of the boxes, the listed finds, and the kept readings together,
so one answer also fixes a box you drew by hand.

1. **Answer for each character.** Each step shows one character, drawn
   large, and every place that shows it, with the character outlined. When
   the text has it drawn more than one way, each way has its own tile. If
   they are all that character, select **Yes, it is 7**, or
   **Yes, they are all 7**. The characters the value
   [needs](Values.md#characters-a-number-needs) come first.
2. **Fix one that is something else.** Type the right character in
   **It is** under its tile and press Enter. Every place that shows it gets
   that character, and the tile moves to that character's step.
   Escape in the field takes back what you typed.
   **Not a character** is for something that is no text, such as part of an
   icon: every place that shows it is left out. The tile stays in its step,
   and **It is a character** takes that back.
3. **Type what does not fit.** A last step, **Type what these show**, lists
   any place whose text has another number of characters than its picture
   shows, so its characters could not be checked one at a time. Type what it
   shows, or leave it out. What you type counts as your answer for its
   characters. A place like that gets its text by itself when your answers
   cover all its characters.
4. **Finish.** **Done**, **Back to the screenshot**, **Back to review**, or
   Escape closes the check. The texts it fixed stay in the list of boxes,
   in the list of finds, and in review, where you add or keep them as
   before. A reading the reference keeps already has its fixed text right
   away. One that shows what is not a character is kept no longer, and
   **It is a character** keeps it again.

**Skip** goes on without answering, and **Back** returns to the step before.

You can answer from the keyboard. The **?** beside the buttons lists the
keys:

| Key         | What it does                                                              |
| ----------- | ------------------------------------------------------------------------- |
| Enter       | Presses the highlighted button: the **Yes** answer, **Next**, or **Done** |
| A character | Goes in **It is** under the first shape not checked yet                   |
| Alt+S       | **Skip**                                                                  |
| Alt+B       | **Back**                                                                  |

A character you type goes in **It is** under the step's first shape once
all its shapes are checked. Enter in **It is** makes the character your
answer. Alt+S or Alt+B does that too when you have typed a character and
not pressed Enter yet, and stays at the step: press it again to move.
Typing the character the step already asks about does nothing.

Enter and typing answer a step only while the check has focus: select
anywhere in the check to give it focus. A text field that has focus keeps
Enter and the characters you type, and a button that has focus keeps Enter,
as anywhere in Windows.

A find whose characters you all answered for is ticked, and you can clear
the tick afterward. A reading that review leaves out is not asked about and
stays left out, and its text gets your answers too. A reading you kept and
then left out in review without keeping again still teaches the reference,
so the check asks about it until you keep the readings again.

A box has no tick. It gets your answers in its text, and a box with no text
gets its text once your answers cover every character it shows. A box that
shows something you answered **Not a character** for still teaches it, and
the tile names the box: delete it, or draw it around the characters alone.
The check leaves out a box in whose picture AppyTappy finds no characters,
since no answer could be about it: fix that box in its row under **Boxes**.

You can still change any text by hand. An answer changes only the character
it is about, in the places that show it, and never a text you typed since.
When a text you typed gives a character another answer than you gave in the
check, the check asks about that character again.

A find that AppyTappy is still reading has no text to check. The last
screen says how many are still being read. Once its text arrives, it gets
the answers you gave already, **Not a character** included, and any other
character it shows joins the steps: go **Back** to them, or select
**Check characters** again. The same holds for a find or a reading that
first shows while the check is open.

### Check again after you find or record more

AppyTappy keeps your answers until you save or discard the reference. When
you select **Check characters** again, after **Find more like it** or a
recording has added text, the check asks only about the characters you have
not answered for yet. The new finds and readings get the answers you gave
already: a character AppyTappy misread again has the character you gave it,
and a find whose characters you all answered for is ticked. When nothing is
left to ask, the check says **Nothing new to check**.

A character is asked about again when you skipped its step, or when a text
you typed since gives it another character than your answer.

The last screen says how many characters were not asked about again.
**Check all again** there goes through every character once more, with your
answers in place, so that you can change one or take back
**Not a character**, which keeps the places it left out again.

### Check before you save

**Not confirmed yet** shows above **Save reference** while the reference
learns a character that is AppyTappy's guess: it is in a find you added, or
a reading you kept, with the text AppyTappy gave it, and nobody has
confirmed that character by itself. You confirm a character when you answer
for it in **Check characters**, or type it yourself, in that text or in
another one that shows the same shape.

Keeping a reading in review doesn't confirm its characters. That is why a
character can show here after **Found, not checked yet** under
[**Recording**](#record-readings) has let it go: review counts a reading you
keep as checked, and **Not confirmed yet** asks about each character.

**Check characters** there opens the check. You can also select
**Save reference** as it is: the offer never holds the save up.

## What a reference can't learn

A box's text has one character for each character the box shows, and
AppyTappy says so when the two differ:

- **The box shows fewer characters than you typed.** Characters that touch
  count as one, and a reference can't learn them.
- **The box shows more characters than you typed.** A character counts once
  for each of its marks that has blank space on both sides. Some percent
  signs are three such marks: a ring, a slash, and a ring. Type a character
  for each mark, such as `87o/o`; the number still reads as 87. Anything else
  in the text's color counts too, such as an icon or a frame's edge, so draw
  the box around the number alone.

AppyTappy takes the background from the pixels just around each box and
region, and the characters from the color inside them that splits into as
many characters as you typed. So a box whose text you typed as it shows
teaches the reference, whatever colors the text and its outline have, and
there is no color to pick or tolerance to set. A box drawn along the edge
of a button or a panel teaches the text on it, not the panel.

A reference reads one font. These it can't read, or reads less well:

- **Another font.** Its characters are other shapes. Give it its own
  reference, or teach this one boxes in that font too.
- **Characters that touch.** They count as one.
- **Text over a busy background,** such as a scene that shows through or a
  panel with a pattern. The reference can't tell the characters from what
  is behind them, and its boxes show more characters than you typed.
- **Rotated or slanted text** that it learned upright.
- **Very small text.** In text about 12 pixels tall, a `.` is only a pixel
  or two wide, and some characters can fail to read when the reference
  learned them at another size. Draw a box around a reading at the small
  size too.
- **Text in the background's own color** reads only with an outline more
  than a pixel wide around it.

A reference knows how tall a character is beside the others in its box, so
it tells a `.` from a `0` and a `,` from a `'`. A box that shows a sign
alone, such as `-`, teaches less than one that shows it beside digits.

When two colors in a region each show characters the reference knows, and
they say different text, the reference reads neither and says so. Draw the
region around the one number.

## Saved pictures it can't learn

A reference keeps a picture of every box and reading it learned from, and
learns them all again each time you save it. No screenshot shows those
pictures. A picture teaches the reference nothing when its file is missing
from the application's folder, or when it no longer splits into as many
characters as its text has.

The editor then lists each such picture under
**Saved pictures it can't learn**: the
picture, what you typed for it, and why the reference can't learn it. Its
**?** says the same in short. The reference can't be saved while it keeps
one of them: **Save reference** waits, and says
`Remove the saved pictures it can't learn first.` Do one of these:

- **Remove the picture.** Select **Remove** beside it. The reference loses
  only what that picture taught it. If
  [**Characters still missing**](#what-it-has-learned) then lists a
  character, draw a box around it on a screenshot. The editor counts the
  pictures you removed, and **Put back** brings them all back. Saving the
  reference deletes them.
  A reference needs at least one picture, box, or kept reading, so draw a box
  before you save one whose pictures you removed.

A value that reads with the reference can show the same problem, such as
`Percent font can't learn from its saved picture of "87%"`, or say that a
saved picture is missing from the application's folder. Select
**Edit Percent font** where the value picks its reference, and remove the
picture there.

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

1. **Start recording.** Draw a box around the characters first. Under **Record readings**, choose where to
   **Look in**: **This place**, or **The whole window**, which
   [Record the whole window](#record-the-whole-window) describes. Then
   select **Start recording**. The region turns red while it records.
2. **Use the application.** Go on in it until the region has shown the
   characters you need. AppyTappy reads each kept reading's text for you, and
   **Recording** says what that gives so far:
   - **Still missing** shows each character the value needs that nothing
     has shown yet. Make the application show these. It shows while you
     teach the reference for a value that
     [needs characters](Values.md#characters-a-number-needs).
   - **Found, not checked yet** shows the characters AppyTappy read in the
     kept readings. They are its guess until you check their readings in
     review: type in a reading's text, set its **Keep**, or select
     **Keep all** or the button that keeps them. **Keep all** counts every
     reading as checked, so read the texts first.
   - **Found** shows the characters the reference knows already, and the ones
     in readings you checked.
   - **Without text** counts the shapes that no reading's text names, such as
     while a reading is still being read.

   The newest kept readings show below. Moments when the region shows no
   characters, such as while a menu covers it, are skipped.
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
   without keeping, and your changes stay for next time. **Save reference**
   is off while review is open, since the reference has only the readings
   you kept: keep them to save them with the reference, or go **Back** to
   save without review's changes. To record more, keep
   the place it reads as it
   was, on a screenshot of the same window size: if you changed it,
   **Use its settings** puts it back. In a reference you teach by itself,
   that moves the box it recorded at back to where it was and to the top of
   the list, or adds an empty box there if you deleted it. Or **Discard**
   the recording to start a new one. A recording with readings asks first,
   since its readings go with any text you typed and any readings you kept
   in review: **Discard recording** drops them, and **Keep recording**
   leaves everything as it is.
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

You can change the window's size while you record. How the recording
follows depends on what you answered for the application under
**How often does this window change size?**:

- **Often.** Recording goes on. Once the window has kept its new size for a
  moment, the recorded place is scaled to it and keeps its distance from
  the side of the window the value stays at, or the side it is nearest when
  the reference is taught by itself, and a recording of the whole window looks
  for the text sizes scaled the same way. **Recording** then says so, such
  as **The window is 1280 × 720 now, so the place is scaled to it.** A
  character that looks the same at two window sizes is kept once.
  Recording pauses and says why in two cases. A recording of one place
  pauses when the scaled place would reach outside the window. A recording
  of the whole window still looks there, starting near the edge the place
  passed. Either pauses when the window is so small that the scaled text is
  under 5 pixels tall, too small to tell the characters apart. Recording
  goes on when you make the window larger. The window need not redraw for
  any of this: a window that shows nothing new at its new size is followed
  too.
- **Rarely.** Recording pauses at another size and says to resize the
  window. It goes on when the window is back at the size it had on the
  screenshot the recorded place is on.

Every screenshot with a box that teaches the reference, and the recording,
must have the size the reference reads at, and the reference says which one
does not. What a reference learned still reads at other sizes. If the window's capture fails for a moment, recording stops
and says why; select **Record more** once the window shows again. If the
window closes, nothing more can be recorded from it. Review and keep the
readings you want, then select **Save reference**, which still works after
the window closed: **Change window** drops everything you have not saved. To
record more, open the window again and **Edit** the reference.

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
for text in the color the text in that place has when the recording
starts, as tall as it shows on its screenshot, anywhere in the window, and reads each piece like
the place itself. While it records, the window shows an
outline around each piece it found the last time it looked, and **Recording
the whole window** says in how many places that was.

An application can show the same characters at more than one size, such as a
small counter and a large total. Each [box](#boxes) is one more example of a
size:

1. Before you start, draw a box around some text of the other size, on a
   screenshot of the same window size. The box needs no text typed in it to
   be an example, and a box you typed text in is one too.
2. Select **Start recording**. **Recording the whole window** says which
   sizes it looks for, such as
   **Looking for text of 2 sizes: 14 px and 28 px tall**. Two examples of
   nearly the same height count as one size.
3. In review, each reading says which size it is, such as
   **text 28 px tall**, when the recording looked for more than one. That
   is how tall the text is in the reading's picture. A reading recorded
   while the window had another size names the size it was scaled from too,
   such as **text 28 px tall, the 14 px size scaled**.

To add a size to a recording you stopped, draw a box around some text of
that size and select **Record more**. A recording keeps the sizes it has: to
drop one, **Discard** the recording and start a new one. A character that
looks the same at two sizes is kept once, so a second size adds readings only
for the shapes it draws differently.

It has limits, and review is where you deal with them:

- **It can pick up other text.** A menu's label or a name in the same color
  and size is kept too. While you teach the reference for a value that
  [needs characters](Values.md#characters-a-number-needs), a reading whose
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
- **It finds the sizes it has an example of.** Text much larger or smaller
  than every example's is not collected. What the reference learns still
  reads at other sizes.
- **It finds one color.** Text elsewhere in the window that turns another
  color while it records is not collected. The place the reference reads
  is read in any color.
- **It reads text by itself on a line.** Text that touches other things in
  the same color, or a line of more than 24 characters, is skipped.

**Record more** goes on looking where the recording looked before.

## How a reference was made

The References page says under a reference's name how it was made, unless
you made it by hand with boxes on screenshots you took:

- **Made with a recording.** A [recorded reading](#record-readings), or a
  box on a screenshot that a
  [recording of looks](Values.md#record-screenshots) kept, taught it.
- **Imported from** a file, with the file's name. The reference came with an
  application you [imported](Applications.md#share-an-application). It goes
  on saying so when you teach it more.

## Delete a reference

Select **Delete** next to a reference, then **Delete reference** to confirm.
A reference that a value reads with, at every size or in a fix at one size,
cannot be deleted: **Delete** says **Can't delete** with the reference's name
and the values to point at another reference first, in the reference's row,
and **OK** closes that.
