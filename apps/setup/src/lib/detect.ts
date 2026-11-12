// Finding a bar's colors for the user: the request and its answer, what the colors measure, and the words that
// say how far to trust them.

import type { BarDetection, BarMarks, BarMeasure, BarReading } from './bindings.ts';
import type { Color } from './pixels.ts';
import { type Rect, sameRect } from './region.ts';

/** A bar's two colors. */
export interface BarColors {
  readonly filled: Color;
  readonly empty: Color;
}

/** The bar's colors as far as they are set. */
export interface PickedColors {
  readonly filled: Color | null;
  readonly empty: Color | null;
}

/** What a detection looks at: the region, the screenshot it was drawn on, and how many screenshots there are. */
export interface DetectKey {
  readonly region: Rect;
  readonly drawnOn: number;
  readonly shots: number;
}

type Proposal = Extract<BarDetection, { readonly kind: 'proposed' }>;

/** Where detecting the bar stands. */
export type Detecting =
  | { readonly kind: 'idle' }
  | { readonly kind: 'detecting'; readonly key: DetectKey }
  /** Colors were found. While `asking`, the user has yet to say whether they replace the colors picked by hand. */
  | { readonly kind: 'found'; readonly key: DetectKey; readonly proposal: Proposal; readonly asking: boolean }
  /** No colors are proposed, or the request failed; `message` says what to do. */
  | { readonly kind: 'declined'; readonly key: DetectKey; readonly message: string };

/** What moves a detection on. */
export type DetectEvent =
  | { readonly kind: 'ask'; readonly key: DetectKey }
  /** The answer to the request made for `key`, with the colors the draft holds as it arrives. */
  | {
      readonly kind: 'answer';
      readonly key: DetectKey;
      readonly detection: BarDetection;
      readonly picked: PickedColors;
    }
  | { readonly kind: 'fail'; readonly key: DetectKey; readonly message: string }
  /** The user takes the colors found. */
  | { readonly kind: 'use' }
  /** The user keeps the colors picked by hand. */
  | { readonly kind: 'keep' }
  /** What a detection would look at now, or null when there is nothing to look at. */
  | { readonly kind: 'inputs'; readonly key: DetectKey | null };

function sameKey(a: DetectKey, b: DetectKey | null): boolean {
  return b !== null && sameRect(a.region, b.region) && a.drawnOn === b.drawnOn && a.shots === b.shots;
}

function sameColor(a: Color | null, b: Color | null): boolean {
  return a !== null && b !== null && a.red === b.red && a.green === b.green && a.blue === b.blue;
}

/** Whether `picked` holds the two colors of `colors`, either way around. */
function holds(picked: PickedColors, colors: BarColors): boolean {
  return (
    (sameColor(picked.filled, colors.filled) && sameColor(picked.empty, colors.empty)) ||
    (sameColor(picked.filled, colors.empty) && sameColor(picked.empty, colors.filled))
  );
}

function colorsOf(proposal: Proposal): BarColors {
  return { filled: proposal.filled, empty: proposal.empty };
}

/**
 * The detection after `event`, and the colors to put in the draft, if any. An answer or a failure for anything but
 * the request under way is dropped, so a detection of a region or of screenshots that changed since never shows.
 * Colors found go straight into a draft with none; a draft with a color picked by hand is asked about first.
 */
export function detectStep(
  state: Detecting,
  event: DetectEvent,
): { readonly state: Detecting; readonly use: BarColors | null } {
  const stay = { state, use: null };
  switch (event.kind) {
    case 'ask':
      return { state: { kind: 'detecting', key: event.key }, use: null };
    case 'inputs':
      return state.kind === 'idle' || sameKey(state.key, event.key) ? stay : { state: { kind: 'idle' }, use: null };
    case 'answer': {
      if (state.kind !== 'detecting' || !sameKey(state.key, event.key)) {
        return stay;
      }
      const { detection, picked } = event;
      if (detection.kind === 'declined') {
        return { state: { kind: 'declined', key: state.key, message: detection.message }, use: null };
      }
      const asking = (picked.filled !== null || picked.empty !== null) && !holds(picked, detection);
      return {
        state: { kind: 'found', key: state.key, proposal: detection, asking },
        use: picked.filled === null && picked.empty === null ? colorsOf(detection) : null,
      };
    }
    case 'fail':
      return state.kind === 'detecting' && sameKey(state.key, event.key)
        ? { state: { kind: 'declined', key: state.key, message: event.message }, use: null }
        : stay;
    case 'use':
      return state.kind === 'found' && state.asking
        ? { state: { ...state, asking: false }, use: colorsOf(state.proposal) }
        : stay;
    case 'keep':
      return state.kind === 'found' && state.asking ? { state: { kind: 'idle' }, use: null } : stay;
  }
}

