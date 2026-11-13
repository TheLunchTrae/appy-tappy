// What the ? beside a field explains, for the fields whose name and note leave something out.

import { LIMITS } from './bindings.ts';
import { shortcutKeysInWords } from './shortcut.ts';

/** The condition: a value and what it has to be. */
export const WHEN_HINT =
  "Pick a value and what it has to be. The automation runs once each time this becomes true, and only while the value's application is open. Is below and is above leave the number itself out; is at most and is at least count it.";

/** The actions and what media and volume actions change. */
export const ACTIONS_HINT =
  "The actions run one after another, from the top. Media and volume actions change whatever is playing or the whole system, unless you pick One application and name its executable, such as Player.exe. For a browser's media, pick Whatever is playing, since a browser can report another name.";

/** What undo can and cannot put back. */
export const UNDO_HINT =
  'Skipped tracks, pressed keys, opened programs, and played sounds stay as they are. With undo on, AppyTappy also puts things back when the application closes, when automations are turned off from the tray icon, and when AppyTappy quits.';

/** The wait before an automation can run again. */
export const COOLDOWN_HINT =
  'Counted from when it last ran. If the condition becomes true during the wait and is still true when the wait ends, it runs then. 0 means no wait.';

/** The application an automation needs running. */
export const NEEDS_RUNNING_HINT =
  'The application whose value the automation watches. The automation runs only while that application is open.';

/** The anti-cheat setting. */
export const ANTI_CHEAT_HINT =
  'Anti-cheat can treat keys another program presses as cheating, so AppyTappy presses keys only into an application set to None. Reading the screen, media, volume, opening, and sounds work whatever this says.';

/** The overlay menu. */
export const OVERLAY_HINT =
  "Shows the application's values as AppyTappy reads them, while AppyTappy Setup is open. Over the window sits in the window's top-left corner. Picture in picture is a small panel that stays on top of every window.";

/** The kinds of check a value can make. */
export const CHECK_HINT =
  "Number reads digits, such as a score, with a reference. Bar fill reads how full a bar is, from 0 to 100. Color is yes while the region's average color is close to one you pick. State reads which of a few named looks the region shows.";

/** Editing a saved value, beside a fix of it at one window size. */
export const EDIT_HINT =
  'An edit changes the value as it was first drawn, which every window size reads or scales from. A fix leaves that alone and draws the value again at one other size.';

/** The region's typed pixels. */
export const REGION_HINT =
  "X and Y are the region's top-left corner, counted in pixels from the window's top-left corner. Drawing on a screenshot fills these in; type here to nudge it.";

/** The tolerance's scale for a check's colors. */
export const TOLERANCE_HINT = `From 0, an exact match, up to ${LIMITS.maxTolerance}, which matches any color. Raise it if the check misses; lower it if it matches when it shouldn't.`;

/** The tolerance's scale for a reference's characters' color. */
export const INK_TOLERANCE_HINT = `From 0, only the exact color, up to ${LIMITS.maxTolerance}, which counts every pixel. Raise it if parts of characters go missing; lower it if the background counts as characters.`;

/** A bar fill's two colors. */
export const BAR_COLORS_HINT =
  "Pick a filled part of the bar for Filled color and an empty part for Empty color. Borders and labels inside the region don't count.";

/** Where a recording of readings looks for characters. */
export const RECORD_SCOPE_HINT =
  "This place records only where the reference reads: its first box, or the value's region. The whole window also records text in the characters' color and as tall as the text there, wherever the window shows it, so a stats panel or a second counter in the same font teaches the reference too. When the window shows the characters at another size as well, draw a box around some text of that size first: each box is one more example of a size to look for. It looks over the window twice a second, so text that flashes by faster can be missed, and it can pick up other text of that color and size, which you leave out in review.";

/** What the sizes of text are that a recording of the whole window looks for. */
export const TEXT_SIZES_HINT =
  'A recording of the whole window looks for text about as tall as an example: the text where the reference reads, and the text in each box you drew. Two examples of nearly the same height are one size. To collect another size, stop, draw a box around some text of that size, and select Record more. A character that looks the same at two sizes is still kept once.';

/** What a recording does when its window changes size. */
export const RECORD_RESIZED_HINT =
  "This application's window changes size often, so a recording goes on when it does. Once the window has kept its new size for a moment, the place is scaled to it and keeps its distance from the side it stays at, and the whole window is searched for text at the scaled sizes. A character that looks the same at two window sizes is kept once. A recording for an application whose window rarely changes size waits for the size its place was drawn at instead.";

/** What a reference is. */
export const REFERENCE_HINT =
  'A reference knows what each character looks like in one font and color, learned from the boxes you draw on screenshots and the readings you record. Values that show the same font can share one.';

/** The characters of its reference that a number value reads. */
export const CHARACTERS_READ_HINT =
  'A reference can know a whole font. Reading only the characters this value shows stops it mistaking one character for a lookalike, such as 0 for O. The reference keeps every character, so other values can read more of it.';

