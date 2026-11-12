import { describe, expect, test } from 'bun:test';

import {
  bounding,
  contains,
  fit,
  fromCorners,
  fromPicture,
  moveRect,
  pixelAt,
  sameRect,
  tagPlace,
  toPicture,
} from '../src/lib/region.ts';

const bounds = { width: 100, height: 50 };

describe('fromCorners', () => {
  test('covers the pixels between two points in any order', () => {
    expect(fromCorners({ x: 10.4, y: 5.6 }, { x: 20.2, y: 2 }, bounds)).toEqual({ x: 10, y: 2, width: 11, height: 4 });
  });

  test('stays inside the picture', () => {
    expect(fromCorners({ x: -5, y: -5 }, { x: 500, y: 500 }, bounds)).toEqual({ x: 0, y: 0, width: 100, height: 50 });
  });

  test('is null when it covers no pixel', () => {
    expect(fromCorners({ x: 10, y: 10 }, { x: 10, y: 30 }, bounds)).toBeNull();
    expect(fromCorners({ x: 120, y: 10 }, { x: 150, y: 30 }, bounds)).toBeNull();
  });
});

describe('moveRect', () => {
  const rect = { x: 10, y: 10, width: 20, height: 10 };

  test('moves by whole pixels', () => {
    expect(moveRect(rect, 5.4, -2.6, bounds)).toEqual({ x: 15, y: 7, width: 20, height: 10 });
  });

  test('stops at the edges', () => {
    expect(moveRect(rect, -50, 500, bounds)).toEqual({ x: 0, y: 40, width: 20, height: 10 });
    expect(moveRect(rect, 500, -50, bounds)).toEqual({ x: 80, y: 0, width: 20, height: 10 });
  });

  test('cuts a rectangle larger than the picture to it', () => {
    expect(moveRect({ x: 60, y: 30, width: 300, height: 20 }, 4, 4, bounds)).toEqual({
      x: 0,
      y: 30,
      width: 100,
      height: 20,
    });
    expect(moveRect({ x: 60, y: 30, width: 20, height: 90 }, 4, 4, bounds)).toEqual({
      x: 64,
      y: 0,
      width: 20,
      height: 50,
    });
  });
});

test('contains counts the left and top edges in and the right and bottom out', () => {
  const rect = { x: 10, y: 10, width: 5, height: 5 };
  expect(contains(rect, { x: 10, y: 10 })).toBe(true);
  expect(contains(rect, { x: 14.9, y: 14.9 })).toBe(true);
  expect(contains(rect, { x: 15, y: 12 })).toBe(false);
  expect(contains(rect, { x: 12, y: 9 })).toBe(false);
});

describe('fit', () => {
  test('keeps the shape inside the space', () => {
    expect(fit({ width: 1920, height: 1080 }, { width: 960, height: 960 })).toEqual({ width: 960, height: 540 });
    expect(fit({ width: 100, height: 200 }, { width: 400, height: 100 })).toEqual({ width: 50, height: 100 });
  });

  test('is empty for an empty picture or space', () => {
    expect(fit({ width: 0, height: 10 }, { width: 10, height: 10 })).toEqual({ width: 0, height: 0 });
    expect(fit({ width: 10, height: 10 }, { width: 10, height: 0 })).toEqual({ width: 0, height: 0 });
  });
});

test('toPicture scales a point from the drawn size to the picture', () => {
  expect(toPicture({ x: 480, y: 135 }, { width: 960, height: 540 }, { width: 1920, height: 1080 })).toEqual({
    x: 960,
    y: 270,
  });
  expect(toPicture({ x: 4, y: 4 }, { width: 0, height: 0 }, { width: 10, height: 10 })).toEqual({ x: 0, y: 0 });
});

