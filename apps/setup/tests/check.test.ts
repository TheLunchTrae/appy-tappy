import { describe, expect, test } from 'bun:test';

import { buildSpec, DEFAULT_TOLERANCE, EMPTY_DRAFT, withKind } from '../src/lib/check.ts';

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
        recorded: [],
      },
    });
  });

  test('builds a number from kept recorded readings alone, in reading order', () => {
    const draft = {
      ...EMPTY_DRAFT,
      region,
      ink: red,
      recorded: new Map([
        [4, '9%'],
        [1, ' 8% '],
        [2, ''],
      ]),
    };
    expect(buildSpec(draft)).toEqual({
      ok: true,
      spec: {
        kind: 'number',
        region,
        ink: red,
        tolerance: DEFAULT_TOLERANCE.number,
        labels: [],
        recorded: [
          { reading: 1, text: '8%' },
          { reading: 4, text: '9%' },
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
      next: 'Type what the region shows on a screenshot, or record it.',
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

  test('a picture check compares only what was selected while it ignores the background', () => {
    const steps = [{ kind: 'click', x: 4, y: 5, combine: 'replace' }] as const;
    const picture = { ...withKind({ ...EMPTY_DRAFT, region }, 'reference'), reference: 3, steps, colorMatch: 30 };
    expect(buildSpec(picture)).toEqual({
      ok: true,
      spec: {
        kind: 'reference',
        region,
        screenshot: 3,
        tolerance: DEFAULT_TOLERANCE.reference,
        selection: { tolerance: 30, steps },
      },
    });
    const whole = { kind: 'reference', region, screenshot: 3, tolerance: DEFAULT_TOLERANCE.reference } as const;
    expect(buildSpec({ ...picture, ignoreBackground: false })).toEqual({ ok: true, spec: whole });
    expect(buildSpec({ ...picture, steps: [] })).toEqual({ ok: true, spec: whole });
  });
});

test('withKind keeps the region and colors and resets the tolerance', () => {
  const draft = withKind({ ...EMPTY_DRAFT, region, ink: red, tolerance: 99 }, 'color');
  expect(draft.kind).toBe('color');
  expect(draft.region).toEqual(region);
  expect(draft.ink).toEqual(red);
  expect(draft.tolerance).toBe(DEFAULT_TOLERANCE.color);
});
