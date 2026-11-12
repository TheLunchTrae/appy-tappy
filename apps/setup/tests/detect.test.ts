import { describe, expect, test } from 'bun:test';

import type { BarDetection } from '../src/lib/bindings.ts';
import {
  type DetectKey,
  type Detecting,
  detectingText,
  detectStep,
  leftOutText,
  measuredFor,
  measureKey,
  readingLine,
} from '../src/lib/detect.ts';

const green = { red: 40, green: 210, blue: 60 };
const gray = { red: 55, green: 55, blue: 70 };
const red = { red: 255, green: 0, blue: 0 };
const key: DetectKey = { region: { x: 1, y: 2, width: 30, height: 4 }, drawnOn: 0, shots: '0,1' };
type Proposal = Extract<BarDetection, { readonly kind: 'proposed' }>;
const proposed: Proposal = {
  kind: 'proposed',
  filled: green,
  empty: gray,
  foundOn: 0,
  shownOn: [0, 1],
  without: [],
  moves: true,
  rough: false,
  around: false,
  leftOut: 0,
};
const nonePicked = { filled: null, empty: null };
const detecting: Detecting = { kind: 'detecting', key };
const idle: Detecting = { kind: 'idle' };

describe('detectStep', () => {
  test('asking starts a detection, whatever stood before', () => {
    expect(detectStep(idle, { kind: 'ask', key })).toEqual({ state: detecting, use: null });
    const declined: Detecting = { kind: 'declined', key, message: 'No bar.' };
    expect(detectStep(declined, { kind: 'ask', key }).state).toEqual(detecting);
  });

  test('colors found go straight into a draft with none', () => {
    expect(detectStep(detecting, { kind: 'answer', key, detection: proposed, picked: nonePicked })).toEqual({
      state: { kind: 'found', key, proposal: proposed, asking: false },
      use: { filled: green, empty: gray },
    });
  });

  test('a draft with a color picked by hand is asked about before it changes', () => {
    for (const picked of [
      { filled: red, empty: null },
      { filled: null, empty: red },
      { filled: green, empty: red },
    ]) {
      expect(detectStep(detecting, { kind: 'answer', key, detection: proposed, picked })).toEqual({
        state: { kind: 'found', key, proposal: proposed, asking: true },
        use: null,
      });
    }
    const asking: Detecting = { kind: 'found', key, proposal: proposed, asking: true };
    expect(detectStep(asking, { kind: 'use' })).toEqual({
      state: { kind: 'found', key, proposal: proposed, asking: false },
      use: { filled: green, empty: gray },
    });
    expect(detectStep(asking, { kind: 'keep' })).toEqual({ state: idle, use: null });
  });

  test('a draft that holds the colors found already is not asked about', () => {
    for (const picked of [
      { filled: green, empty: gray },
      { filled: gray, empty: green },
    ]) {
      expect(detectStep(detecting, { kind: 'answer', key, detection: proposed, picked })).toEqual({
        state: { kind: 'found', key, proposal: proposed, asking: false },
        use: null,
      });
    }
  });

  test('no bar, or a failed request, says what to do', () => {
    const declined: BarDetection = { kind: 'declined', message: 'Take a screenshot where the bar is partly filled.' };
    expect(detectStep(detecting, { kind: 'answer', key, detection: declined, picked: nonePicked })).toEqual({
      state: { kind: 'declined', key, message: declined.message },
      use: null,
    });
    expect(detectStep(detecting, { kind: 'fail', key, message: 'That screenshot is gone.' })).toEqual({
      state: { kind: 'declined', key, message: 'That screenshot is gone.' },
      use: null,
    });
  });

  test('an answer for a region or screenshots that changed since is dropped', () => {
    const moved = { ...key, region: { ...key.region, x: 5 } };
    const redrawn = { ...key, drawnOn: 1 };
    const more = { ...key, shots: '0,1,2' };
    // One removed and one taken: as many as before, but not the same ones.
    const others = { ...key, shots: '0,2' };
    for (const now of [moved, redrawn, more, others]) {
      // The region or the screenshots changed, and a new detection may be under way for them.
      const reset = detectStep(detecting, { kind: 'inputs', key: now });
      expect(reset).toEqual({ state: idle, use: null });
      for (const state of [reset.state, detectStep(reset.state, { kind: 'ask', key: now }).state]) {
        expect(detectStep(state, { kind: 'answer', key, detection: proposed, picked: nonePicked })).toEqual({
          state,
          use: null,
        });
        expect(detectStep(state, { kind: 'fail', key, message: 'Gone.' })).toEqual({ state, use: null });
      }
    }
    expect(detectStep(detecting, { kind: 'inputs', key: null }).state).toEqual(idle);
  });

  test('the same inputs keep what was found, and stray answers change nothing', () => {
    const found: Detecting = { kind: 'found', key, proposal: proposed, asking: false };
    expect(detectStep(found, { kind: 'inputs', key: { ...key, region: { ...key.region } } }).state).toBe(found);
    expect(detectStep(found, { kind: 'use' })).toEqual({ state: found, use: null });
    expect(detectStep(found, { kind: 'keep' })).toEqual({ state: found, use: null });
    expect(detectStep(idle, { kind: 'use' })).toEqual({ state: idle, use: null });
    expect(detectStep(idle, { kind: 'inputs', key })).toEqual({ state: idle, use: null });
    expect(detectStep(found, { kind: 'answer', key, detection: proposed, picked: nonePicked }).state).toBe(found);
  });
});

