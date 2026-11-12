import { describe, expect, test } from 'bun:test';

import type { Layer, MatchView, SelectStep, SelectionView } from '../src/lib/bindings.ts';
import {
  type Preview,
  type PreviewDraft,
  type PreviewRequest,
  type Stepped,
  NO_PREVIEW,
  SELECTED_TINT,
  awaited,
  barLegend,
  countIn,
  inkLines,
  layersOf,
  nearLine,
  paint,
  paintedOf,
  plan,
  previewDriver,
  selectedPixels,
  share,
  step,
  tinted,
  tintsFor,
} from '../src/lib/matchPreview.ts';
import type { Rect } from '../src/lib/region.ts';
import type { Selection } from '../src/lib/selection.ts';

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
  const [a, b, c] = [request(10), request(20), request(30)];
  const wanted = (from: Preview, wants: PreviewRequest): Stepped => step(from, { kind: 'wanted', request: wants });
  const answered = (from: Preview, id: number, count: number): Stepped =>
    step(from, { kind: 'answered', id, view: view(count) });

  test('sends a request at once while none is in flight, under the next number', () => {
    const first = wanted(NO_PREVIEW, a);
    expect(first.send).toEqual({ id: 1, request: a });
    expect(first.preview).toEqual({ sent: 1, flight: { id: 1, request: a }, queued: null, shown: null, error: null });
    const idle = answered(first.preview, 1, 4);
    expect(idle).toEqual({
      preview: { sent: 1, flight: null, queued: null, shown: { request: a, view: view(4) }, error: null },
      send: null,
    });
    expect(wanted(idle.preview, b).send).toEqual({ id: 2, request: b });
  });

  test('sends only the first and the last of a burst of changes', () => {
    const first = wanted(NO_PREVIEW, a);
    const second = wanted(first.preview, b);
    expect(second.send).toBeNull();
    expect(second.preview.queued).toBe(b);
    // The newer request takes the queued one's place.
    const third = wanted(second.preview, c);
    expect(third.send).toBeNull();
    expect(third.preview.queued).toBe(c);
    expect(third.preview.flight).toEqual({ id: 1, request: a });
    const next = answered(third.preview, 1, 4);
    expect(next.send).toEqual({ id: 2, request: c });
    expect(next.preview).toEqual({
      sent: 2,
      flight: { id: 2, request: c },
      queued: null,
      shown: { request: a, view: view(4) },
      error: null,
    });
    const last = answered(next.preview, 2, 9);
    expect(last.send).toBeNull();
    expect(last.preview.flight).toBeNull();
    expect(last.preview.shown).toEqual({ request: c, view: view(9) });
  });

  test('queues nothing when the settings come back to what the request in flight asks', () => {
    const back = wanted(wanted(wanted(NO_PREVIEW, a).preview, b).preview, request(10));
    expect(back.preview.queued).toBeNull();
    expect(answered(back.preview, 1, 4).send).toBeNull();
  });

  test('drops the answer of a request in flight when the preview is turned off, and sends nothing more', () => {
    const queued = wanted(wanted(NO_PREVIEW, a).preview, b);
    const off = step(queued.preview, { kind: 'cleared' });
    expect(off).toEqual({ preview: { ...NO_PREVIEW, sent: 1 }, send: null });
    expect(answered(off.preview, 1, 4)).toEqual({ preview: off.preview, send: null });
    expect(step(off.preview, { kind: 'failed', id: 1, message: 'gone' })).toEqual({ preview: off.preview, send: null });
  });

  test('drops an answer or a failure that carries an older request number', () => {
    // Request 1 is still out when the preview is turned off and on again, which sends request 2.
    const off = step(wanted(NO_PREVIEW, a).preview, { kind: 'cleared' });
    const again = wanted(off.preview, b);
    expect(again.send).toEqual({ id: 2, request: b });
    expect(answered(again.preview, 1, 4)).toEqual({ preview: again.preview, send: null });
    expect(step(again.preview, { kind: 'failed', id: 1, message: 'gone' })).toEqual({
      preview: again.preview,
      send: null,
    });
    const newer = answered(again.preview, 2, 9);
    expect(newer.preview.shown).toEqual({ request: b, view: view(9) });
    // The older answer arrives last: the newer one stays.
    expect(answered(newer.preview, 1, 4)).toEqual({ preview: newer.preview, send: null });
  });

  test('shows why a request failed, and still sends the queued one', () => {
    const shown = answered(wanted(NO_PREVIEW, a).preview, 1, 4);
    const failed = step(wanted(shown.preview, b).preview, {
      kind: 'failed',
      id: 2,
      message: 'That screenshot is gone.',
    });
    expect(failed).toEqual({
      preview: { sent: 2, flight: null, queued: null, shown: null, error: 'That screenshot is gone.' },
      send: null,
    });
    const queued = wanted(wanted(shown.preview, b).preview, c);
    const next = step(queued.preview, { kind: 'failed', id: 2, message: 'gone' });
    expect(next.send).toEqual({ id: 3, request: c });
    expect(next.preview).toEqual({ sent: 3, flight: { id: 3, request: c }, queued: null, shown: null, error: null });
  });
});

