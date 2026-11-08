import { describe, expect, test } from 'bun:test';

import type { SelectStep } from '../src/lib/bindings.ts';
import {
  type Selection,
  combineOf,
  ignoredPixels,
  keptCount,
  madeFor,
  refit,
  shade,
  withStep,
} from '../src/lib/selection.ts';

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

describe('selections', () => {
  const steps = [click(1, 'replace')];
  const box = (x: number) => ({ x, y: 0, width: 2, height: 2 });
  const made = (of: readonly SelectStep[], x: number | null, colorMatch = 24): Selection => ({
    steps: of,
    colorMatch,
    view: x === null ? null : { region: box(x), spans: [] },
  });

  test("are a state's only for its steps and color match", () => {
    const selections = new Map([[3, made(steps, 0)]]);
    expect(madeFor(selections, { screenshot: 3, steps }, 24)).toBe(selections.get(3) ?? null);
    expect(madeFor(selections, { screenshot: 3, steps: [...steps] }, 24)).toBeNull();
    expect(madeFor(selections, { screenshot: 3, steps }, 30)).toBeNull();
    expect(madeFor(selections, { screenshot: 4, steps }, 24)).toBeNull();
  });

  test('fit the region around what every state selects', () => {
    const other = [click(9, 'replace')];
    const fit = refit([made(steps, 0), made(other, 6)], null, null);
    expect(fit.region).toEqual({ x: 0, y: 0, width: 8, height: 2 });
    expect([...(fit.fitted?.steps ?? [])]).toEqual([steps, other]);
  });

  test('refit a region changed by hand only for steps that select something new', () => {
    const last = { steps: new Set([steps]), region: box(0) };
    const byHand = box(5);
    const newer = [click(2, 'replace')];
    expect(refit([made(newer, 4)], last, byHand).region).toEqual(box(4));
    // A new color match for steps seen before leaves the hand edit.
    const kept = refit([made(steps, 4, 30)], last, byHand);
    expect(kept.region).toBeNull();
    expect(kept.fitted?.region).toEqual(box(0));
    // Steps that select nothing leave it too, and are no longer new to it.
    const nothing = refit([made(newer, null)], last, byHand);
    expect(nothing.region).toBeNull();
    expect([...(nothing.fitted?.steps ?? [])]).toEqual([steps, newer]);
  });

  test('refit a region left as fitted for a new color match', () => {
    const last = { steps: new Set([steps]), region: box(0) };
    expect(refit([made(steps, 4, 30)], last, box(0)).region).toEqual(box(4));
  });

  test('leave the region before any selection finds something', () => {
    expect(refit([made(steps, null)], null, box(0))).toEqual({ fitted: null, region: null });
  });
});