describe('detectingText', () => {
  const position = (index: number): number => index + 1;
  const note =
    'Finds a bar of two plain colors on the screenshots: one color from the left, or from the bottom of a tall region, and the other after it. Pick the colors by hand to correct them, or when it finds none.';
  const side =
    'The left color, or the bottom one of a tall region, counts as filled: swap the colors if the bar fills from the other end.';
  const caveat =
    'One fill cannot tell a bar from a picture in two colors: compare the readings below with what you see, or take a screenshot at another fill and detect again.';
  const held = { filled: green, empty: gray };
  const found = (proposal: Proposal): Detecting => ({ kind: 'found', key, proposal, asking: false });
  const still: Proposal = { ...proposed, moves: false };

  test('says what detecting looks for, that it looks, and why it found none', () => {
    expect(detectingText(idle, nonePicked, position)).toBe(note);
    expect(detectingText(detecting, nonePicked, position)).toBe('Looking for the bar…');
    expect(detectingText({ kind: 'declined', key, message: 'No bar.' }, nonePicked, position)).toBe('No bar.');
  });

  test('names the screenshots that show the bar at different fills', () => {
    expect(detectingText(found(proposed), held, position)).toBe(
      `Screenshots 1 and 2 show the bar in these two colors, at different fills. ${side}`,
    );
    expect(detectingText(found({ ...proposed, shownOn: [0, 2, 3] }), held, position)).toBe(
      `Screenshots 1, 3, and 4 show the bar in these two colors, at different fills. ${side}`,
    );
  });

  test('says what one fill cannot show, whether or not other screenshots were looked at', () => {
    expect(detectingText(found({ ...still, foundOn: 1, shownOn: [1] }), held, position)).toBe(
      `Found on screenshot 2 alone. ${caveat} ${side}`,
    );
    // The others show the same fill, a full or an empty bar, or neither color: none shows another fill of both.
    const others = `Found on screenshot 2. No other screenshot shows both colors at another fill. ${caveat}`;
    expect(detectingText(found({ ...still, foundOn: 1 }), held, position)).toBe(`${others} ${side}`);
    expect(detectingText(found({ ...still, foundOn: 1, shownOn: [1], without: [0] }), held, position)).toBe(
      `${others} Screenshot 1 shows neither color in the region. ${side}`,
    );
  });

  test('names the screenshots that show neither color, readings that are off, and screenshots left out', () => {
    const start = 'Screenshots 1 and 2 show the bar in these two colors, at different fills.';
    expect(detectingText(found({ ...proposed, without: [2, 4] }), held, position)).toBe(
      `${start} Screenshots 3 and 5 show neither color in the region. ${side}`,
    );
    expect(detectingText(found({ ...proposed, rough: true }), held, position)).toBe(
      `${start} On at least one screenshot the colors read a few points off where the bar ends, as a label or a soft edge on the bar causes. ${side}`,
    );
    expect(detectingText(found({ ...proposed, around: true, rough: true }), held, position)).toBe(
      `${start} Only some strips of the region show the bar from end to end, so AppyTappy went by those. What lies in the rest, such as a label, an icon, or the background, counts only where it has one of the two colors. On at least one screenshot the colors read a few points off where the bar ends, as a label or a soft edge on the bar causes. ${side}`,
    );
    expect(detectingText(found({ ...proposed, leftOut: 1 }), held, position)).toBe(
      `${start} One older screenshot of this size was not looked at. ${side}`,
    );
    expect(detectingText(found({ ...proposed, leftOut: 3 }), held, position)).toBe(
      `${start} 3 older screenshots of this size were not looked at. ${side}`,
    );
  });

  test('shows how the colors were found only while the draft holds them, or is asked about them', () => {
    const shown = `Screenshots 1 and 2 show the bar in these two colors, at different fills. ${side}`;
    // Swapped colors are still the ones found.
    expect(detectingText(found(proposed), { filled: gray, empty: green }, position)).toBe(shown);
    for (const picked of [
      { filled: green, empty: red },
      { filled: green, empty: green },
      { filled: green, empty: null },
      { filled: { ...green, blue: 61 }, empty: gray },
      nonePicked,
    ]) {
      expect(detectingText(found(proposed), picked, position)).toBe(note);
    }
    const asking: Detecting = { kind: 'found', key, proposal: proposed, asking: true };
    expect(detectingText(asking, { filled: red, empty: null }, position)).toBe(shown);
  });
});

