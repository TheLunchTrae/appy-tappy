// Selecting the thing a picture check looks for: the steps the user takes on
// a screenshot, and which pixels of the region the selection leaves out.

import type { Rect } from './region.ts';

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

/** The steps the setup app replays on the screenshot a picture check compares against. */
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
 * screenshot, begins a new selection.
 */
export function withStep(
  steps: readonly SelectStep[],
  before: number | null,
  on: number,
  step: SelectStep,
): readonly SelectStep[] {
  if (step.combine === 'replace' || before !== on) {
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
