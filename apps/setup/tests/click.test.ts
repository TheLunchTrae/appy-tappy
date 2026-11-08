import { describe, expect, test } from 'bun:test';

import { MAX_SEARCH_PIXELS, middle, placeSpot, searchArea, spotAt, targetSummary } from '../src/lib/click.ts';

const window = { width: 1280, height: 720 };

describe('spots', () => {
  test('are measured from the nearest corner and land where they were picked', () => {
    for (const point of [
      { x: 10, y: 20 },
      { x: 1270, y: 15 },
      { x: 5, y: 700 },
      { x: 1279, y: 719 },
      { x: 640, y: 360 },
    ]) {
      const spot = spotAt(point, window);
      if (spot.kind !== 'spot') {
        throw new Error('a spot');
      }
      expect(placeSpot(spot, window)).toEqual(point);
    }
    expect(spotAt({ x: 1270, y: 15 }, window)).toEqual({ kind: 'spot', corner: 'topRight', x: 9, y: 15 });
    expect(spotAt({ x: 5, y: 700 }, window)).toEqual({ kind: 'spot', corner: 'bottomLeft', x: 5, y: 19 });
  });

  test('follow their corner when the window grows, and land nowhere outside it', () => {
    const spot = spotAt({ x: 1270, y: 700 }, window);
    if (spot.kind !== 'spot') {
      throw new Error('a spot');
    }
    expect(placeSpot(spot, { width: 1920, height: 1080 })).toEqual({ x: 1910, y: 1060 });
    expect(placeSpot(spot, { width: 5, height: 5 })).toBeNull();
  });
});

describe('search areas', () => {
  test('reach 50 pixels around the picture, inside the window', () => {
    expect(searchArea({ x: 100, y: 100, width: 40, height: 20 }, window)).toEqual({
      x: 50,
      y: 50,
      width: 140,
      height: 120,
    });
    expect(searchArea({ x: 10, y: 700, width: 40, height: 20 }, window)).toEqual({
      x: 0,
      y: 650,
      width: 100,
      height: 70,
    });
  });

  test('narrow to hold at most 160000 pixels, and refuse a picture larger than that', () => {
    const area = searchArea({ x: 200, y: 200, width: 380, height: 380 }, window);
    expect(area).not.toBeNull();
    expect((area?.width ?? 0) * (area?.height ?? 0)).toBeLessThanOrEqual(MAX_SEARCH_PIXELS);
    expect(area).toEqual({ x: 190, y: 190, width: 400, height: 400 });
    expect(searchArea({ x: 0, y: 0, width: 401, height: 400 }, window)).toBeNull();
  });
});

describe('targets', () => {
  test('press in the middle of a picture until another place is picked', () => {
    expect(middle({ width: 124, height: 20 })).toEqual({ x: 62, y: 10 });
  });

  test('say what they click in a few words', () => {
    expect(targetSummary('left', null)).toBe('No target yet. Set up where it clicks.');
    expect(targetSummary('double', { kind: 'spot', corner: 'bottomRight', x: 40, y: 12 })).toBe(
      'Double click 40 px from the right, 12 px from the bottom',
    );
    expect(
      targetSummary('right', {
        kind: 'picture',
        image: 'click-1.png',
        windowSize: window,
        search: { x: 0, y: 0, width: 10, height: 10 },
        press: { x: 1, y: 1 },
        tolerance: 20,
      }),
    ).toBe("Right click on this picture, when it's found near where it was");
  });
});
