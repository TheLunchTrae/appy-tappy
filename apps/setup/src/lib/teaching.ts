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
  /** Whether it needs more characters than one reference saves. */
  readonly overLimit: boolean;
  /** Whether a recording's review is open, under the character check or not. */
  readonly reviewing: boolean;
}

/** What Save reference says while review is open. */
export const SAVE_AFTER_REVIEW =
  "Keep the readings to save them with the reference, or go back to the screenshot to save without review's changes. Saving ends the recording.";

/**
 * Whether the reference can be saved now, and what the editor says when it waits for review. Saving ends the
 * recording, and the reference learns the readings the last Keep put on it: what review changed since, and what a
 * character check opened from review answered, would go with the recording. So while review is open the save waits
 * for Keep, or for the way back to the screenshot.
 */
export function referenceSave(save: ReferenceSave): { readonly can: boolean; readonly waits: string | null } {
  const ready = save.named && save.built && save.tried && !save.busy && !save.overLimit;
  return { can: ready && !save.reviewing, waits: save.reviewing ? SAVE_AFTER_REVIEW : null };
}

/**
 * Why the value's capture changes no saved reference `chosen`, or null when it may: one of the `drafted`, the
 * references that have a draft of changes, is changed by finishing its draft, which would undo what was changed
 * apart from it. It holds for every way into the reference's editor from a value: Edit, and Record readings.
 */
export function draftHolds(
  chosen: { readonly id: string; readonly name: string },
  drafted: ReadonlySet<string>,
): string | null {
  return drafted.has(chosen.id)
    ? `${chosen.name} has a draft of changes. Finish it on the References page to change it.`
    : null;
}