describe('previewDriver', () => {
  /** A driver whose requests wait in `asks` until the test answers them, and the previews it showed. */
  const driven = () => {
    const asks: { request: PreviewRequest; answer: (view: MatchView) => void; fail: (error: unknown) => void }[] = [];
    const shown: Preview[] = [];
    const want = previewDriver(
      async (asked) =>
        new Promise<MatchView>((answer, fail) => {
          asks.push({ request: asked, answer, fail });
        }),
      (preview) => shown.push(preview),
      String,
    );
    return { asks, want, latest: () => shown.at(-1) };
  };
  /** Lets the answers given so far reach the driver. */
  const settle = async (): Promise<void> => {
    await new Promise((done) => {
      setTimeout(done, 0);
    });
  };
  const [a, b, c] = [request(10), request(20), request(30)];

  test('sends the first and the last of a burst, one at a time', async () => {
    const { asks, want, latest } = driven();
    want(a);
    want(b);
    want(c);
    expect(asks.map((ask) => ask.request)).toEqual([a]);
    asks[0]?.answer(view(4));
    await settle();
    expect(asks.map((ask) => ask.request)).toEqual([a, c]);
    expect(latest()?.shown).toEqual({ request: a, view: view(4) });
    asks[1]?.answer(view(9));
    await settle();
    expect(asks).toHaveLength(2);
    expect(latest()).toEqual({
      sent: 2,
      flight: null,
      queued: null,
      shown: { request: c, view: view(9) },
      error: null,
    });
  });

  test('sends nothing after it is cleared with one request in flight and one queued', async () => {
    const { asks, want, latest } = driven();
    want(a);
    want(b);
    want(null);
    asks[0]?.answer(view(4));
    await settle();
    expect(asks.map((ask) => ask.request)).toEqual([a]);
    expect(latest()).toEqual({ ...NO_PREVIEW, sent: 1 });
  });

  test('gives each answer the number its own request was sent with', async () => {
    const { asks, want, latest } = driven();
    // Request 1 is still out when the preview is turned off and on again, which sends request 2.
    want(a);
    want(null);
    want(b);
    expect(asks.map((ask) => ask.request)).toEqual([a, b]);
    asks[0]?.answer(view(4));
    await settle();
    // The answer to request 1 is not taken for request 2's.
    expect(latest()).toEqual({ sent: 2, flight: { id: 2, request: b }, queued: null, shown: null, error: null });
    asks[1]?.answer(view(9));
    await settle();
    expect(latest()?.shown).toEqual({ request: b, view: view(9) });
  });

  test('turns a failed request into its message, and goes on with the queued one', async () => {
    const { asks, want, latest } = driven();
    want(a);
    asks[0]?.fail('That screenshot is gone.');
    await settle();
    expect(latest()?.error).toBe('That screenshot is gone.');
    want(b);
    want(c);
    asks[1]?.fail('gone');
    await settle();
    expect(asks.map((ask) => ask.request)).toEqual([a, b, c]);
    expect(latest()?.error).toBeNull();
  });
});

