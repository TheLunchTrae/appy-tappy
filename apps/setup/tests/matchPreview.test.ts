import { describe, expect, test } from 'bun:test';

import type { Layer, MatchView } from '../src/lib/bindings.ts';
import {
  type Preview,
  type PreviewDraft,
  type PreviewRequest,
  NO_PREVIEW,
  barLegend,
  countIn,
  inkLines,
  layersOf,
  nearLine,
  paint,
  plan,
  share,
  step,
  tinted,
  tintsFor,
} from '../src/lib/matchPreview.ts';

const WHITE = { red: 255, green: 255, blue: 255 };
const GREEN = { red: 40, green: 210, blue: 60 };
const PINK = { red: 250, green: 60, blue: 200 };
const REGION = { x: 2, y: 1, width: 4, height: 2 };
const SIZE = { width: 10, height: 6 };
const WHOLE = { x: 0, y: 0, ...SIZE };

const draft = (parts: Partial<PreviewDraft>): PreviewDraft => ({
  ink: null,
  tolerance: 40,
  filled: null,
  empty: null,
  color: null,
  region: null,
  ...parts,
});

const layer = (spans: Layer['spans'], count?: number): Layer => ({
  count: count ?? (spans ?? []).reduce((sum, [start, end]) => sum + end - start, 0),
  spans,
});

describe('plan', () => {
  test('has nothing to show on the live view, and says a screenshot is needed', () => {
    expect(plan('ink', draft({ ink: WHITE }), null, null)).toEqual({
      kind: 'blocked',
      why: 'Matching pixels show on a screenshot. Select one, or take one.',
    });
  });

  test("looks for a reference's characters on the whole screenshot, whatever the region", () => {
    expect(plan('ink', draft({ ink: WHITE, region: REGION }), 3, SIZE)).toEqual({
      kind: 'ask',
      request: { screenshot: 3, area: WHOLE, rule: { kind: 'ink', ink: WHITE, tolerance: 40 } },
    });
    expect(plan('ink', draft({}), 3, SIZE).kind).toBe('blocked');
    expect(plan('ink', draft({ ink: WHITE }), 3, null).kind).toBe('blocked');
  });

  test("looks for a bar's and a color check's pixels in the region", () => {
    const bar = draft({ filled: GREEN, empty: WHITE, region: REGION });
    expect(plan('barFill', bar, 0, SIZE)).toEqual({
      kind: 'ask',
      request: { screenshot: 0, area: REGION, rule: { kind: 'barFill', filled: GREEN, empty: WHITE } },
    });
    expect(plan('barFill', { ...bar, empty: null }, 0, SIZE).kind).toBe('blocked');
    expect(plan('barFill', { ...bar, filled: null }, 0, SIZE).kind).toBe('blocked');
    expect(plan('barFill', { ...bar, region: null }, 0, SIZE).kind).toBe('blocked');
    const color = draft({ color: PINK, tolerance: 7, region: REGION });
    expect(plan('color', color, 1, SIZE)).toEqual({
      kind: 'ask',
      request: { screenshot: 1, area: REGION, rule: { kind: 'color', color: PINK, tolerance: 7 } },
    });
    expect(plan('color', { ...color, color: null }, 1, SIZE).kind).toBe('blocked');
    expect(plan('color', { ...color, region: null }, 1, SIZE).kind).toBe('blocked');
  });
});

const request = (tolerance: number, screenshot = 0, area = WHOLE): PreviewRequest => ({
  screenshot,
  area,
  rule: { kind: 'ink', ink: WHITE, tolerance },
});
const view = (count: number): MatchView => ({ kind: 'ink', ink: layer([[0, count]]) });