/** A reference's characters' color. */
export const INK_HINT =
  'Pick a pixel inside a character, not on its edge. Pixels within the tolerance of this color count as part of the characters.';

/** The boxes a reference learns from. */
export const BOXES_HINT =
  'Drag on any screenshot to draw a box tightly around some characters, then type exactly what it shows, such as 87% or 1,250. Draw as many as you like, several on one screenshot too. Press a box to select it; drag it to move it, drag a corner to resize it, or press Delete to remove it. Each box with text teaches the reference the characters in it. A reference you teach by itself reads and records at the box marked Reads here, the first in the list.';

/** The characters a reference needs. */
export const EXPECTED_HINT =
  'Your list of what to teach. AppyTappy shows which of these characters the reference has yet to learn. It only guides you: a character is learned from the boxes you type and the readings you keep, never from this list, and a number reads the same way whatever you turn on.';
/** Finding more text like the selected box. */
export const FIND_LIKE_HINT =
  "Looks on the selected box's screenshot for other text in the characters' color and as tall as the text in the box, and reads each as text. It lists only text that shows the characters the reference needs. Each one you tick and add becomes a box. Draw the box tightly around the text and type what it shows first, so the size is right.";

/** What the found text's list shows, and what each row's box and field do. */
export const FOUND_HINT =
  'Each find listed is outlined on the screenshot under the same number, with what the text reader made of it. A find that is skipped keeps its number, so the numbers can have gaps. Tick the ones to add, or Select all, and fix any text it misread. A flagged one needs its text typed, or leaving out, before you can add them; Deselect flagged leaves out every flagged one at once.';

/** What Check characters steps through, and what an answer changes. */
export const CHECK_CHARACTERS_HINT =
  'Goes through the characters one at a time, across the boxes, the finds listed, and the readings a recording keeps. Each step shows one character and every place it appears, and asks once whether that is what it is. Your answer fixes the text of every place that shows it, so a character the text reader misread the same way everywhere needs one fix. You can still change any text by hand.';

/** The keys that answer a step of Check characters. */
export const CHECK_KEYS_HINT =
  'Enter presses the highlighted button: Yes, Next, or Done. To fix a shape, type the right character: it goes in It is under the first shape not checked yet, and Enter there makes it your answer. Alt+S skips the step, Alt+B goes back, and Escape closes the check. A text field or a button that has focus keeps Enter for itself.';

/** Why the reference editor offers Check characters before a save, and that the save does not wait for it. */
export const CHECK_OFFER_HINT =
  'AppyTappy read these in text you added or kept without typing it, and keeping a text does not confirm its characters. It tends to misread a character the same way everywhere, and a reference that learns one wrong reads its values wrong. Check characters asks about each once. Saving does not wait for it.';

/** What Reread reads again, and when that changes anything. */
export const REREAD_HINT =
  "Has the text reader read again the finds it could not read, and the flagged ones whose text you have not typed, with the characters' color and tolerance as they are set now. Your ticks and the text you typed stay. The same color and tolerance read the same text, so Reread waits for you to change one of them, unless the text reader could not read a find at all.";

/** The read interval's two waits. */
export const READING_HINT = `AppyTappy reads a value only when its window draws something new. It waits the fastest read while a value changes, and up to the slowest read while it holds still. Each is ${LIMITS.fastestReadMs} to ${LIMITS.slowestReadMs} ms.`;

/** Which shortcuts the screenshot shortcut can be. */
export const SHORTCUT_HINT = `${shortcutKeysInWords()} Change shortcut on the screenshot page changes the same setting.`;

/** What the runtime trace is for. */
export const TRACE_HINT =
  'Useful when something reads or runs slower than expected. The files stay on this computer, in the trace folder.';

/** What marking an application's values as checked does. */
export const MARK_CHECKED_HINT =
  'Records the build of the application running now, so the Values page tells you again only when it changes.';

/** Saving an unfinished item as a draft. */
export const DRAFT_HINT =
  "A draft keeps what you set up so you can finish it later. It never runs and isn't part of what AppyTappy reads until you finish and save it.";

/** The build a shared file's values were checked against. */
export const SHARED_CHECKED_HINT =
  'The build of the application that whoever made this file checked its values against. An update can move or restyle what a value reads, so the Values page warns while you run another build.';

/** How a shared file's definition was made. */
export const ORIGIN_HINT =
  'How whoever made this file got its values: drawn and labeled by hand, taught with a recording, or imported from a file of someone else.';

/** Confirming the anti-cheat setting of an imported application. */
export const IMPORT_ANTI_CHEAT_HINT =
  'AppyTappy presses keys only into an application set to None, and anti-cheat can treat those keys as cheating. A file from someone else could say None for an application that has anti-cheat, so the file never sets this: you do. You can change it later on this page.';
