// Recording a value's region for its looks, and which of the session's screenshots can be removed.

import { type Draft, CHECKS, stateOn } from './check.ts';
import { clock } from './review.ts';

/** A screenshot in the strip: its index in the session, and whether a recording kept it. */
interface Listed {
  readonly index: number;
  readonly recorded: boolean;
}

/** Why `draft`'s region cannot be recorded for its looks yet, or null when it can. */
export function lookBlocked(draft: Draft): string | null {
  return draft.region === null || draft.drawnOn === null ? 'Draw the region on a screenshot to record it.' : null;
}

/**
 * How far, on average, the whole region may be from a look and still show that look, while `draft`'s region is
 * recorded. A state check's own tolerance as it is when the recording starts, the one that check compares a state's
 * whole region with; for a bar or a color, whose own tolerance compares colors, the tolerance a state check starts
 * with.
 */
export function lookTolerance(draft: Draft): number {
  return draft.kind === 'state' ? draft.tolerance : CHECKS.state.tolerance;
}

/**
 * Why screenshot `index` cannot be removed, or null when it can. None can while a recording of looks records, as
 * `recording` says: the recording knows each look it kept and would not keep a removed one again. Otherwise one
 * can when nothing set up uses it: `draft`'s region is not drawn on it, no state is taken from it, and no box is on
 * it.
 */
export function removeBlocked(draft: Draft, index: number, recording: boolean): string | null {
  if (recording) {
    return 'Stop recording to remove a screenshot.';
  }
  if (draft.drawnOn === index) {
    return 'The region is drawn on this screenshot. Draw it on another one to remove this one.';
  }
  if (stateOn(draft, index) !== -1) {
    return 'A state is taken from this screenshot. Remove the state to remove the screenshot.';
  }
  if (draft.boxes.some((box) => box.screenshot === index)) {
    return 'A box is on this screenshot. Delete its boxes to remove the screenshot.';
  }
  return null;
}

/**
 * The recorded screenshots among `shots` that nothing set up uses, which Remove all recorded removes once no
 * recording runs.
 */
export function removableRecorded(shots: readonly Listed[], draft: Draft): readonly number[] {
  return shots
    .filter((shot) => shot.recorded && removeBlocked(draft, shot.index, false) === null)
    .map((shot) => shot.index);
}

/**
 * The screenshots among `kept`, the ones a recording kept in the order it kept them, that `shots` still lists. One
 * is missing once it is removed, and for a moment before the strip hears of it.
 */
export function listedKept(kept: readonly number[], shots: readonly Listed[]): readonly number[] {
  return kept.filter((index) => shots.some((shot) => shot.index === index));
}

/**
 * `shots` with `shot`, in the order the session kept them, which is the order of their indexes. A recorded
 * screenshot can reach the strip after one taken later. A screenshot the strip already has is not added again.
 */
export function withShot<Shot extends Listed>(shots: readonly Shot[], shot: Shot): readonly Shot[] {
  if (shots.some((each) => each.index === shot.index)) {
    return shots;
  }
  return [...shots, shot].toSorted((left, right) => left.index - right.index);
}

/** How many screenshots a recording kept, such as "1 screenshot". */
export function keptText(count: number): string {
  return count === 1 ? '1 screenshot' : `${count} screenshots`;
}

/**
 * What the stopped recording that kept `kept` screenshots in `elapsedMs` left: how many it kept, or once some are
 * removed how many of them the strip still lists, `listed`.
 */
export function keptSummary(kept: number, listed: number, elapsedMs: number): string {
  return listed === kept
    ? `The last recording kept ${keptText(kept)} in ${clock(elapsedMs)}.`
    : `${listed} of the last recording's ${keptText(kept)} ${listed === 1 ? 'is' : 'are'} still in the strip.`;
}
