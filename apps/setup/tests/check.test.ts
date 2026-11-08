import { describe, expect, test } from 'bun:test';

import {
  buildCharacters,
  buildSpec,
  DEFAULT_TOLERANCE,
  EMPTY_DRAFT,
  fromReference,
  newestOfSize,
  reviewSpec,
  settingsDiffer,
  toReference,
  withKind,
} from '../src/lib/check.ts';

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
    expect(buildSpec({ ...EMPTY_DRAFT, region, reference: 'percent', whole: true }, 'example')).toEqual({
      ok: true,
      spec: { kind: 'number', region, reference: percent, whole: true },
    });
  });

  test('builds a bar fill from both colors', () => {
    const bar = withKind({ ...EMPTY_DRAFT, region }, 'barFill');
    expect(buildSpec(bar, 'example')).toEqual({ ok: false, next: "Pick the bar's filled color." });
    expect(buildSpec({ ...bar, filled: red }, 'example')).toEqual({ ok: false, next: "Pick the bar's empty color." });
    expect(buildSpec({ ...bar, filled: red, empty: gray }, 'example')).toEqual({
      ok: true,
      spec: { kind: 'barFill', region, filled: red, empty: gray },
    });
  });

  test('builds a color and an image check', () => {
    const color = withKind({ ...EMPTY_DRAFT, region }, 'color');
    expect(buildSpec(color, 'example')).toEqual({ ok: false, next: 'Pick the color to look for.' });
    expect(buildSpec({ ...color, color: red }, 'example')).toEqual({
      ok: true,
      spec: { kind: 'color', region, color: red, tolerance: DEFAULT_TOLERANCE.color },
    });
    const image = withKind({ ...EMPTY_DRAFT, region, drawnOn: 1 }, 'image');
    expect(buildSpec(image, 'example')).toEqual({ ok: false, next: 'Choose the screenshot to compare against.' });
    expect(buildSpec({ ...image, image: 3 }, 'example')).toEqual({
      ok: true,
      spec: { kind: 'image', region, drawnOn: 1, screenshot: 3, tolerance: DEFAULT_TOLERANCE.image },
    });
    expect(buildSpec({ ...image, image: 3, drawnOn: null }, 'example')).toEqual({
      ok: false,
      next: 'Draw a region on a screenshot.',
    });
  });

  test('an image check compares only what was selected while it ignores the background', () => {
    const steps = [{ kind: 'click', x: 4, y: 5, combine: 'replace' }] as const;
    const picture = { ...withKind({ ...EMPTY_DRAFT, region, drawnOn: 3 }, 'image'), image: 3, steps, colorMatch: 30 };
    expect(buildSpec(picture, 'app')).toEqual({
      ok: true,
      spec: {
        kind: 'image',
        region,
        drawnOn: 3,
        screenshot: 3,
        tolerance: DEFAULT_TOLERANCE.image,
        selection: { tolerance: 30, steps },
      },
    });
    const whole = { kind: 'image', region, drawnOn: 3, screenshot: 3, tolerance: DEFAULT_TOLERANCE.image } as const;
    expect(buildSpec({ ...picture, ignoreBackground: false }, 'app')).toEqual({ ok: true, spec: whole });
    expect(buildSpec({ ...picture, steps: [] }, 'app')).toEqual({ ok: true, spec: whole });
  });
});

