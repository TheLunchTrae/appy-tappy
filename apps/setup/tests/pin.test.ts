import { describe, expect, test } from 'bun:test';

import { dragCorner, fromCorners, moveRect, type Rect } from '../src/lib/region.ts';
import {
  AUTOMATIC,
  type PinChoice,
  pinChoiceAfterRegion,
  pinGuides,
  pinMeaning,
  pinNotice,
  pinOfView,
  pinSummary,
  pinView,
} from '../src/lib/pin.ts';

const window = { width: 300, height: 90 };
/** In the top right third of the window. */
const topRight = { x: 250, y: 5, width: 40, height: 10 };
/** In the bottom left third. */
const bottomLeft = { x: 10, y: 70, width: 40, height: 10 };
const byHand: PinChoice = { kind: 'byHand', pin: 'topRight' };

describe('pinChoiceAfterRegion', () => {
  /** The choice once the region on a picture of the window's size goes from the top right one to `next`. */
  const after = (choice: PinChoice, next: Rect): PinChoice => pinChoiceAfterRegion(choice, topRight, next, window);

  test('keeps a side chosen by hand while the region is moved or resized', () => {
    const moved = moveRect(topRight, -240, 65, window);
    expect(moved).toEqual(bottomLeft);
    expect(after(byHand, moved)).toBe(byHand);
    for (const corner of ['nw', 'ne', 'sw', 'se'] as const) {
      expect(after(byHand, dragCorner(topRight, corner, { x: 200, y: 40 }, window))).toBe(byHand);
    }
    // Dragged past the corner that stays put, the region flips over it and still has it.
    const flipped = dragCorner(topRight, 'se', { x: 100, y: 2 }, window);
    expect(flipped).toEqual({ x: 100, y: 2, width: 150, height: 3 });
    expect(after(byHand, flipped)).toBe(byHand);
    // An edge moved by typing keeps the corners of the edge across from it.
    expect(after(byHand, { ...topRight, width: 20 })).toBe(byHand);
    expect(after(byHand, { ...topRight, x: 3 })).toBe(byHand);
  });

  test('keeps a side chosen by hand while a region that reaches past a smaller picture is resized', () => {
    // Drawn on a 1920 wide screenshot and now dragged on a 1280 wide one: the corner that stays put is kept inside
    // the picture, so the region keeps none of its own corners.
    const drawn = { x: 1800, y: 50, width: 100, height: 40 };
    const smaller = { width: 1280, height: 720 };
    const fromRight = dragCorner(drawn, 'se', { x: 1000, y: 200 }, smaller);
    expect(fromRight).toEqual({ x: 1000, y: 50, width: 280, height: 150 });
    expect(pinChoiceAfterRegion(byHand, drawn, fromRight, smaller)).toBe(byHand);
    const fromLeft = dragCorner(drawn, 'nw', { x: 900, y: 10 }, smaller);
    expect(fromLeft).toEqual({ x: 900, y: 10, width: 380, height: 80 });
    expect(pinChoiceAfterRegion(byHand, drawn, fromLeft, smaller)).toBe(byHand);
    // On a picture that holds the whole region, the same change is a new drawing.
    expect(pinChoiceAfterRegion(byHand, drawn, fromRight, { width: 1920, height: 1080 })).toBe(AUTOMATIC);
    // A region drawn elsewhere on the smaller picture is new there too.
    expect(pinChoiceAfterRegion(byHand, drawn, { x: 100, y: 300, width: 60, height: 20 }, smaller)).toBe(AUTOMATIC);
  });

  test('returns to automatic when a region is drawn anew', () => {
    // A drag that starts outside the region draws a new one from there.
    const drawn = fromCorners({ x: 20, y: 60 }, { x: 23, y: 62 }, window);
    expect(drawn).toEqual({ x: 20, y: 60, width: 3, height: 2 });
    expect(after(byHand, { x: 20, y: 60, width: 3, height: 2 })).toBe(AUTOMATIC);
    // A new region as wide as the old one, or with one edge in line with it, is still new.
    expect(after(byHand, { x: 10, y: 70, width: 40, height: 12 })).toBe(AUTOMATIC);
    expect(after(byHand, { x: 10, y: 70, width: 30, height: 10 })).toBe(AUTOMATIC);
    expect(after(byHand, { x: 250, y: 40, width: 20, height: 20 })).toBe(AUTOMATIC);
    expect(after(byHand, { x: 100, y: 5, width: 20, height: 20 })).toBe(AUTOMATIC);
    expect(pinChoiceAfterRegion(byHand, null, topRight, window)).toBe(AUTOMATIC);
  });

  test('stays automatic through every change', () => {
    expect(after(AUTOMATIC, bottomLeft)).toBe(AUTOMATIC);
    expect(after(AUTOMATIC, { ...topRight, x: 3 })).toBe(AUTOMATIC);
  });
});

