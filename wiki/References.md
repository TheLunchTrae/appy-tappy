# References

A reference is how AppyTappy knows an application's characters: their color,
and the shape of each digit and sign. A [Number](Values.md#checks) value reads
its region with one of its application's references, so several values in the
same font share one reference, and teaching it a new digit teaches all of
them.

The **References** page lists each application's references, with the
characters each knows and the values that read with it.

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
   Turn on **Show matching pixels** to tint every pixel on the screenshot
   that counts as part of the characters, and watch the tint as you drag
   **Tolerance**. Raise it until the characters are tinted whole, and lower it
   if the background around them is tinted too. Under the switch, AppyTappy
   counts the tinted pixels in the region and on the rest of the screenshot.
   The tint is only a guide, as
   [Show matching pixels](Values.md#show-matching-pixels) describes.
3. **Teach it the characters.** Draw a region around text on a screenshot,
   then type what it shows, such as `87%`. AppyTappy learns the shape of each
   character from what you type. Label screenshots until
   **Characters it knows** shows every digit the numbers need, or
   [record the region](#record-readings) to collect them as they show.
4. **Save.** Select **Save reference**.

To teach a saved reference more, select **Edit** next to it on the References
page, or **Edit** while you pick it for a value. What it learned before stays;
new labels and readings add to it. Changing its color or tolerance changes how
every value that reads with it reads, and the editor names them.

## Record readings

Typing what a counter shows on screenshot after screenshot is slow when it
shows many values. A reference can record its region instead, while you use
the application, and keep each different reading it shows.

1. **Start recording.** Draw the region and pick the **Characters' color**
   first. Then select **Start recording** under **Record readings**. The
   region turns red while it records.
2. **Use the application.** Go on in it until the region has shown the
   characters you need, such as every digit. Each different reading is kept
   once, with the time it first showed, and AppyTappy reads its text for you.
   Readings appear under **Recording** as they are found. Moments when the
   region shows none of the characters' color, such as while a menu covers
   it, are skipped.
3. **Stop and review.** Come back and select **Stop and review**. Each
   reading shows with the text AppyTappy read. Fix any text it got wrong, and
   clear **Keep** on any reading that is not the value, such as a paused
   screen. A reading whose text gives one shape two different characters,
   such as a `1` typed as `l`, is flagged: fix it or leave it out.
4. **Keep them.** Select the button that says how many you keep, such as
   **Keep 12 readings**. The kept readings teach the reference like labeled
   screenshots. **Review recording** opens them again with your choices, and
   **Record more** goes on with the same recording. **Back** leaves review
   without keeping, and your changes stay for next time. To record more, keep
   the region, the characters' color, and the tolerance as they were, on a
   screenshot of the same window size: if you changed them, **Use its
   settings** puts them back. Or **Discard** the recording to start a new one.
   Changing window also discards it.

Keep the window the size it was when you took the screenshot you drew the
region on: at another size, recording pauses and says to resize the window.
Every screenshot you label, and the recording, must have that size too, and
the reference says which one does not. If the window's capture fails for a
moment, recording stops and says why; select **Record more** once the window
shows again. If the window closes, nothing more can be recorded from it.
Review and keep the readings you want, then select **Save reference**, which
still works after the window closed: **Change window** drops everything you
have not saved. To record more, open the window again and **Edit** the
reference. One recording keeps at most 200 readings.

If AppyTappy cannot read a reading's text, review leaves its text empty for
you to type.

## Delete a reference

Select **Delete** next to a reference, then confirm. A reference that a value
reads with, at every size or in a fix at one size, cannot be deleted:
AppyTappy names the values to change or delete first.