/** `numbers` as words, such as `1`, `1 and 2`, or `1, 2, and 3`. */
function listed(numbers: readonly number[]): string {
  const last = numbers.at(-1);
  if (last === undefined || numbers.length === 1) {
    return numbers.join('');
  }
  const before = numbers.slice(0, -1).join(', ');
  return numbers.length === 2 ? `${before} and ${last}` : `${before}, and ${last}`;
}

/** What one screenshot cannot show, and what to do about it. */
const ONE_SCREENSHOT =
  'One screenshot cannot tell a bar from a picture in two colors: compare the readings below with what you see, or take a screenshot at another fill and detect again.';

/**
 * How `proposal`'s colors were found for a region drawn on screenshot `drawnOn`, and how far to trust them.
 * `position` gives a screenshot's number in the strip.
 */
function basisText(proposal: Proposal, drawnOn: number, position: (index: number) => number): string {
  const numbers = proposal.screenshots.map(position);
  switch (proposal.basis) {
    case 'layout':
      return numbers.length > 1
        ? `Found on screenshot ${position(drawnOn)} alone: the region does not change between two colors on screenshots ${listed(numbers)}. ${ONE_SCREENSHOT}`
        : `Found on screenshot ${position(drawnOn)} alone. ${ONE_SCREENSHOT}`;
    case 'changes':
      return `Found from the pixels that change between screenshots ${listed(numbers)}.`;
    case 'roughChanges':
      return `Found from the pixels that change between screenshots ${listed(numbers)}. The two colors leave out some of those pixels, as on a bar filled with a gradient, so readings can be a few points off.`;
  }
}

/** Which color the detection takes as the filled one, and how to turn that around. */
const FILLED_SIDE_NOTE =
  'The left color, or the bottom one of a tall region, counts as filled: swap the colors if the bar fills from the other end.';

/** What detecting does, shown until it has something to say. */
const DETECT_NOTE =
  'Finds the filled and the empty color on the screenshots. Pick them by hand below to correct them, or when it finds none.';

/**
 * What shows under Detect the bar: how the colors were found while the draft's `picked` colors are those, or the
 * user is asked about them; why none were found; or what detecting does. `position` gives a screenshot's number in
 * the strip.
 */
export function detectingText(detecting: Detecting, picked: PickedColors, position: (index: number) => number): string {
  switch (detecting.kind) {
    case 'idle':
      return DETECT_NOTE;
    case 'detecting':
      return 'Looking for the bar…';
    case 'declined':
      return detecting.message;
    case 'found':
      return detecting.asking || holds(picked, detecting.proposal)
        ? `${basisText(detecting.proposal, detecting.key.drawnOn, position)} ${FILLED_SIDE_NOTE}`
        : DETECT_NOTE;
  }
}

/** What `reading`'s screenshot reads, such as `Screenshot 2: 35`; `position` gives its number in the strip. */
export function readingLine(reading: BarReading, position: (index: number) => number): string {
  const { fill } = reading;
  // A whole number unless a tenth shows more, as a bar a few pixels long reads.
  const shown = fill === null ? 'neither color shows' : String(Math.round(fill * 10) / 10);
  return `Screenshot ${position(reading.screenshot)}: ${shown}`;
}

/** What a measure answers for: the detection's inputs, the two colors, and the screenshot shown. */
export function measureKey(key: DetectKey, colors: BarColors, shown: number | null): string {
  return JSON.stringify([key.region, key.drawnOn, key.shots, colors.filled, colors.empty, shown]);
}

/** A measure and what it was made for. */
export interface Measured {
  readonly key: string;
  readonly measure: BarMeasure | null;
  readonly error: string | null;
}

/** `made` when it was made for `key`, or else null: an answer for other colors, region, or screenshots never shows. */
export function measuredFor(made: Measured | null, key: string | null): Measured | null {
  return made?.key === key ? made : null;
}

/** The overlay's color over a pixel that counts as each part, as red, green, blue, and alpha. */
export const MARK_TONES = {
  filled: [242, 179, 61, 170],
  empty: [74, 163, 255, 170],
} as const;

/** A tone as CSS, for the legend. */
export function toneCss([red, green, blue]: readonly [number, number, number, number]): string {
  return `rgb(${red} ${green} ${blue})`;
}

/**
 * The RGBA pixels that mark what a bar's two colors count in a region of `pixels` pixels: a tone over each pixel of
 * `marks`' runs, and clear elsewhere. A run that reaches past the region is cut off at its end.
 */
export function paintMarks(marks: BarMarks, pixels: number): Uint8ClampedArray<ArrayBuffer> {
  const painted = new Uint8ClampedArray(pixels * 4);
  for (const part of ['filled', 'empty'] as const) {
    for (const [start, end] of marks[part]) {
      for (let index = Math.max(0, start); index < Math.min(end, pixels); index += 1) {
        painted.set(MARK_TONES[part], index * 4);
      }
    }
  }
  return painted;
}
