// Recording a value's region for its looks, and which of the session's screenshots can be removed.

import { type Draft, type Drawn, DEFAULT_TOLERANCE, stateOn } from './check.ts';

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
 * How far, on average, the region may be from a look and still show that look, while `draft`'s region is recorded.
 * A state check's own tolerance, so a recording keeps the looks that check can tell apart; for a bar or a color,
 * whose own tolerance compares colors, the tolerance a state check starts with.
 */
export function lookTolerance(draft: Draft): number {
  return draft.kind === 'state' ? draft.tolerance : DEFAULT_TOLERANCE.state;
}

/**
 * Why screenshot `index` cannot be removed, or null when nothing set up uses it: `draft`'s region is not drawn on
 * it, no state is taken from it, and it has no label. `value` is the value's region while the reference editor is
 * open over it, which comes back when the editor closes.
 */
export function removeBlocked(draft: Draft, value: Drawn | null, index: number): string | null {
  if (draft.drawnOn === index || value?.drawnOn === index) {
    return 'The region is drawn on this screenshot. Draw it on another one to remove this one.';
  }
  if (stateOn(draft, index) !== -1) {
    return 'A state is taken from this screenshot. Remove the state to remove the screenshot.';
  }
  if ((draft.labels.get(index) ?? '').trim() !== '') {
    return 'This screenshot is labeled. Clear what it shows to remove it.';
  }
  return null;
}

/** The recorded screenshots among `shots` that can be removed, which Remove all recorded removes. */
export function removableRecorded(shots: readonly Listed[], draft: Draft, value: Drawn | null): readonly number[] {
  return shots
    .filter((shot) => shot.recorded && removeBlocked(draft, value, shot.index) === null)
    .map((shot) => shot.index);
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
