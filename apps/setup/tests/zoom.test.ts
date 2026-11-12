import { describe, expect, test } from 'bun:test';

import { toPicture } from '../src/lib/region.ts';
import { GUTTER, ZOOM_STEP, clampZoom, percent, scrollShift, shownAt, wheelZoom } from '../src/lib/zoom.ts';

const picture = { width: 1920, height: 1080 };
// The picture fits this space at half its size, with the gutter around it.
const space = { width: 960 + 2 * GUTTER, height: 700 };

describe('shownAt', () => {
  test('shows the whole picture with room around it at no zoom', () => {
    expect(shownAt(picture, space, 1)).toEqual({ width: 960, height: 540 });
  });

  test('grows with the zoom and keeps the shape', () => {
    expect(shownAt(picture, space, 3)).toEqual({ width: 2880, height: 1620 });
  });

  test('is empty with no picture or no space', () => {
    expect(shownAt({ width: 0, height: 0 }, space, 2)).toEqual({ width: 0, height: 0 });
    expect(shownAt(picture, { width: 2 * GUTTER, height: 300 }, 2)).toEqual({ width: 0, height: 0 });
  });
});

describe('clampZoom', () => {
  test('never shows less than the whole picture', () => {
    expect(clampZoom(0.4, picture, space)).toBe(1);
    expect(clampZoom(1 / ZOOM_STEP, picture, space)).toBe(1);
  });

  test('stops where a pixel of the picture shows eight wide', () => {
    expect(clampZoom(5, picture, space)).toBe(5);
    expect(clampZoom(400, picture, space)).toBe(16);
    expect(shownAt(picture, space, clampZoom(Infinity, picture, space)).width).toBe(picture.width * 8);
  });

  test('keeps a picture that already shows larger than that whole', () => {
    expect(clampZoom(3, { width: 20, height: 10 }, space)).toBe(1);
  });

  test('is 1 while there is nothing to show', () => {
    expect(clampZoom(3, picture, { width: 0, height: 0 })).toBe(1);
  });
});

test('the wheel zooms in when turned away, out when turned back, and doubles over 200', () => {
  expect(wheelZoom(2, -200)).toBe(4);
  expect(wheelZoom(2, 200)).toBe(1);
  expect(wheelZoom(2, 0)).toBe(2);
  expect(wheelZoom(1, -100)).toBeCloseTo(Math.SQRT2, 12);
});

test('percent says how large the picture shows against its own pixels', () => {
  expect(percent({ width: 960, height: 540 }, picture)).toBe('50%');
  expect(percent({ width: 2880, height: 1620 }, picture)).toBe('150%');
  expect(percent({ width: 0, height: 0 }, { width: 0, height: 0 })).toBe('');
});

describe('zooming around a point', () => {
  // The picture's box in its view at one zoom, as the page lays it out: centered while it fits, scrolled otherwise.
  const boxAt = (zoom: number, scroll: { x: number; y: number }) => {
    const shown = shownAt(picture, space, zoom);
    return {
      ...shown,
      left: Math.max((space.width - shown.width) / 2, GUTTER) - scroll.x,
      top: Math.max((space.height - shown.height) / 2, GUTTER) - scroll.y,
    };
  };
  const under = (anchor: { x: number; y: number }, box: ReturnType<typeof boxAt>) =>
    toPicture({ x: anchor.x - box.left, y: anchor.y - box.top }, box, picture);

  test('keeps the pixel under the anchor there at every zoom and position', () => {
    const anchors = [
      { x: 20, y: 90 },
      { x: 496, y: 350 },
      { x: 913.5, y: 612.25 },
    ];
    const scrolls = [
      { x: 0, y: 0 },
      { x: 311, y: 77.5 },
    ];
    for (const from of [1, 1.5, 4, 16]) {
      for (const to of [1.5, 2.25, 7.3, 16]) {
        for (const scroll of scrolls) {
          for (const anchor of anchors) {
            const before = boxAt(from, scroll);
            const point = under(anchor, before);
            // The picture has its new size before the view scrolls.
            const resized = boxAt(to, scroll);
            const shift = scrollShift(point, anchor, resized, picture);
            const after = boxAt(to, { x: scroll.x + shift.x, y: scroll.y + shift.y });
            expect(under(anchor, after).x).toBeCloseTo(point.x, 8);
            expect(under(anchor, after).y).toBeCloseTo(point.y, 8);
          }
        }
      }
    }
  });

  test('scrolls nowhere when the pixel is already under the anchor', () => {
    const box = boxAt(4, { x: 100, y: 40 });
    const anchor = { x: 300, y: 200 };
    const shift = scrollShift(under(anchor, box), anchor, box, picture);
    expect(shift.x).toBeCloseTo(0, 9);
    expect(shift.y).toBeCloseTo(0, 9);
  });
});
