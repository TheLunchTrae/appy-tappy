import { describe, expect, test } from 'bun:test';

import { buildCharacters, buildSpec, DEFAULT_TOLERANCE, EMPTY_DRAFT, reviewSpec, withKind } from '../src/lib/check.ts';

const region = { x: 1, y: 2, width: 3, height: 4 };
const red = { red: 255, green: 0, blue: 0 };
const gray = { red: 40, green: 40, blue: 40 };
const percent = { app: 'example', id: 'percent' };

describe('buildSpec', () => {
  test('asks for a region first', () => {
    expect(buildSpec(EMPTY_DRAFT, 'example')).toEqual({ ok: false, next: 'Draw a region on a screenshot.' });
  });

  test("builds a number from the application's reference and whether it is whole", () => {
    const pick = 'Pick a reference to read the number with, or make one from this region.';
    expect(buildSpec({ ...EMPTY_DRAFT, region }, 'example')).toEqual({ ok: false, next: pick });
    // An application not saved yet has no references.
    expect(buildSpec({ ...EMPTY_DRAFT, region, reference: 'percent' }, null)).toEqual({ ok: false, next: pick });
    expect(buildSpec({ ...EMPTY_DRAFT, region, reference: 'percent', whole: true }, 'example')).toEqual({
      ok: true,
      spec: { kind: 'number', region, reference: percent, whole: true },
    });
  });

  test('builds a bar fill from both colors', () => {
    const bar = withKind({ ...EMPTY_DRAFT, region }, 'barFill');
    expect(buildSpec(bar, null)).toEqual({ ok: false, next: "Pick the bar's filled color." });
    expect(buildSpec({ ...bar, filled: red }, null)).toEqual({ ok: false, next: "Pick the bar's empty color." });
    expect(buildSpec({ ...bar, filled: red, empty: gray }, null)).toEqual({
      ok: true,
      spec: { kind: 'barFill', region, filled: red, empty: gray },
    });
  });

  test('builds a color and an image check', () => {
    const color = withKind({ ...EMPTY_DRAFT, region }, 'color');
    expect(buildSpec(color, null)).toEqual({ ok: false, next: 'Pick the color to look for.' });
    expect(buildSpec({ ...color, color: red }, null)).toEqual({
      ok: true,
      spec: { kind: 'color', region, color: red, tolerance: DEFAULT_TOLERANCE.color },
    });
    const image = withKind({ ...EMPTY_DRAFT, region }, 'image');
    expect(buildSpec(image, null)).toEqual({ ok: false, next: 'Choose the screenshot to compare against.' });
    expect(buildSpec({ ...image, image: 3 }, null)).toEqual({
      ok: true,
      spec: { kind: 'image', region, screenshot: 3, tolerance: DEFAULT_TOLERANCE.image },
    });
  });

  test('an image check compares only what was selected while it ignores the background', () => {
    const steps = [{ kind: 'click', x: 4, y: 5, combine: 'replace' }] as const;
    const picture = { ...withKind({ ...EMPTY_DRAFT, region }, 'image'), image: 3, steps, colorMatch: 30 };
    expect(buildSpec(picture, 'app')).toEqual({
      ok: true,
      spec: {
        kind: 'image',
        region,
        screenshot: 3,
        tolerance: DEFAULT_TOLERANCE.image,
        selection: { tolerance: 30, steps },
      },
    });
    const whole = { kind: 'image', region, screenshot: 3, tolerance: DEFAULT_TOLERANCE.image } as const;
    expect(buildSpec({ ...picture, ignoreBackground: false }, 'app')).toEqual({ ok: true, spec: whole });
    expect(buildSpec({ ...picture, steps: [] }, 'app')).toEqual({ ok: true, spec: whole });
  });
});

describe('buildCharacters', () => {
  test('teaches a new reference its labels, in screenshot order, skipping blank ones', () => {
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
    expect(buildCharacters(draft, null)).toEqual({
      ok: true,
      spec: {
        kind: 'characters',
        region,
        reference: null,
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

  test('teaches kept recorded readings alone, in reading order', () => {
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
    const built = buildCharacters(draft, null);
    expect(built.ok && built.spec.recorded).toEqual([
      { reading: 1, text: '8%' },
      { reading: 4, text: '9%' },
    ]);
  });

  test('asks for what a new reference needs', () => {
    expect(buildCharacters(EMPTY_DRAFT, null)).toEqual({
      ok: false,
      next: 'Draw a region around the characters on a screenshot.',
    });
    expect(buildCharacters({ ...EMPTY_DRAFT, region }, null)).toEqual({
      ok: false,
      next: "Pick the characters' color.",
    });
    expect(buildCharacters({ ...EMPTY_DRAFT, region, ink: red, labels: new Map([[0, ' ']]) }, null)).toEqual({
      ok: false,
      next: 'Type what the region shows on a screenshot, or record it.',
    });
  });

  test('a saved reference needs nothing new to be tried', () => {
    const built = buildCharacters({ ...EMPTY_DRAFT, region, ink: red }, percent);
    expect(built.ok && built.spec.reference).toEqual(percent);
  });
});

test('reviewSpec waits for the region and color, and names the reference', () => {
  expect(reviewSpec({ ...EMPTY_DRAFT, region }, [], null)).toBeNull();
  const kept = [{ reading: 0, text: '5' }];
  expect(reviewSpec({ ...EMPTY_DRAFT, region, ink: red }, kept, percent)).toEqual({
    kind: 'characters',
    region,
    reference: percent,
    ink: red,
    tolerance: DEFAULT_TOLERANCE.number,
    labels: [],
    recorded: kept,
  });
});

test('withKind keeps the region and colors and resets the tolerance', () => {
  const draft = withKind({ ...EMPTY_DRAFT, region, ink: red, tolerance: 99 }, 'color');
  expect(draft.kind).toBe('color');
  expect(draft.region).toEqual(region);
  expect(draft.ink).toEqual(red);
  expect(draft.tolerance).toBe(DEFAULT_TOLERANCE.color);
});