test('a reading shows its screenshot and its fill to a tenth', () => {
  const position = (index: number): number => index + 1;
  expect(readingLine({ screenshot: 0, fill: 72 }, position)).toBe('Screenshot 1: 72');
  expect(readingLine({ screenshot: 2, fill: 33.333_333 }, position)).toBe('Screenshot 3: 33.3');
  expect(readingLine({ screenshot: 1, fill: 0 }, position)).toBe('Screenshot 2: 0');
  expect(readingLine({ screenshot: 1, fill: null }, position)).toBe('Screenshot 2: neither color shows');
});

test('a measure shows only for the colors, region, and screenshots it was made for', () => {
  const colors = { filled: green, empty: gray };
  const made = { key: measureKey(key, colors), measure: { readings: [], leftOut: 0 }, error: null };
  expect(measuredFor(made, measureKey({ ...key }, { ...colors }))).toBe(made);
  expect(measuredFor(made, measureKey(key, { filled: gray, empty: green }))).toBeNull();
  expect(measuredFor(made, measureKey({ ...key, shots: '0,2' }, colors))).toBeNull();
  expect(measuredFor(made, measureKey({ ...key, drawnOn: 1 }, colors))).toBeNull();
  expect(measuredFor(made, measureKey({ ...key, region: { ...key.region, width: 31 } }, colors))).toBeNull();
  expect(measuredFor(made, null)).toBeNull();
  expect(measuredFor(null, made.key)).toBeNull();
});

test('older screenshots that were not looked at are counted in words', () => {
  expect(leftOutText(0)).toBeNull();
  expect(leftOutText(1)).toBe('One older screenshot of this size was not looked at.');
  expect(leftOutText(2)).toBe('2 older screenshots of this size were not looked at.');
});
