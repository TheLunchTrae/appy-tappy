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

/** What a reference is. */
export const REFERENCE_HINT =
  'A reference knows what each character looks like in one font and color, learned from screenshots you label. Values that show the same font can share one.';

/** A reference's characters' color. */
export const INK_HINT =
  'Pick a pixel inside a character, not on its edge. Pixels within the tolerance of this color count as part of the characters.';

/** A screenshot's label in the reference editor. */
export const LABEL_HINT =
  'Type exactly what the region shows, such as 87% or 1,250. Each labeled screenshot teaches the reference the characters in it.';

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