describe('step', () => {
  const asked = (from: Preview, id: number): Preview => step(from, { kind: 'asked', id });

  test("shows the newest request's answer", () => {
    const answered = step(asked(NO_PREVIEW, 1), { kind: 'answered', id: 1, request: request(10), view: view(4) });
    expect(answered).toEqual({ awaiting: null, shown: { request: request(10), view: view(4) }, error: null });
  });

  test('drops an answer or a failure that carries an older request number', () => {
    const waiting = asked(asked(NO_PREVIEW, 1), 2);
    expect(step(waiting, { kind: 'answered', id: 1, request: request(10), view: view(4) })).toBe(waiting);
    expect(step(waiting, { kind: 'failed', id: 1, message: 'gone' })).toBe(waiting);
    const answered = step(waiting, { kind: 'answered', id: 2, request: request(20), view: view(9) });
    expect(answered.shown).toEqual({ request: request(20), view: view(9) });
    // The older answer arrives after the newer one: the newer one stays.
    expect(step(answered, { kind: 'answered', id: 1, request: request(10), view: view(4) })).toBe(answered);
  });

  test('drops an answer that arrives after the preview is turned off', () => {
    const off = step(asked(NO_PREVIEW, 1), { kind: 'cleared' });
    expect(off).toEqual(NO_PREVIEW);
    expect(step(off, { kind: 'answered', id: 1, request: request(10), view: view(4) })).toBe(off);
    expect(step(off, { kind: 'failed', id: 1, message: 'gone' })).toBe(off);
  });

  test('keeps the last answer while the next is awaited, and forgets it when the next fails', () => {
    const shown = step(asked(NO_PREVIEW, 1), { kind: 'answered', id: 1, request: request(10), view: view(4) });
    const waiting = asked(shown, 2);
    expect(waiting.shown).toEqual(shown.shown);
    expect(step(waiting, { kind: 'failed', id: 2, message: 'That screenshot is gone.' })).toEqual({
      awaiting: null,
      shown: null,
      error: 'That screenshot is gone.',
    });
    // Asking again clears the failure.
    expect(asked(step(waiting, { kind: 'failed', id: 2, message: 'gone' }), 3).error).toBeNull();
  });
});

describe('tinted', () => {
  const shown: Preview = { awaiting: 2, shown: { request: request(10), view: view(4) }, error: null };

  test('keeps the tint while another tolerance is asked about on the same rectangle', () => {
    expect(tinted(shown, request(11))).toBe(shown.shown);
  });

  test('shows nothing once the preview is off, or for another screenshot, rectangle, or kind of rule', () => {
    expect(tinted(shown, null)).toBeNull();
    expect(tinted(NO_PREVIEW, request(10))).toBeNull();
    expect(tinted(shown, request(10, 1))).toBeNull();
    expect(tinted(shown, request(10, 0, REGION))).toBeNull();
    expect(tinted(shown, { ...request(10), rule: { kind: 'color', color: WHITE, tolerance: 10 } })).toBeNull();
  });
});

describe('tints', () => {
  const magenta = { red: 255, green: 0, blue: 255 };
  const green = { red: 0, green: 255, blue: 0 };

  test('take whichever of magenta and green stands out more against the picked color', () => {
    expect(tintsFor({ kind: 'ink', ink: GREEN, tolerance: 0 })).toEqual([{ color: magenta, striped: false }]);
    expect(tintsFor({ kind: 'ink', ink: PINK, tolerance: 0 })).toEqual([{ color: green, striped: false }]);
    expect(tintsFor({ kind: 'color', color: PINK, tolerance: 0 })).toEqual([{ color: green, striped: false }]);
  });

  test("give a bar's empty pixels the other color, in stripes", () => {
    expect(tintsFor({ kind: 'barFill', filled: GREEN, empty: WHITE })).toEqual([
      { color: magenta, striped: false },
      { color: green, striped: true },
    ]);
    expect(tintsFor({ kind: 'barFill', filled: PINK, empty: WHITE })).toEqual([
      { color: green, striped: false },
      { color: magenta, striped: true },
    ]);
  });

  test('pair with the layers of each kind of answer in order', () => {
    const [filled, empty, near] = [layer([[0, 1]]), layer([[1, 2]]), layer([[2, 3]])];
    expect(layersOf({ kind: 'barFill', filled, empty })).toEqual([filled, empty]);
    expect(layersOf({ kind: 'ink', ink: near })).toEqual([near]);
    expect(layersOf({ kind: 'color', near, average: WHITE, distance: 0, matches: true })).toEqual([near]);
  });
});

