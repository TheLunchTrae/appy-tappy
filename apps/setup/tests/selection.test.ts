import { describe, expect, test } from 'bun:test';

import { type SelectStep, combineOf, ignoredPixels, keptCount, shade, withStep } from '../src/lib/selection.ts';

const click = (x: number, combine: SelectStep['combine']): SelectStep => ({ kind: 'click', x, y: 0, combine });

test('Shift adds, Alt takes away, and a plain press starts over', () => {
  expect(combineOf({ shiftKey: false, altKey: false })).toBe('replace');
  expect(combineOf({ shiftKey: true, altKey: false })).toBe('add');
  expect(combineOf({ shiftKey: false, altKey: true })).toBe('remove');
  expect(combineOf({ shiftKey: true, altKey: true })).toBe('remove');
});

describe('withStep', () => {
  const first = [click(1, 'replace')];

  test('adds a step that combines on the same screenshot', () => {
    expect(withStep(first, 2, 2, click(5, 'add'))).toEqual([click(1, 'replace'), click(5, 'add')]);
    expect(withStep(first, 2, 2, click(5, 'remove'))).toEqual([click(1, 'replace'), click(5, 'remove')]);
  });

  test('starts over for a plain step or another screenshot', () => {
    expect(withStep(first, 2, 2, click(5, 'replace'))).toEqual([click(5, 'replace')]);
    expect(withStep(first, 2, 4, click(5, 'add'))).toEqual([click(5, 'replace')]);
    expect(withStep([], null, 4, click(5, 'add'))).toEqual([click(5, 'replace')]);
  });

  test('ignores taking away from nothing', () => {
    expect(withStep([], null, 4, click(5, 'remove'))).toBeNull();
    expect(withStep([], 4, 4, click(5, 'remove'))).toBeNull();
    expect(withStep(first, 2, 4, click(5, 'remove'))).toBeNull();
  });
});

describe('ignoredPixels', () => {
  // A 3x2 box at (1, 1) that selects its first row and the last pixel of its second.
  const view = {
    region: { x: 1, y: 1, width: 3, height: 2 },
    spans: [
      [0, 3],
      [5, 6],
    ] as const,
  };

  test('leaves out the box pixels the selection does not keep', () => {
    expect([...ignoredPixels(view, view.region)]).toEqual([0, 0, 0, 1, 1, 0]);
  });

  test('leaves out what a wider box adds and keeps what a narrower one holds', () => {
    expect([...ignoredPixels(view, { x: 0, y: 1, width: 5, height: 1 })]).toEqual([1, 0, 0, 0, 1]);
    expect([...ignoredPixels(view, { x: 3, y: 1, width: 1, height: 2 })]).toEqual([0, 0]);
    expect([...ignoredPixels(view, { x: 0, y: 0, width: 1, height: 1 })]).toEqual([1]);
  });

  test('counts the pixels it keeps', () => {
    expect(keptCount(ignoredPixels(view, view.region))).toBe(4);
  });
});

test('shade tints only the left-out pixels, with light diagonal stripes', () => {
  // A 4x2 region; (3, 1) is on a stripe because x + y is a multiple of 4.
  const pixels = shade(Uint8Array.from([0, 1, 1, 1, 1, 0, 1, 1]), 4);
  const at = (index: number) => [...pixels.subarray(index * 4, index * 4 + 4)];
  expect(at(0)).toEqual([0, 0, 0, 0]);
  expect(at(5)).toEqual([0, 0, 0, 0]);
  expect(at(1)).toEqual([17, 19, 23, 160]);
  expect(at(4)).toEqual([17, 19, 23, 160]);
  expect(at(7)).toEqual([255, 255, 255, 110]);
});
