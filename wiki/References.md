# References

A reference is how AppyTappy knows an application's characters: their color,
and the shape of each digit and sign. A [Number](Values.md#checks) value reads
its region with one of its application's references, so several values in the
same font share one reference, and teaching it a new digit teaches all of
them.

The **References** page lists each application's references, with the
characters each knows and the values that read with it. A reference that has
yet to learn characters it needs lists them after **Missing**, and so does
the list of references you pick from for a Number value.

A reference reads its characters at any size, so a number drawn larger than
the ones it learned from still reads.

## Make a reference

Select **New reference** on the References page and pick the application's
window, or select **New reference from this region** while you
[make a value](Values.md#make-a-value).

1. **Name it,** such as `Percent font`.
2. **Pick the color.** Next to **Characters' color**, select **Pick**, then
   click the characters on a screenshot. **Tolerance** says how far a pixel's
   color may be from it and still count: drag the slider or type a number.
3. **Choose the characters it needs.** Under **Characters it needs**, turn on
   what the numbers can show. A new reference starts with **Digits** on.
   [Choose what it needs](#choose-what-it-needs) describes the choices.
4. **Teach it the characters.** Draw a region around text on a screenshot,
   then type what it shows, such as `87%`. AppyTappy learns the shape of each
   character from what you type. Label screenshots until
   **Characters still missing** says it knows every character it needs, or
   [record the region](#record-readings) to collect them as they show.
5. **Save.** Select **Save reference**. A reference with characters still
   missing saves too: the editor says so next to the button, and you can
   teach them later.

To teach a saved reference more, select **Edit** next to it on the References
page, or **Edit** while you pick it for a value. What it learned before stays;
new labels and readings add to it. Changing its color or tolerance changes how
every value that reads with it reads, and the editor names them.

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
you type labels, keep recorded readings, or correct them:

- **Characters still missing** shows each needed character it has yet to
  learn, outlined. This is what is left to teach. Once it has them all, it
  says so.
- **Characters it knows** shows the needed characters it learned. Characters
  it learned that are not on the list show below them. They are fine to keep:
  the list says what you need, not what is allowed.

The list only guides you. AppyTappy learns a character from a label or a kept
reading, never because it is on the list, and a number reads the same way
whatever you turn on. [Checks](Values.md#checks) says how a number reads its
`.` and `,`.

A reference saved without a list shows every character it knows under
**Characters it knows**, and nothing as missing. **Edit** it and turn on what
it needs to see what is left to teach.

## Record readings

Typing what a counter shows on screenshot after screenshot is slow when it
shows many values. A reference can record its region instead, while you use
the application, and collect the characters it shows.

A recording keeps a reading when it shows a character shape that no kept
reading has. A counter that runs from `0` to `100` shows about a hundred
readings but only eleven shapes, so it leaves about ten readings to review.
Every other reading is counted, not kept.

1. **Start recording.** Draw the region and pick the **Characters' color**
   first. Then select **Start recording** under **Record readings**. The
   region turns red while it records.
2. **Use the application.** Go on in it until the region has shown the
   characters you need. AppyTappy reads each kept reading's text for you, and
   **Recording** says what that gives so far:
   - **Still missing** shows each character the reference needs that nothing
     has shown yet. Make the application show these. It shows when you
     [chose what the reference needs](#choose-what-it-needs).
   - **Found, not checked yet** shows the characters AppyTappy read in the
     kept readings. They are its guess until you keep their readings in
     review.
   - **Found** shows the characters the reference knows already, and the ones
     in readings you kept.
   - **Without text** counts the shapes that no kept reading's text names,
     such as while a reading is still being read.

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
   - **Adds nothing** marks a reading whose characters other kept readings
     all show. It starts with **Keep** cleared, and you can leave it out.
   - A reading whose text gives one shape two different characters, such as
     a `1` typed as `l`, is flagged: fix it or leave it out.

   The same lists as while recording show above the readings, and follow
   what you type and keep.

4. **Keep them.** Select the button that says how many you keep, such as
   **Keep 12 readings**. The kept readings teach the reference like labeled
   screenshots. **Review recording** opens them again with your choices, and
   **Record more** goes on with the same recording. **Back** leaves review
   without keeping, and your changes stay for next time. To record more, keep
   the region, the characters' color, and the tolerance as they were, on a
   screenshot of the same window size: if you changed them, **Use its
   settings** puts them back. Or **Discard** the recording to start a new one.
   Changing window also discards it.

A digit, a sign such as `%` or `.`, and a letter are collected the same way.
One character can show as more than one shape, and the recording keeps each:
an edge that blends with the background differently, or a digit beside a `,`
that reaches below the line, is another shape of the same character. Type the
same character for each. A sign that the application draws as separate marks
side by side, with a gap between them, counts as one shape for each mark, and
a reference cannot learn it as one character.

Keep the window the size it was when you took the screenshot you drew the
region on: at another size, recording pauses and says to resize the window.
Every screenshot you label, and the recording, must have that size too, and
the reference says which one does not. What a reference learned still reads
at other sizes. If the window's capture fails for a moment, recording stops
and says why; select **Record more** once the window shows again. If the
window closes, nothing more can be recorded from it. Review and keep the
readings you want, then select **Save reference**, which still works after
the window closed: **Change window** drops everything you have not saved. To
record more, open the window again and **Edit** the reference.

A recording is full once it has found 256 different shapes, as many as a
reference learns, and says so. That many shapes mean the region shows more
than the value, such as a picture behind it: draw the region around the value
alone and record again. A recording is also full once it has counted more
than a million different readings, which takes hours of a timer that changes
many times a second.

If AppyTappy cannot read a reading's text, review leaves its text empty for
you to type.

## Delete a reference

Select **Delete** next to a reference, then confirm. A reference that a value
reads with, at every size or in a fix at one size, cannot be deleted:
AppyTappy names the values to change or delete first.