describe('paint', () => {
  const magenta = { color: { red: 255, green: 0, blue: 255 }, striped: false };
  const stripes = { color: { red: 0, green: 255, blue: 0 }, striped: true };
  const pixel = (pixels: Uint8ClampedArray, index: number): number[] => [...pixels.subarray(index * 4, index * 4 + 4)];

  test('tints the pixels of each run and leaves the rest clear', () => {
    // A 4x2 rectangle with a run that goes on from the first row into the second.
    const pixels = paint(
      [
        layer([
          [1, 2],
          [3, 5],
        ]),
      ],
      [magenta],
      4,
      2,
    );
    expect(pixels.length).toBe(32);
    const tintedAt = [0, 1, 2, 3, 4, 5, 6, 7].filter((index) => (pixel(pixels, index)[3] ?? 0) > 0);
    expect(tintedAt).toEqual([1, 3, 4]);
    expect(pixel(pixels, 1).slice(0, 3)).toEqual([255, 0, 255]);
    const alpha = pixel(pixels, 1)[3] ?? 0;
    expect(alpha).toBeGreaterThan(100);
    expect(alpha).toBeLessThan(200);
  });

  test('tints a striped layer on two of every four diagonals, in its own color', () => {
    const pixels = paint([layer([[0, 2]]), layer([[2, 16]])], [magenta, stripes], 8, 2);
    expect(pixel(pixels, 0).slice(0, 3)).toEqual([255, 0, 255]);
    const striped = [...Array.from({ length: 16 }).keys()].filter((index) => pixel(pixels, index)[1] === 255);
    // Column plus row, modulo 4, is 0 or 1.
    expect(striped).toEqual([4, 5, 8, 11, 12, 15]);
    expect(pixel(pixels, 2)).toEqual([0, 0, 0, 0]);
  });

  test('shows no matches as a clear rectangle and every match as a full one', () => {
    expect(paint([layer([])], [magenta], 3, 2).every((byte) => byte === 0)).toBe(true);
    const all = paint([layer([[0, 6]])], [magenta], 3, 2);
    expect([0, 1, 2, 3, 4, 5].every((index) => (pixel(all, index)[3] ?? 0) > 0)).toBe(true);
  });

  test('leaves a layer with too many runs to draw clear', () => {
    expect(paint([layer(null, 5)], [magenta], 3, 2).every((byte) => byte === 0)).toBe(true);
  });
});

describe('countIn', () => {
  // Runs in a 10x6 rectangle: all of row 1, and the last pixel of row 2 through the first three of row 3.
  const spans = [
    [10, 20],
    [29, 33],
  ] as const;

  test('counts the pixels of runs that lie in the region', () => {
    // Columns 2 to 5 of rows 1 and 2: four from the first run, none from the second.
    expect(countIn(spans, WHOLE, REGION)).toBe(4);
    // Columns 0 to 2 of rows 2 and 3 hold the three pixels that start row 3.
    expect(countIn(spans, WHOLE, { x: 0, y: 2, width: 3, height: 2 })).toBe(3);
    expect(countIn(spans, WHOLE, { x: 9, y: 2, width: 1, height: 1 })).toBe(1);
    expect(countIn(spans, WHOLE, WHOLE)).toBe(14);
  });

  test('places the runs where their rectangle is on the screenshot', () => {
    const at = { x: 100, y: 50, width: 10, height: 6 };
    expect(countIn(spans, at, { x: 102, y: 51, width: 4, height: 2 })).toBe(4);
    expect(countIn(spans, at, REGION)).toBe(0);
    // A region that reaches past the rectangle counts only what is inside it.
    expect(countIn(spans, at, { x: 90, y: 40, width: 15, height: 12 })).toBe(5);
  });
});

