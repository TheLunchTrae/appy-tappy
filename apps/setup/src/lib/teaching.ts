// Rules of the reference editor on the capture screen: what its buttons wait for.

/** What Save reference depends on. */
export interface ReferenceSave {
  /** Whether the reference has a name. */
  readonly named: boolean;
  /** Whether its characters build: boxes with text, or kept readings, in a color. */
  readonly built: boolean;
  /** Whether the live window is read with the characters as they are now. */
  readonly tried: boolean;
  /** Whether a save, of it or of its draft, is under way. */
  readonly busy: boolean;
  /** Whether a recording's review is open, under the character check or not. */
  readonly reviewing: boolean;
  /** How many of its saved pictures it can't learn as it finds its characters now. */
  readonly unlearned: number;
}

/** What Save reference says while the reference keeps saved pictures it can't learn. */
export const SAVE_AFTER_UNLEARNED = "Remove the saved pictures it can't learn first.";

/** What Save reference says while review is open. */
export const SAVE_AFTER_REVIEW =
  "Keep the readings to save them with the reference, or go back to the screenshot to save without review's changes. Saving ends the recording.";

/** What a reference's editor and its draft say while it has no name. */
export const NAME_THE_REFERENCE = 'Name the reference.';

/**
 * Whether the reference can be saved now, and what the editor says when it waits. Saving ends the
 * recording, and the reference learns the readings the last Keep put on it: what review changed since, and what a
 * character check opened from review answered, would go with the recording. So while review is open the save waits
 * for Keep, or for the way back to the screenshot. A reference that keeps a saved picture it can't learn would be
 * refused, so its save waits until the picture is removed or learned again, and says so. A reference without a name
 * says so too. The reading box says what the characters still need and why a try failed, and a save under way ends
 * by itself, so those waits say nothing here.
 */
export function referenceSave(save: ReferenceSave): { readonly can: boolean; readonly waits: string | null } {
  const ready = save.built && save.tried && !save.busy;
  let waits: string | null = null;
  if (save.reviewing) {
    waits = SAVE_AFTER_REVIEW;
  } else if (save.unlearned > 0) {
    waits = SAVE_AFTER_UNLEARNED;
  } else if (!save.named) {
    waits = NAME_THE_REFERENCE;
  }
  return { can: ready && waits === null, waits };
}

/**
 * The label of the button that closes the character check: it goes back to the recording's review it was opened
 * from while one is open, `reviewing`, and to the screenshot otherwise.
 */
export function checkBackLabel(reviewing: boolean): string {
  return reviewing ? 'Back to review' : 'Back to the screenshot';
}