describe('what shows while requests are out', () => {
  const answer = { request: request(10), view: view(4) };
  const idle: Preview = { sent: 1, flight: null, queued: null, shown: answer, error: null };
  const flying: Preview = { ...idle, sent: 2, flight: { id: 2, request: request(11) } };

  test('keeps the tint while another tolerance is asked about on the same rectangle', () => {
    expect(tinted(flying, request(11))).toBe(answer);
  });

  test('shows nothing once the preview is off, or for another screenshot, rectangle, or kind of rule', () => {
    expect(tinted(idle, null)).toBeNull();
    expect(tinted(NO_PREVIEW, request(10))).toBeNull();
    expect(tinted(idle, request(10, 1))).toBeNull();
    expect(tinted(idle, request(10, 0, REGION))).toBeNull();
    expect(tinted(idle, { ...request(10), rule: { kind: 'color', color: WHITE, tolerance: 10 } })).toBeNull();
  });

  test('says an answer is awaited until what shows is for the settings now', () => {
    expect(awaited(idle, request(10))).toBe(false);
    expect(awaited(flying, request(11))).toBe(true);
    // The settings changed and their request has not gone out yet.
    expect(awaited(idle, request(11))).toBe(true);
    expect(awaited(NO_PREVIEW, request(10))).toBe(true);
    expect(awaited(NO_PREVIEW, null)).toBe(false);
    expect(awaited({ ...idle, shown: null, error: 'gone' }, request(10))).toBe(false);
    expect(awaited({ ...flying, shown: null, error: null }, request(11))).toBe(true);
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
  const box = (width: number, height: number): Rect => ({ x: 5, y: 7, width, height });

  test('tints the pixels of each run and leaves the rest clear', () => {
    // A 4x2 rectangle with a run that goes on from the first row into the second.
    const runs = layer([
      [1, 2],
      [3, 5],
    ]);
    const pixels = paint({ area: box(4, 2), layers: [runs], tints: [magenta] });
    expect(pixels.length).toBe(32);
    const tintedAt = [0, 1, 2, 3, 4, 5, 6, 7].filter((index) => (pixel(pixels, index)[3] ?? 0) > 0);
    expect(tintedAt).toEqual([1, 3, 4]);
    expect(pixel(pixels, 1).slice(0, 3)).toEqual([255, 0, 255]);
    const alpha = pixel(pixels, 1)[3] ?? 0;
    expect(alpha).toBeGreaterThan(100);
    expect(alpha).toBeLessThan(200);
  });

  test('tints a striped layer on two of every four diagonals, in its own color', () => {
    const pixels = paint({
      area: box(8, 2),
      layers: [layer([[0, 2]]), layer([[2, 16]])],
      tints: [magenta, stripes],
    });
    expect(pixel(pixels, 0).slice(0, 3)).toEqual([255, 0, 255]);
    const striped = [...Array.from({ length: 16 }).keys()].filter((index) => pixel(pixels, index)[1] === 255);
    // Column plus row, modulo 4, is 0 or 1.
    expect(striped).toEqual([4, 5, 8, 11, 12, 15]);
    expect(pixel(pixels, 2)).toEqual([0, 0, 0, 0]);
  });

  test('shows no matches as a clear rectangle and every match as a full one', () => {
    expect(paint({ area: box(3, 2), layers: [layer([])], tints: [magenta] }).every((byte) => byte === 0)).toBe(true);
    const all = paint({ area: box(3, 2), layers: [layer([[0, 6]])], tints: [magenta] });
    expect([0, 1, 2, 3, 4, 5].every((index) => (pixel(all, index)[3] ?? 0) > 0)).toBe(true);
  });

  test('leaves a layer with too many runs to draw clear', () => {
    const pixels = paint({ area: box(3, 2), layers: [layer(null, 5)], tints: [magenta] });
    expect(pixels.every((byte) => byte === 0)).toBe(true);
  });

  test('tints an answer over the rectangle it was asked about, each layer in its tint', () => {
    const bar: PreviewRequest = { screenshot: 0, area: REGION, rule: { kind: 'barFill', filled: GREEN, empty: WHITE } };
    const [filled, empty] = [layer([[0, 5]]), layer([[5, 7]])];
    expect(paintedOf({ request: bar, view: { kind: 'barFill', filled, empty } })).toEqual({
      area: REGION,
      layers: [filled, empty],
      tints: tintsFor(bar.rule),
    });
  });
});

describe('selectedPixels', () => {
  const steps: readonly SelectStep[] = [{ kind: 'click', x: 3, y: 2, combine: 'replace' }];
  const states = [{ screenshot: 4, steps }];
  const found: SelectionView = {
    region: { x: 2, y: 1, width: 4, height: 2 },
    spans: [
      [0, 3],
      [5, 7],
    ],
  };
  const made = (view: SelectionView | null, colorMatch = 24): ReadonlyMap<number, Selection> =>
    new Map([[4, { steps, colorMatch, view }]]);

  test("tints the selection's own pixels where its box is, and counts them", () => {
    expect(selectedPixels(states, made(found), 4, 24, false)).toEqual({
      line: 'The selection takes 5 of the 8 pixels in the box around it (63%).',
      painted: { area: found.region, layers: [found], tints: [SELECTED_TINT] },
      updating: false,
    });
  });

  test('keeps the last selection while the one for a new color match is awaited, and says so', () => {
    const waiting = selectedPixels(states, made(found, 20), 4, 24, false);
    expect(waiting.painted?.layers).toEqual([found]);
    expect(waiting.updating).toBe(true);
  });

  test('says why nothing is tinted', () => {
    const none = (line: string, updating = false) => ({ line, painted: null, updating });
    expect(selectedPixels(states, made(found), null, 24, false)).toEqual(
      none('Selected pixels show on a screenshot. Select the one the thing was selected on.'),
    );
    const unselected = none('Nothing is selected on this screenshot yet. Select the thing on it with Snap or Click.');
    expect(selectedPixels(states, made(found), 5, 24, false)).toEqual(unselected);
    expect(selectedPixels([{ screenshot: 4, steps: [] }], made(found), 4, 24, false)).toEqual(unselected);
    // A selection made for other steps is not this state's.
    expect(selectedPixels([{ screenshot: 4, steps: [...steps] }], made(found), 4, 24, false)).toEqual(
      none('Selecting…', true),
    );
    expect(selectedPixels(states, new Map(), 4, 24, false)).toEqual(none('Selecting…', true));
    expect(selectedPixels(states, made(null), 4, 24, false)).toEqual(
      none('Nothing is selected at this color match. Change it, or select the thing again.'),
    );
  });

  test('tints nothing and awaits nothing once the selection for this color match failed', () => {
    const failed = { line: 'The selection could not be made, so none is tinted.', painted: null, updating: false };
    // An earlier color match's selection is not shown as if it were updating.
    expect(selectedPixels(states, made(found, 20), 4, 24, true)).toEqual(failed);
    expect(selectedPixels(states, new Map(), 4, 24, true)).toEqual(failed);
    // A selection made for this color match stays, whatever failed since.
    expect(selectedPixels(states, made(found), 4, 24, true).painted?.layers).toEqual([found]);
    expect(selectedPixels(states, made(found), 4, 24, true).updating).toBe(false);
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