test('a share reads 0% and 100% only for none and for all', () => {
  expect(share(0, 200)).toBe('0%');
  expect(share(200, 200)).toBe('100%');
  expect(share(1, 200)).toBe('under 1%');
  expect(share(2, 200)).toBe('1%');
  expect(share(199, 200)).toBe('over 99%');
  expect(share(198, 200)).toBe('99%');
  expect(share(50, 200)).toBe('25%');
  expect(share(0, 0)).toBe('0%');
});

describe('what the tint shows, in words', () => {
  test("counts a reference's characters in the region and elsewhere", () => {
    // Three pixels in the region and two outside it.
    const ink = layer([
      [12, 15],
      [40, 42],
    ]);
    expect(inkLines(ink, WHOLE, REGION)).toEqual([
      '3 of 8 pixels in the region (38%) count as characters, and 2 pixels elsewhere on the screenshot.',
    ]);
    expect(inkLines(ink, WHOLE, null)).toEqual(['5 of 60 pixels on the screenshot (8%) count as characters.']);
    // A region drawn on a larger screenshot does not lie on this one.
    expect(inkLines(ink, WHOLE, { x: 8, y: 1, width: 4, height: 2 })).toEqual([
      '5 of 60 pixels on the screenshot (8%) count as characters.',
    ]);
    expect(inkLines(layer([[12, 15]]), WHOLE, REGION)).toEqual([
      '3 of 8 pixels in the region (38%) count as characters, and none elsewhere on the screenshot.',
    ]);
  });

  test('says when no pixel matches, when the whole region does, and when only pixels outside it do', () => {
    expect(inkLines(layer([]), WHOLE, REGION)).toEqual([
      'No pixel on the screenshot counts as characters. Raise the tolerance, or pick the color again.',
    ]);
    expect(inkLines(layer([[0, 60]]), WHOLE, REGION)).toEqual([
      'Every pixel in the region counts as characters, and 52 pixels elsewhere on the screenshot. Lower the tolerance until only the characters do.',
    ]);
    expect(inkLines(layer([[40, 41]]), WHOLE, REGION)).toEqual([
      'No pixel in the region counts as characters; 1 pixel elsewhere on the screenshot does.',
    ]);
    expect(inkLines(layer(null, 30), WHOLE, REGION)).toEqual([
      'Too many separate areas match to tint them all, so none is tinted. 30 of 60 pixels on the screenshot (50%) count as characters. Lower the tolerance.',
    ]);
  });

  test("lists a bar's filled, empty, and other pixels", () => {
    const tints = tintsFor({ kind: 'barFill', filled: GREEN, empty: WHITE });
    expect(barLegend(layer([[0, 5]]), layer([[5, 7]]), REGION, tints)).toEqual({
      entries: [
        { name: 'Filled', count: '5 (63%)', tint: tints[0] ?? null },
        { name: 'Empty', count: '2 (25%)', tint: tints[1] ?? null },
        { name: 'Neither, left out', count: '1 (13%)', tint: null },
      ],
      note: null,
    });
    expect(barLegend(layer([]), layer([]), REGION, tints).note).toBe(
      'No pixel in the region is near the filled or the empty color. Pick the colors again.',
    );
    expect(barLegend(layer([[0, 8]]), layer([]), REGION, tints).entries.map((entry) => entry.count)).toEqual([
      '8 (100%)',
      '0 (0%)',
      '0 (0%)',
    ]);
    expect(barLegend(layer(null, 4), layer([[0, 1]]), REGION, tints).note).toBe(
      'Too many separate areas match to tint them all, so none is tinted.',
    );
  });

  test("counts a color check's pixels that are within the tolerance on their own", () => {
    expect(nearLine(layer([]), REGION)).toBe('No pixel in the region is within the tolerance on its own.');
    expect(nearLine(layer([[0, 8]]), REGION)).toBe('Every pixel in the region is within the tolerance on its own.');
    expect(nearLine(layer([[0, 6]]), REGION)).toBe(
      '6 of 8 pixels in the region (75%) are within the tolerance on their own.',
    );
    expect(nearLine(layer([[3, 4]]), REGION)).toBe(
      '1 of 8 pixels in the region (13%) is within the tolerance on its own.',
    );
  });
});
