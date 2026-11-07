import { describe, expect, test } from 'bun:test';

import { buildSpec, DEFAULT_TOLERANCE, EMPTY_DRAFT, withKind } from '../src/check.ts';

const region = { x: 1, y: 2, width: 3, height: 4 };
const red = { red: 255, green: 0, blue: 0 };
const gray = { red: 40, green: 40, blue: 40 };

describe('buildSpec', () => {
  test('asks for a region first', () => {
    expect(buildSpec(EMPTY_DRAFT)).toEqual({ ok: false, next: 'Draw a region on a screenshot.' });
  });

  test('builds a number from its ink and labels, in screenshot order, skipping blank ones', () => {
    const draft = {
      ...EMPTY_DRAFT,
      region,
      ink: red,
      labels: new Map([
        [2, ' 87% '],
        [0, '12%'],
        [1, '  '],
      ]),
    };
    expect(buildSpec(draft)).toEqual({
      ok: true,
      spec: {
        kind: 'number',
        region,
        ink: red,
        tolerance: DEFAULT_TOLERANCE.number,
        labels: [
          { screenshot: 0, text: '12%' },
          { screenshot: 2, text: '87%' },
        ],
      },
    });
  });

  test('asks for what a number needs', () => {
    expect(buildSpec({ ...EMPTY_DRAFT, region })).toEqual({
      ok: false,
      next: "Pick the color of the number's characters.",
    });
    expect(buildSpec({ ...EMPTY_DRAFT, region, ink: red, labels: new Map([[0, ' ']]) })).toEqual({
      ok: false,
      next: 'Type what the region shows on a screenshot.',
    });
  });

  test('builds a bar fill from both colors', () => {
    const bar = withKind({ ...EMPTY_DRAFT, region }, 'barFill');
    expect(buildSpec(bar)).toEqual({ ok: false, next: "Pick the bar's filled color." });
    expect(buildSpec({ ...bar, filled: red })).toEqual({ ok: false, next: "Pick the bar's empty color." });
    expect(buildSpec({ ...bar, filled: red, empty: gray })).toEqual({
      ok: true,
      spec: { kind: 'barFill', region, filled: red, empty: gray },
    });
  });

  test('builds a color and a picture check', () => {
    const color = withKind({ ...EMPTY_DRAFT, region }, 'color');
    expect(buildSpec(color)).toEqual({ ok: false, next: 'Pick the color to look for.' });
    expect(buildSpec({ ...color, color: red })).toEqual({
      ok: true,
      spec: { kind: 'color', region, color: red, tolerance: DEFAULT_TOLERANCE.color },
    });
    const picture = withKind({ ...EMPTY_DRAFT, region }, 'reference');
    expect(buildSpec(picture)).toEqual({ ok: false, next: 'Choose the screenshot to compare against.' });
    expect(buildSpec({ ...picture, reference: 3 })).toEqual({
      ok: true,
      spec: { kind: 'reference', region, screenshot: 3, tolerance: DEFAULT_TOLERANCE.reference },
    });
  });
});

test('withKind keeps the region and colors and resets the tolerance', () => {
  const draft = withKind({ ...EMPTY_DRAFT, region, ink: red, tolerance: 99 }, 'color');
  expect(draft.kind).toBe('color');
  expect(draft.region).toEqual(region);
  expect(draft.ink).toEqual(red);
  expect(draft.tolerance).toBe(DEFAULT_TOLERANCE.color);
});
