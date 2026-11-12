// How a new value's pin is picked, the side of the window it stays at: it follows where the region is unless the
// user chooses a side by hand. Also what a side means when the window changes size, in words and as guide lines.

import type { PinDef } from './bindings.ts';
import type { Point, Rect, Size } from './region.ts';
import { guessPin, PIN_NAMES, PINS } from './sizes.ts';

/** How a value's pin is picked: it follows where the region is, or the user chose it. */
export type PinChoice = { readonly kind: 'automatic' } | { readonly kind: 'byHand'; readonly pin: PinDef };

/** The choice a new value starts with, and the one **Back to automatic** returns to. */
export const AUTOMATIC: PinChoice = { kind: 'automatic' };

/** Whether `a` and `b` have a corner at the same point, whichever corner of each it is. */
function shareCorner(a: Rect, b: Rect): boolean {
  const across = (rect: Rect): readonly number[] => [rect.x, rect.x + rect.width];
  const down = (rect: Rect): readonly number[] => [rect.y, rect.y + rect.height];
  return across(a).some((x) => across(b).includes(x)) && down(a).some((y) => down(b).includes(y));
}

/**
 * The choice once the region on the screenshot goes from `previous` to `next`. Moving a region keeps its size and
 * resizing it keeps a corner, and both keep a side chosen by hand. A region that keeps neither is drawn anew, which
 * returns to automatic, since a side chosen for the region before says nothing about this one.
 */
export function pinChoiceAfterRegion(choice: PinChoice, previous: Rect | null, next: Rect): PinChoice {
  if (previous === null) {
    return AUTOMATIC;
  }
  const moved = previous.width === next.width && previous.height === next.height;
  return moved || shareCorner(previous, next) ? choice : AUTOMATIC;
}

/**
 * The choice that a saved value's pin makes for its `region`, drawn on a window of `window`: automatic when it is
 * the side the region is nearest or the value has none saved, and by hand otherwise.
 */
export function pinChoiceOf(saved: PinDef | undefined, region: Rect, window: Size): PinChoice {
  return saved === undefined || saved === guessPin(region, window) ? AUTOMATIC : { kind: 'byHand', pin: saved };
}

/** What the **Stays at** control shows for a choice. */
export type PinView =
  /** Automatic, with no region drawn yet to follow. */
  | { readonly kind: 'waiting' }
  /** Automatic: `pin` is the side the region is nearest. */
  | { readonly kind: 'automatic'; readonly pin: PinDef }
  /** Chosen by hand. `nearest` is the side the region is nearest when that is another side, and null otherwise. */
  | { readonly kind: 'byHand'; readonly pin: PinDef; readonly nearest: PinDef | null };

/** What `choice` comes to for `region`, drawn on a window of `window`; either is null until a region is drawn. */
export function pinView(choice: PinChoice, region: Rect | null, window: Size | null): PinView {
  const nearest = region === null || window === null ? null : guessPin(region, window);
  if (choice.kind === 'byHand') {
    return { kind: 'byHand', pin: choice.pin, nearest: nearest === choice.pin ? null : nearest };
  }
  return nearest === null ? { kind: 'waiting' } : { kind: 'automatic', pin: nearest };
}

/** The side `view` saves, or null while there is no region to follow. */
export function pinOfView(view: PinView): PinDef | null {
  return view.kind === 'waiting' ? null : view.pin;
}

/** The side and how it was picked, such as `Top right (automatic)`. */
export function pinSummary(view: PinView): string {
  switch (view.kind) {
    case 'waiting':
      return 'Follows the region once you draw it';
    case 'automatic':
      return `${PIN_NAMES[view.pin]} (automatic)`;
    case 'byHand':
      return `${PIN_NAMES[view.pin]} (chosen by hand)`;
  }
}

/** What to say when a side chosen by hand is not the side the region is nearest, or null when nothing needs saying. */
export function pinNotice(view: PinView): string | null {
  if (view.kind !== 'byHand' || view.nearest === null) {
    return null;
  }
  const chosen = PIN_NAMES[view.pin].toLowerCase();
  return `Chosen by hand: ${chosen}. The region is now nearest the ${PIN_NAMES[view.nearest].toLowerCase()}.`;
}

/** Where along the window a pin sits, across or down: at its start, its middle or its end. */
type Along = 'start' | 'middle' | 'end';

const ALONG: readonly Along[] = ['start', 'middle', 'end'];

/** Where `pin` sits across the window and down it. */
function sides(pin: PinDef): { readonly across: Along; readonly down: Along } {
  const index = PINS.indexOf(pin);
  return { across: ALONG[index % 3] ?? 'start', down: ALONG[Math.floor(index / 3)] ?? 'start' };
}

/** What a region that stays at `pin` keeps its scaled distance from. */
function keptFrom(pin: PinDef): string {
  const { across, down } = sides(pin);
  const edgeAcross = across === 'start' ? 'left' : 'right';
  const edgeDown = down === 'start' ? 'top' : 'bottom';
  if (across === 'middle' && down === 'middle') {
    return 'the center of the window';
  }
  if (across === 'middle') {
    return `the ${edgeDown} edge and the middle of the window's width`;
  }
  if (down === 'middle') {
    return `the ${edgeAcross} edge and the middle of the window's height`;
  }
  return `the ${edgeDown} and ${edgeAcross} edges`;
}

/** What staying at `pin` means for the region when the window changes size. */
export function pinMeaning(pin: PinDef): string {
  return `When the window changes size, the region scales with it and keeps its scaled distance from ${keptFrom(pin)}.`;
}

/** A line from the edge or the middle of the window that a region keeps its distance from, to the region. */
export interface Guide {
  /** The end on the window's edge or middle. */
  readonly from: Point;
  /** The end on the region. */
  readonly to: Point;
}

/**
 * One end of a guide along one axis: where on a side `length` long a span from `start` that is `size` long is
 * measured from, and the point of the span the distance is measured to. From the middle that is the span's nearer
 * edge, or the middle itself when the span reaches over it.
 */
function measured(along: Along, start: number, size: number, length: number): readonly [number, number] {
  const end = start + size;
  switch (along) {
    case 'start':
      return [0, start];
    case 'end':
      return [length, end];
    case 'middle': {
      const middle = length / 2;
      return [middle, Math.min(Math.max(middle, start), end)];
    }
  }
}

/**
 * The guides that show what `region`, on a window of `window`, keeps its distance from while it stays at `pin`: one
 * across the window at the region's middle height, and one down it at the region's middle width.
 */
export function pinGuides(region: Rect, window: Size, pin: PinDef): { readonly across: Guide; readonly down: Guide } {
  const at = sides(pin);
  const [fromX, toX] = measured(at.across, region.x, region.width, window.width);
  const [fromY, toY] = measured(at.down, region.y, region.height, window.height);
  const middleX = region.x + region.width / 2;
  const middleY = region.y + region.height / 2;
  return {
    across: { from: { x: fromX, y: middleY }, to: { x: toX, y: middleY } },
    down: { from: { x: middleX, y: fromY }, to: { x: middleX, y: toY } },
  };
}
