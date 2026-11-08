import { describe, expect, test } from 'bun:test';

import { bounding, contains, dragCorner, fit, fromCorners, moveRect, sameRect, toPicture } from '../src/lib/region.ts';

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

describe('dragCorner', () => {
  const rect = { x: 10, y: 10, width: 20, height: 10 };

  test('keeps the opposite corner in place', () => {
    expect(dragCorner(rect, 'se', { x: 40, y: 25 }, bounds)).toEqual({ x: 10, y: 10, width: 30, height: 15 });
    expect(dragCorner(rect, 'nw', { x: 5, y: 5 }, bounds)).toEqual({ x: 5, y: 5, width: 25, height: 15 });
    expect(dragCorner(rect, 'ne', { x: 35, y: 0 }, bounds)).toEqual({ x: 10, y: 0, width: 25, height: 20 });
    expect(dragCorner(rect, 'sw', { x: 0, y: 30 }, bounds)).toEqual({ x: 0, y: 10, width: 30, height: 20 });
  });

  test('flips past the opposite corner', () => {
    expect(dragCorner(rect, 'se', { x: 5, y: 5 }, bounds)).toEqual({ x: 5, y: 5, width: 5, height: 5 });
  });

  test('keeps the rectangle when the drag would empty it', () => {
    expect(dragCorner(rect, 'se', { x: 10, y: 40 }, bounds)).toEqual(rect);
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
