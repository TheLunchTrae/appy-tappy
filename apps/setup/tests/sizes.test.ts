import { describe, expect, test } from 'bun:test';

import type { ScreenDef } from '../src/lib/bindings.ts';
import {
  fixAt,
  guessPin,
  type NowCell,
  nowCell,
  offersFix,
  pinOf,
  placement,
  type ReadFrom,
  regionText,
  sameSize,
} from '../src/lib/sizes.ts';

const window = { width: 1920, height: 1080 };
const larger = { width: 3440, height: 1440 };

const screen: ScreenDef = {
  windowSize: window,
  region: { x: 40, y: 1000, width: 200, height: 40 },
  check: { kind: 'barFill', filled: { red: 255, green: 0, blue: 0 }, empty: { red: 0, green: 0, blue: 0 } },
  fixes: [],
};

const fixed: ScreenDef = {
  ...screen,
  fixes: [{ windowSize: larger, region: { x: 50, y: 1330, width: 260, height: 50 }, images: [] }],
};

describe('guessPin', () => {
  test("takes the third of the window the region's center is in", () => {
    const at = (x: number, y: number): string => guessPin({ x, y, width: 2, height: 2 }, { width: 30, height: 30 });
    // Centers at 1, 15 and 29 across and down.
    expect(at(0, 0)).toBe('topLeft');
    expect(at(14, 0)).toBe('topCenter');
    expect(at(28, 0)).toBe('topRight');
    expect(at(0, 14)).toBe('middleLeft');
    expect(at(14, 14)).toBe('center');
    expect(at(28, 14)).toBe('middleRight');
    expect(at(0, 28)).toBe('bottomLeft');
    expect(at(14, 28)).toBe('bottomCenter');
    expect(at(28, 28)).toBe('bottomRight');
  });

  test('puts a center on a third line in the third after it, as AppyTappy does', () => {
    // A center at exactly 10 of 30 starts the middle third, and at 20 the last.
    expect(guessPin({ x: 9, y: 0, width: 2, height: 2 }, { width: 30, height: 30 })).toBe('topCenter');
    expect(guessPin({ x: 8, y: 0, width: 3, height: 2 }, { width: 30, height: 30 })).toBe('topLeft');
    expect(guessPin({ x: 19, y: 19, width: 2, height: 2 }, { width: 30, height: 30 })).toBe('bottomRight');
  });
});

describe('regionText', () => {
  test('says where the value was drawn, the side it stays at and each size it is fixed at', () => {
    expect(regionText(screen)).toBe('Drawn at 1920 × 1080, pinned bottom left');
    expect(regionText({ ...fixed, pin: 'center' })).toBe('Drawn at 1920 × 1080, pinned center · fixed at 3440 × 1440');
    expect(pinOf({ ...screen, pin: 'topRight' })).toBe('topRight');
  });
});

describe('placement', () => {
  test('reads the drawing at its size, a fix at its own, and scales elsewhere only when the size changes often', () => {
    expect(placement(fixed, 'rarely', window)).toBe('drawn');
    expect(placement(fixed, 'often', larger)).toBe('fixed');
    expect(placement(fixed, 'rarely', larger)).toBe('fixed');
    expect(placement(fixed, 'often', { width: 1280, height: 720 })).toBe('scaled');
    expect(placement(fixed, 'rarely', { width: 1280, height: 720 })).toBe('notDrawn');
    expect(fixAt(fixed, larger)?.region.x).toBe(50);
    expect(fixAt(fixed, { width: 3440, height: 1441 })).toBeNull();
    expect(sameSize(window, { width: 1920, height: 1081 })).toBe(false);
  });
});

describe('nowCell', () => {
  const reading = { text: '50', noState: false };

  test('shows the reading, and where it reads from', () => {
    expect(nowCell(fixed, 'often', window, reading, null)).toEqual({ kind: 'reading', text: '50', from: 'drawn' });
    expect(nowCell(fixed, 'often', larger, reading, null)).toEqual({ kind: 'reading', text: '50', from: 'fixed' });
  });

  test('says why there is no reading', () => {
    expect(nowCell(screen, 'often', window, null, null)).toEqual({ kind: 'waiting' });
    expect(nowCell(screen, 'often', larger, { text: 'none', noState: true }, null)).toEqual({
      kind: 'noState',
      from: 'scaled',
    });
    expect(nowCell(fixed, 'often', larger, null, 'Cannot read.')).toEqual({
      kind: 'unread',
      problem: 'Cannot read.',
      from: 'fixed',
    });
    // Read only at its drawn sizes, it says so whatever the window read.
    expect(nowCell(screen, 'rarely', larger, reading, 'Not read.')).toEqual({ kind: 'notDrawn' });
  });
});

describe('offersFix', () => {
  test('offers a fix where a value does not read, except at the size it was first drawn at', () => {
    const unread = (from: ReadFrom): NowCell => ({ kind: 'unread', problem: 'Cannot read.', from });
    expect(offersFix(unread('scaled'))).toBe(true);
    expect(offersFix(unread('fixed'))).toBe(true);
    expect(offersFix(unread('drawn'))).toBe(false);
    expect(offersFix({ kind: 'noState', from: 'scaled' })).toBe(true);
    expect(offersFix({ kind: 'noState', from: 'drawn' })).toBe(false);
    expect(offersFix({ kind: 'notDrawn' })).toBe(true);
    expect(offersFix({ kind: 'reading', text: '50', from: 'scaled' })).toBe(false);
    expect(offersFix({ kind: 'waiting' })).toBe(false);
  });
});