describe('pinView', () => {
  test('follows the region while automatic', () => {
    expect(pinView(AUTOMATIC, topRight, window)).toEqual({ kind: 'automatic', pin: 'topRight' });
    expect(pinView(AUTOMATIC, bottomLeft, window)).toEqual({ kind: 'automatic', pin: 'bottomLeft' });
    expect(pinView(AUTOMATIC, null, window)).toEqual({ kind: 'waiting' });
    expect(pinView(AUTOMATIC, topRight, null)).toEqual({ kind: 'waiting' });
  });

  test('keeps a side chosen by hand and names the nearest side only when it is another', () => {
    expect(pinView(byHand, topRight, window)).toEqual({ kind: 'byHand', pin: 'topRight', nearest: null });
    expect(pinView(byHand, bottomLeft, window)).toEqual({ kind: 'byHand', pin: 'topRight', nearest: 'bottomLeft' });
    expect(pinView(byHand, null, null)).toEqual({ kind: 'byHand', pin: 'topRight', nearest: null });
  });

  test('saves the side shown, and none while there is no region to follow', () => {
    expect(pinOfView(pinView(AUTOMATIC, bottomLeft, window))).toBe('bottomLeft');
    expect(pinOfView(pinView(byHand, bottomLeft, window))).toBe('topRight');
    expect(pinOfView(pinView(AUTOMATIC, null, window))).toBeNull();
  });
});

describe('what the control says', () => {
  test('names the side and how it was picked', () => {
    expect(pinSummary({ kind: 'waiting' })).toBe('Follows the region once you draw it');
    expect(pinSummary({ kind: 'automatic', pin: 'topRight' })).toBe('Top right (automatic)');
    expect(pinSummary({ kind: 'byHand', pin: 'center', nearest: null })).toBe('Center (chosen by hand)');
  });

  test('names both sides once a side chosen by hand is not the nearest', () => {
    expect(pinNotice({ kind: 'byHand', pin: 'topRight', nearest: 'bottomLeft' })).toBe(
      'Chosen by hand: top right. The region is now nearest the bottom left.',
    );
    expect(pinNotice({ kind: 'byHand', pin: 'topRight', nearest: null })).toBeNull();
    expect(pinNotice({ kind: 'automatic', pin: 'topRight' })).toBeNull();
    expect(pinNotice({ kind: 'waiting' })).toBeNull();
  });

  test('says what each kind of side keeps its distance from', () => {
    const kept = (text: string): string =>
      `When the window changes size, the region scales with it and keeps its scaled distance from ${text}.`;
    expect(pinMeaning('topRight')).toBe(kept('the top and right edges'));
    expect(pinMeaning('bottomLeft')).toBe(kept('the bottom and left edges'));
    expect(pinMeaning('topCenter')).toBe(kept("the top edge and the middle of the window's width"));
    expect(pinMeaning('bottomCenter')).toBe(kept("the bottom edge and the middle of the window's width"));
    expect(pinMeaning('middleLeft')).toBe(kept("the left edge and the middle of the window's height"));
    expect(pinMeaning('middleRight')).toBe(kept("the right edge and the middle of the window's height"));
    expect(pinMeaning('center')).toBe(kept('the center of the window'));
  });
});

describe('pinGuides', () => {
  const region = { x: 200, y: 20, width: 40, height: 10 };

  test('runs from each edge the region stays at to the side of the region that faces it', () => {
    expect(pinGuides(region, window, 'topRight')).toEqual({
      across: { from: { x: 300, y: 25 }, to: { x: 240, y: 25 } },
      down: { from: { x: 220, y: 0 }, to: { x: 220, y: 20 } },
    });
    expect(pinGuides(region, window, 'bottomLeft')).toEqual({
      across: { from: { x: 0, y: 25 }, to: { x: 200, y: 25 } },
      down: { from: { x: 220, y: 90 }, to: { x: 220, y: 30 } },
    });
  });

  test("runs from the window's middle to the side of the region nearer it", () => {
    // The region is right of the middle across, 150, and above the middle down, 45.
    expect(pinGuides(region, window, 'center')).toEqual({
      across: { from: { x: 150, y: 25 }, to: { x: 200, y: 25 } },
      down: { from: { x: 220, y: 45 }, to: { x: 220, y: 30 } },
    });
    const leftBelow = { x: 20, y: 60, width: 40, height: 10 };
    expect(pinGuides(leftBelow, window, 'center')).toEqual({
      across: { from: { x: 150, y: 65 }, to: { x: 60, y: 65 } },
      down: { from: { x: 40, y: 45 }, to: { x: 40, y: 60 } },
    });
  });

  test('has no length from a middle the region reaches over', () => {
    const over = { x: 100, y: 40, width: 100, height: 10 };
    expect(pinGuides(over, window, 'center')).toEqual({
      across: { from: { x: 150, y: 45 }, to: { x: 150, y: 45 } },
      down: { from: { x: 150, y: 45 }, to: { x: 150, y: 45 } },
    });
  });
});
