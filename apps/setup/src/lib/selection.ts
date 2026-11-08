// Selecting the thing a state check looks for: the steps the user takes on
// a screenshot, and which pixels of the region the selection leaves out.

import { type Rect, bounding, sameRect } from './region.ts';

/** How the user marks the region: by hand, by snapping a rough box, or by clicking the thing. */
export type SelectTool = 'box' | 'snap' | 'click';

/** The tools, in the order the capture screen offers them. */
export const SELECT_TOOLS: readonly SelectTool[] = ['box', 'snap', 'click'];

/** What each tool is called on screen, and a few words on how it works. */
export const TOOL_NAMES: Readonly<Record<SelectTool, { readonly name: string; readonly hint: string }>> = {
  box: { name: 'Box', hint: 'Draw it exactly' },
  snap: { name: 'Snap', hint: 'Rough box' },
  click: { name: 'Click', hint: 'Click the thing' },
};

/** How a step's pick combines with the selection before it. */
export type Combine = 'replace' | 'add' | 'remove';

/** One thing the user did to select. */
export type SelectStep =
  | { readonly kind: 'snap'; readonly region: Rect; readonly combine: Combine }
  | { readonly kind: 'click'; readonly x: number; readonly y: number; readonly combine: Combine };

/** The steps the setup app replays on the screenshot a state is taken from. */
export interface SelectionSpec {
  /** How far a color may be from another and still count as alike. */
  readonly tolerance: number;
  readonly steps: readonly SelectStep[];
}

/** A selection as the setup app answers it. */
export interface SelectionView {
  /** The box around the selected pixels. */
  readonly region: Rect;
  /** The runs of selected pixels in the box, each from its first pixel's index to the one after its last. */
  readonly spans: readonly (readonly [number, number])[];
}

/** A selection made for a state's steps, at a color match. */
export interface Selection {
  readonly steps: readonly SelectStep[];
  readonly colorMatch: number;
  readonly view: SelectionView | null;
}

/** What a state's selection leaves of the region. */
export interface Masking {
  /** Whether its last steps selected anything. */
  readonly found: boolean;
  /** How many of the region's pixels are compared, or null until its selection is made. */
  readonly kept: number | null;
}

/** Where a selection is made: the screenshot it is taken from, and the steps taken on it. */
interface Selecting {
  readonly screenshot: number;
  readonly steps: readonly SelectStep[];
}

/** The selection made for `state`'s steps at `colorMatch`, or null while none is. */
export function madeFor(
  selections: ReadonlyMap<number, Selection>,
  state: Selecting,
  colorMatch: number,
): Selection | null {
  const made = selections.get(state.screenshot);
  return made !== undefined && made.steps === state.steps && made.colorMatch === colorMatch ? made : null;
}

/** The box the region was last fitted to, and the steps it came from. */
export interface Fitted {
  readonly steps: ReadonlySet<readonly SelectStep[]>;
  readonly region: Rect;
}

/**
 * The fit after `made`, the selections every state has now, when the region was last fitted as `last` and shows
 * `shown`, and the region to show, or null to leave it. Steps that select something new refit the region even over
 * a hand edit; steps that select nothing, or a new color match for steps seen before, refit only a region the user
 * has not changed.
 */
export function refit(
  made: readonly Selection[],
  last: Fitted | null,
  shown: Rect | null,
): { readonly fitted: Fitted | null; readonly region: Rect | null } {
  const region = bounding(made.flatMap((selection) => (selection.view === null ? [] : [selection.view.region])));
  const fresh = made.some((selection) => selection.view !== null && last?.steps.has(selection.steps) !== true);
  if (region !== null && (last === null || fresh || sameRect(shown, last.region))) {
    return { fitted: { steps: new Set(made.map((selection) => selection.steps)), region }, region };
  }
  if (last === null) {
    return { fitted: null, region: null };
  }
  // The region stays, but these steps are no longer new to it.
  const steps = new Set([...last.steps, ...made.map((selection) => selection.steps)]);
  return { fitted: { steps, region: last.region }, region: null };
}

/** The color match a selection starts with. */
export const DEFAULT_COLOR_MATCH = 24;

/** The combine a pointer press asks for: Shift adds, Alt takes away, and a plain press starts over. */
export function combineOf(keys: { readonly shiftKey: boolean; readonly altKey: boolean }): Combine {
  if (keys.altKey) {
    return 'remove';
  }
  return keys.shiftKey ? 'add' : 'replace';
}

/**
 * The steps after `step`, taken on screenshot `on`, when the earlier ones
 * were taken on `before`. A step that starts over, or one on another
 * screenshot, begins a new selection. Taking away from nothing changes
 * nothing, so it is `null`.
 */
export function withStep(
  steps: readonly SelectStep[],
  before: number | null,
  on: number,
  step: SelectStep,
): readonly SelectStep[] | null {
  const fresh = steps.length === 0 || before !== on;
  if (step.combine === 'remove' && fresh) {
    return null;
  }
  if (step.combine === 'replace' || fresh) {
    return [{ ...step, combine: 'replace' }];
  }
  return [...steps, step];
}

/** Which pixels of the selection's own box it selects, one entry for each, row by row. */
function selected(view: SelectionView): Uint8Array {
  const kept = new Uint8Array(view.region.width * view.region.height);
  for (const [start, end] of view.spans) {
    kept.fill(1, start, end);
  }
  return kept;
}

/**
 * Which pixels of `region` the selection leaves out, one entry for each,
 * row by row: 1 for a left-out pixel. Pixels of `region` outside the
 * selection's box are left out too, as after the user widens the box by hand.
 */
export function ignoredPixels(view: SelectionView, region: Rect): Uint8Array {
  const kept = selected(view);
  const ignored = new Uint8Array(region.width * region.height).fill(1);
  const box = view.region;
  for (let row = 0; row < region.height; row += 1) {
    const y = region.y + row - box.y;
    if (y < 0 || y >= box.height) {
      continue;
    }
    for (let column = 0; column < region.width; column += 1) {
      const x = region.x + column - box.x;
      if (x >= 0 && x < box.width && kept[y * box.width + x] === 1) {
        ignored[row * region.width + column] = 0;
      }
    }
  }
  return ignored;
}

/** How many pixels `ignored` keeps. */
export function keptCount(ignored: Uint8Array): number {
  return ignored.reduce((count, left) => count + (left === 1 ? 0 : 1), 0);
}

/**
 * The RGBA pixels that shade what `ignored` leaves out of a `width`-wide
 * region: a dark tint with light diagonal stripes, clear where pixels are kept.
 */
export function shade(ignored: Uint8Array, width: number): Uint8ClampedArray<ArrayBuffer> {
  const pixels = new Uint8ClampedArray(ignored.length * 4);
  ignored.forEach((left, index) => {
    if (left !== 1) {
      return;
    }
    const stripe = (Math.floor(index / width) + (index % width)) % 4 === 0;
    pixels.set(stripe ? [255, 255, 255, 110] : [17, 19, 23, 160], index * 4);
  });
  return pixels;
}
