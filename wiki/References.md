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
4. **Teach it the characters.**
   [Draw a region](Values.md#draw-the-region) around text on a screenshot,
   then type what it shows, such as `87%`. AppyTappy learns the shape of each
   character from what you type. Label screenshots until
   **Characters still missing** says it knows every character it needs,
   [find more like it](#find-more-like-it) on the same screenshot, or
   [record the region](#record-readings) to collect them as they show.
5. **Save.** Select **Save reference**. A reference with characters still
   missing saves too: the editor says so next to the button, and you can
   teach them later. To finish it later instead, select **Save as draft**
   ([Drafts](Drafts.md)). A draft doesn't keep a recording.

The box at the bottom of the panel shows what the reference reads on the
live window, a few times a second. When it can't read the window, it says why
instead, in the words
[A number reads in setup but the automation does nothing](Troubleshooting.md#a-number-reads-in-setup-but-the-automation-does-nothing)
explains. That box is the reference's own reading of the region you drew, which
[scales with the window](Values.md#window-size). A reference you make for a
number that reads only some characters reads only those here too. The
automation reads with the same reference, where its value's region is at the
window's size.

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
you type labels, add found text, keep recorded readings, or correct them:

- **Characters still missing** shows each needed character it has yet to
  learn, outlined. This is what is left to teach. Once it has them all, it
  says so.
- **Characters it knows** shows the needed characters it learned. Characters
  it learned that are not on the list show below them. They are fine to keep:
  the list says what you need, not what is allowed.

The list only guides you. AppyTappy learns a character from a label, found
text you add, or a kept reading, never because it is on the list, and a number reads the same way
whatever you turn on. [Checks](Values.md#checks) says how a number reads its
`.` and `,`.

A reference saved without a list shows every character it knows under
**Characters it knows**, and nothing as missing. **Edit** it and turn on what
it needs to see what is left to teach.

## Find more like it

A screen often shows more numbers in the same font than the one you drew the
region around, such as a stats panel. After you draw the region and pick the
**Characters' color**, select **Find more like it** under the text you typed.
AppyTappy looks on the screenshot for other text in that color and as tall as
the text in the region, and reads each as text.

1. **Check what it found.** Each find shows numbered on the screenshot and in
   the list, with the text AppyTappy read. Fix any text it got wrong, and clear
   the box beside any find that is not in the font, such as a label. When
   AppyTappy reads a letter while the reference has learned none, such as a `0`
   read as `O`, the find is flagged until you type its text, and so is text
   that gives one shape two different characters: fix it or leave it out.
2. **Add them.** Select the button that says how many you add, such as
   **Add 3 to the reference**. They teach the reference like labeled
   screenshots, and **Characters it knows** outlines the characters only the
   finds would teach until you add them. Until then they still count under
   **Characters still missing**. **Not these** closes the list without
   adding any, and **Remove found text** takes the added finds back out.

Draw the region tightly around the text first, since the finds' characters
must be about as tall as the text in it; a comma or a mark below them does not
count. Text of another size is skipped, and the list says how much. One search
lists at most 12 finds; add those, then select **Find more like it** again, and
it lists the rest, leaving out what you added.

## What a reference can't learn

A label has one character for each character the region shows, and AppyTappy
says so when the two differ:

- **The region shows fewer characters than you typed.** Characters that touch
  count as one, and a reference can't learn them.
- **The region shows more characters than you typed.** A character counts once
  for each of its marks that has blank space on both sides. Some percent
  signs are three such marks: a ring, a slash, and a ring. Type a character
  for each mark, such as `87o/o`; the number still reads as 87. Anything else
  in the characters' color counts too, so draw the region around the number
  alone.

A reference knows a character as it looked beside the others in the region.
A sign that reaches above or below the digits, such as the `,` in `1,234`,
changes how every character beside it looks to AppyTappy. Label readings
that show the sign until they show every digit, and readings without it
too.

Small text takes more labels. Where the characters are less than about 16
pixels tall, label several readings of each character, or
[record](#record-readings) them.

## Record readings

Typing what a counter shows on screenshot after screenshot is slow when it
shows many values. A reference can record its region instead, while you use
the application, and collect the characters it shows. While you make a
**Number** value, **Record readings** under **How to read it** opens this
recording on the value's region. The other checks
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
cannot be labeled, take a screenshot that shows the character without the
stray shape and label that instead.

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
a reference cannot learn it as one character:
[What a reference can't learn](#what-a-reference-cant-learn) says what to
type for it.

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
labels for you to check. It doesn't show what the reference reads: the box
at the bottom of the panel does, on the live window.

To remove a screenshot, select it, then select **Remove screenshot** with its
number. One the region is drawn on, or that you labeled, stays: draw the
region on another screenshot or clear its label first.

## Delete a reference

Select **Delete** next to a reference, then confirm. A reference that a value
reads with, at every size or in a fix at one size, cannot be deleted:
AppyTappy names the values to change or delete first.