describe('buildCharacters', () => {
  test('teaches a new reference its labels, in screenshot order, skipping blank ones', () => {
    const draft = {
      ...EMPTY_DRAFT,
      region,
      drawnOn: 2,
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
        screenshot: 2,
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
      drawnOn: 0,
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
    expect(buildCharacters({ ...EMPTY_DRAFT, region, ink: red }, null)).toEqual({
      ok: false,
      next: 'Draw a region around the characters on a screenshot.',
    });
    expect(buildCharacters({ ...EMPTY_DRAFT, region, drawnOn: 0 }, null)).toEqual({
      ok: false,
      next: "Pick the characters' color.",
    });
    expect(
      buildCharacters({ ...EMPTY_DRAFT, region, drawnOn: 0, ink: red, labels: new Map([[0, ' ']]) }, null),
    ).toEqual({
      ok: false,
      next: 'Type what the region shows on a screenshot, or record it.',
    });
  });

  test('a saved reference needs nothing new to be tried', () => {
    const built = buildCharacters({ ...EMPTY_DRAFT, region, drawnOn: 0, ink: red }, percent);
    expect(built.ok && built.spec.reference).toEqual(percent);
  });
});

test('reviewSpec waits for the region and color, and names the reference', () => {
  expect(reviewSpec({ ...EMPTY_DRAFT, region, drawnOn: 1 }, [], null)).toBeNull();
  expect(reviewSpec({ ...EMPTY_DRAFT, region, ink: red }, [], null)).toBeNull();
  const kept = [{ reading: 0, text: '5' }];
  expect(reviewSpec({ ...EMPTY_DRAFT, region, drawnOn: 1, ink: red }, kept, percent)).toEqual({
    kind: 'characters',
    region,
    screenshot: 1,
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

describe('the reference editor', () => {
  const region = { x: 1, y: 2, width: 30, height: 10 };
  const taught = { x: 5, y: 6, width: 40, height: 12 };
  const saved = {
    id: 'percent',
    name: 'Percent',
    ink: red,
    tolerance: 25,
    samples: [{ image: 'ref-percent-1.png', text: '87%' }],
  };

  test("opens with the reference's characters and the value's region", () => {
    const value = { ...EMPTY_DRAFT, region, ink: gray, labels: new Map([[0, '5']]) };
    expect(toReference(value, saved)).toEqual({ ...value, ink: red, tolerance: 25, labels: new Map() });
    expect(toReference(value, null)).toEqual({
      ...value,
      ink: null,
      tolerance: DEFAULT_TOLERANCE.number,
      labels: new Map(),
    });
  });

  test("puts the value's region back however the reference was taught", () => {
    const editor = {
      ...toReference({ ...EMPTY_DRAFT, region, drawnOn: 0 }, saved),
      region: taught,
      drawnOn: 3,
      labels: new Map([[0, '100%']]),
    };
    expect(fromReference(editor, { region, drawnOn: 0 })).toEqual({
      ...editor,
      region,
      drawnOn: 0,
      labels: new Map(),
      recorded: new Map(),
    });
  });
});

describe('a stopped recording', () => {
  const settings = { region, ink: red, tolerance: 40, windowSize: { width: 1920, height: 1080 } };
  const recorded = { ...EMPTY_DRAFT, region, drawnOn: 0, ink: red, tolerance: 40 };
  const big = { width: 1920, height: 1080 };
  const small = { width: 1280, height: 720 };

  test('records the same while the draft has its settings at its window size', () => {
    expect(settingsDiffer({ ...recorded, region: { ...region } }, settings, { ...big })).toBe(false);
  });

  test('offers its settings back for any other region, color, tolerance or window size', () => {
    expect(settingsDiffer({ ...recorded, region: { ...region, x: 2 } }, settings, big)).toBe(true);
    expect(settingsDiffer({ ...recorded, ink: gray }, settings, big)).toBe(true);
    expect(settingsDiffer({ ...recorded, ink: null }, settings, big)).toBe(true);
    expect(settingsDiffer({ ...recorded, tolerance: 41 }, settings, big)).toBe(true);
    expect(settingsDiffer(recorded, settings, small)).toBe(true);
    expect(settingsDiffer(recorded, settings, null)).toBe(true);
  });

  test('goes back on the newest screenshot of its window size', () => {
    const screenshots = [
      [0, big],
      [1, small],
      [2, big],
      [3, small],
    ] as const;
    expect(newestOfSize(screenshots, { ...big })).toBe(2);
    expect(newestOfSize(screenshots, { width: 800, height: 600 })).toBeNull();
  });
});
