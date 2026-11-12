import { describe, expect, test } from 'bun:test';

import type { BarDetection } from '../src/lib/bindings.ts';
import {
  type DetectKey,
  type Detecting,
  detectingText,
  detectStep,
  MARK_TONES,
  measuredFor,
  measureKey,
  paintMarks,
  readingLine,
  toneCss,
} from '../src/lib/detect.ts';

const green = { red: 40, green: 210, blue: 60 };
const gray = { red: 55, green: 55, blue: 70 };
const red = { red: 255, green: 0, blue: 0 };
const key: DetectKey = { region: { x: 1, y: 2, width: 30, height: 4 }, drawnOn: 0, shots: 2 };
type Proposal = Extract<BarDetection, { readonly kind: 'proposed' }>;
const proposed: Proposal = { kind: 'proposed', filled: green, empty: gray, basis: 'changes', screenshots: [0, 1] };
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
    const more = { ...key, shots: 3 };
    for (const now of [moved, redrawn, more]) {
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
    'Finds the filled and the empty color on the screenshots. Pick them by hand below to correct them, or when it finds none.';
  const side =
    'The left color, or the bottom one of a tall region, counts as filled: swap the colors if the bar fills from the other end.';
  const caveat =
    'One screenshot cannot tell a bar from a picture in two colors: compare the readings below with what you see, or take a screenshot at another fill and detect again.';
  const held = { filled: green, empty: gray };
  const found = (proposal: Proposal, drawnOn = 0): Detecting => ({
    kind: 'found',
    key: { ...key, drawnOn },
    proposal,
    asking: false,
  });

  test('says what detecting does, that it looks, and why it found none', () => {
    expect(detectingText(idle, nonePicked, position)).toBe(note);
    expect(detectingText(detecting, nonePicked, position)).toBe('Looking for the bar…');
    expect(detectingText({ kind: 'declined', key, message: 'No bar.' }, nonePicked, position)).toBe('No bar.');
  });

  test('names the screenshots the changes were found between', () => {
    expect(detectingText(found(proposed), held, position)).toBe(
      `Found from the pixels that change between screenshots 1 and 2. ${side}`,
    );
    expect(detectingText(found({ ...proposed, screenshots: [0, 2, 3] }), held, position)).toBe(
      `Found from the pixels that change between screenshots 1, 3, and 4. ${side}`,
    );
  });

  test('says when the colors leave out part of what changes', () => {
    expect(detectingText(found({ ...proposed, basis: 'roughChanges' }), held, position)).toBe(
      `Found from the pixels that change between screenshots 1 and 2. The two colors leave out some of those pixels, as on a bar filled with a gradient, so readings can be a few points off. ${side}`,
    );
  });

  test('says what one screenshot cannot show, and when the others did not help', () => {
    expect(detectingText(found({ ...proposed, basis: 'layout', screenshots: [1] }, 1), held, position)).toBe(
      `Found on screenshot 2 alone. ${caveat} ${side}`,
    );
    expect(detectingText(found({ ...proposed, basis: 'layout' }, 1), held, position)).toBe(
      `Found on screenshot 2 alone: the region does not change between two colors on screenshots 1 and 2. ${caveat} ${side}`,
    );
  });

  test('shows how the colors were found only while the draft holds them, or is asked about them', () => {
    const shown = `Found from the pixels that change between screenshots 1 and 2. ${side}`;
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
  const made = { key: measureKey(key, colors, 1), measure: { readings: [], marks: null }, error: null };
  expect(measuredFor(made, measureKey({ ...key }, { ...colors }, 1))).toBe(made);
  expect(measuredFor(made, measureKey(key, { filled: gray, empty: green }, 1))).toBeNull();
  expect(measuredFor(made, measureKey(key, colors, null))).toBeNull();
  expect(measuredFor(made, measureKey({ ...key, shots: 3 }, colors, 1))).toBeNull();
  expect(measuredFor(made, measureKey({ ...key, drawnOn: 1 }, colors, 1))).toBeNull();
  expect(measuredFor(made, measureKey({ ...key, region: { ...key.region, width: 31 } }, colors, 1))).toBeNull();
  expect(measuredFor(made, null)).toBeNull();
  expect(measuredFor(null, made.key)).toBeNull();
});

test('marks paint a tone over each counted pixel and leave the rest clear', () => {
  const painted = paintMarks({ filled: [[0, 2]], empty: [[3, 4]] }, 5);
  expect([...painted]).toEqual([
    ...MARK_TONES.filled,
    ...MARK_TONES.filled,
    ...[0, 0, 0, 0],
    ...MARK_TONES.empty,
    ...[0, 0, 0, 0],
  ]);
  // A run past the region, as after the region shrank, stops at its end.
  expect([...paintMarks({ filled: [[1, 9]], empty: [] }, 2)]).toEqual([0, 0, 0, 0, ...MARK_TONES.filled]);
  expect(toneCss(MARK_TONES.empty)).toBe('rgb(74 163 255)');
});