test('fromPicture puts a point of the picture where it shows, and toPicture takes it back', () => {
  const picture = { width: 1920, height: 1080 };
  expect(fromPicture({ x: 960, y: 270 }, { width: 960, height: 540 }, picture)).toEqual({ x: 480, y: 135 });
  expect(fromPicture({ x: 4, y: 4 }, { width: 10, height: 10 }, { width: 0, height: 0 })).toEqual({ x: 0, y: 0 });
  // At sizes that divide unevenly, from a tenth of the picture's own size to eight times it.
  for (const scale of [0.1, 0.37, 1, 1.5, 3.3, 8]) {
    const shown = { width: picture.width * scale, height: picture.height * scale };
    for (const point of [
      { x: 0, y: 0 },
      { x: 7, y: 1079 },
      { x: 1919.5, y: 0.25 },
      { x: 640.3, y: 333.7 },
    ]) {
      const back = toPicture(fromPicture(point, shown, picture), shown, picture);
      expect(back.x).toBeCloseTo(point.x, 9);
      expect(back.y).toBeCloseTo(point.y, 9);
    }
  }
});

test('pixelAt is the pixel under a point, and the last one for a point on or past the far edge', () => {
  expect(pixelAt({ x: 10.9, y: 0.2 }, bounds)).toEqual({ x: 10, y: 0 });
  expect(pixelAt({ x: 100, y: 50 }, bounds)).toEqual({ x: 99, y: 49 });
  expect(pixelAt({ x: 100.4, y: 73 }, bounds)).toEqual({ x: 99, y: 49 });
  expect(pixelAt({ x: -0.5, y: -3 }, bounds)).toEqual({ x: 0, y: 0 });
});

describe('tagPlace', () => {
  // A 100 by 50 picture drawn at twice its size, with a label that needs 20 drawn pixels.
  const place = (rect: { x: number; y: number; width: number; height: number }) => tagPlace(rect, bounds, 2, 20);

  test('goes above the box where the picture has room', () => {
    expect(place({ x: 10, y: 10, width: 20, height: 10 }).side).toBe('above');
  });

  test('goes below a box too near the top', () => {
    expect(place({ x: 10, y: 9, width: 20, height: 10 }).side).toBe('below');
    expect(place({ x: 10, y: 0, width: 20, height: 40 }).side).toBe('below');
  });

  test('goes inside only a box with no room above or below', () => {
    expect(place({ x: 10, y: 9, width: 20, height: 32 }).side).toBe('inside');
    expect(place({ x: 0, y: 0, width: 100, height: 50 }).side).toBe('inside');
  });

  test('lines up with the side of the box farther from the edge of the picture', () => {
    expect(place({ x: 10, y: 10, width: 20, height: 10 }).align).toBe('left');
    expect(place({ x: 40, y: 10, width: 20, height: 10 }).align).toBe('left');
    expect(place({ x: 70, y: 10, width: 20, height: 10 }).align).toBe('right');
  });
});

test('sameRect compares every side, and two missing rectangles are the same', () => {
  const rect = { x: 1, y: 2, width: 3, height: 4 };
  expect(sameRect(rect, { ...rect })).toBe(true);
  expect(sameRect(rect, { ...rect, x: 0 })).toBe(false);
  expect(sameRect(rect, { ...rect, y: 0 })).toBe(false);
  expect(sameRect(rect, { ...rect, width: 0 })).toBe(false);
  expect(sameRect(rect, { ...rect, height: 0 })).toBe(false);
  expect(sameRect(rect, null)).toBe(false);
  expect(sameRect(null, rect)).toBe(false);
  expect(sameRect(null, null)).toBe(true);
});

describe('bounding', () => {
  test('holds every rectangle', () => {
    expect(
      bounding([
        { x: 10, y: 5, width: 4, height: 2 },
        { x: 2, y: 6, width: 3, height: 8 },
      ]),
    ).toEqual({ x: 2, y: 5, width: 12, height: 9 });
    expect(bounding([{ x: 1, y: 2, width: 3, height: 4 }])).toEqual({ x: 1, y: 2, width: 3, height: 4 });
  });

  test('is null with no rectangle', () => {
    expect(bounding([])).toBeNull();
  });
});
