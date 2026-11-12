import { describe, expect, test } from 'bun:test';

import type { RecordedReading } from '../src/lib/bindings.ts';
import {
  addedShapes,
  additions,
  checkedReadings,
  clock,
  editAll,
  entries,
  flaggedReadings,
  keepBlocked,
  keptAdditions,
  keptLabels,
  named,
  reviewOrder,
  seen,
  sighted,
  startEntry,
  tileNote,
} from '../src/lib/review.ts';

/** A shape for each character, the same for the same character, as when the reader read every reading right. */
const shapesOf = (text: string): number[] => Array.from(text.trim(), (character) => character.codePointAt(0) ?? 0);
/** A reading the reader read as `text`, of `shapes`. */
const read = (text: string, shapes: readonly number[] = shapesOf(text)): RecordedReading => ({
  firstMs: 0,
  seen: 1,
  text: { kind: 'read', text },
  shapes,
});
const pending: RecordedReading = { firstMs: 0, seen: 1, text: { kind: 'pending' }, shapes: [1] };
const failed: RecordedReading = { firstMs: 0, seen: 1, text: { kind: 'failed', text: 'No model.' }, shapes: [2] };

describe('clock and seen', () => {
  test('show minutes and seconds, and how often', () => {
    expect([clock(0), clock(4_999), clock(65_000), clock(-5)]).toEqual(['0:00', '0:04', '1:05', '0:00']);
    expect([seen(1), seen(3)]).toEqual(['seen once', 'seen 3 times']);
  });

  test('count how often a shape showed, in how many readings', () => {
    expect(sighted({ frames: 1, readings: 1 })).toBe('seen once');
    expect(sighted({ frames: 5, readings: 1 })).toBe('seen 5 times in 1 reading');
    expect(sighted({ frames: 214, readings: 37 })).toBe('seen 214 times in 37 readings');
  });
});

describe('what a reading adds', () => {
  const all = (readings: readonly RecordedReading[]): boolean[] => readings.map(() => true);

  test('a text names the shapes only with one character for each', () => {
    expect(named(read('', [5, 6]), '8 7')).toEqual(['8', '7']);
    expect(named(read('', [5, 6]), '87%')).toBeNull();
    expect(named(read('', [5, 6]), '8')).toBeNull();
    expect(named(read('', []), ' ')).toBeNull();
    // One character of two UTF-16 units is one shape.
    expect(named(read('', [5]), '😀')).toEqual(['😀']);
  });

  test('each shape counts for the first reading that teaches it', () => {
    const readings = [read('0%'), read('1%'), read('2.5%')];
    expect(additions(readings, all(readings))).toEqual([[0, 1], [0], [0, 1, 2]]);
    // A shape a reading shows twice is added once, where it first stands.
    expect(additions([read('110')], [true])).toEqual([[0, 2]]);
  });

  test('a reading whose shapes the others teach adds nothing', () => {
    // 12 came first, but 13 and 24 show its shapes and one more each.
    const readings = [read('12'), read('13'), read('24')];
    expect(additions(readings, all(readings))).toEqual([[], [0, 1], [0, 1]]);
    // Of three that each make up for another, only the first is dropped, so every shape is still taught.
    const ring = [read('12'), read('13'), read('23')];
    expect(additions(ring, all(ring))).toEqual([[], [0, 1], [0]]);
  });

  test('a reading that teaches nothing adds what no teaching reading shows', () => {
    const readings = [read('12'), read('13'), read('24')];
    // Without 13, only 12 teaches the 1, so 12 is needed, and 13 would add the 3.
    expect(additions(readings, [true, false, true])).toEqual([[0, 1], [1], [1]]);
    // A reading no text names adds its shapes until others teach them all.
    expect(additions([read('?', [1, 2]), read('1', [1])], [false, true])).toEqual([[1], [0]]);
    expect(additions([read('?', [1, 2]), read('12', [1, 2])], [false, true])).toEqual([[], [0, 1]]);
  });

  test('review shows the readings that add shapes first, each group in the order it showed', () => {
    expect(reviewOrder([read('12'), read('13'), read('24'), read('5')])).toEqual([1, 2, 3, 0]);
    expect(reviewOrder([])).toEqual([]);
  });

  test('a kept reading adds only with a text that names its shapes', () => {
    const readings = [read('12'), read('13')];
    const kept = (first: string, keep: boolean) => [
      { keep, text: first },
      { keep: true, text: '13' },
    ];
    expect(keptAdditions(readings, kept('12', true))).toEqual([[0, 1], [1]]);
    expect(keptAdditions(readings, kept('1', true))).toEqual([[1], [0, 1]]);
    expect(keptAdditions(readings, kept('12', false))).toEqual([[1], [0, 1]]);
    expect(keptAdditions(readings, [])).toEqual([
      [0, 1],
      [0, 1],
    ]);
  });

  test('an added shape shows its character by the text, and how often it showed', () => {
    const shapes = [
      { frames: 9, readings: 4 },
      { frames: 2, readings: 1 },
    ];
    const reading = read('7%', [1, 0]);
    expect(addedShapes(reading, '7%', [0, 1], shapes)).toEqual([
      { character: '7', shape: { frames: 2, readings: 1 } },
      { character: '%', shape: { frames: 9, readings: 4 } },
    ]);
    expect(addedShapes(reading, '7', [1], shapes)).toEqual([{ character: null, shape: { frames: 9, readings: 4 } }]);
    expect(addedShapes(reading, '7%', [5], shapes)).toEqual([]);
  });
});

describe('review entries', () => {
  test('start from what was kept, else from what the reader read', () => {
    expect(startEntry(read('87%'), '88%')).toEqual({ keep: true, text: '88%' });
    expect(startEntry(read(' 87% '), undefined)).toEqual({ keep: true, text: '87%' });
    expect(startEntry(read(''), undefined)).toEqual({ keep: false, text: '' });
    expect(startEntry(pending, undefined)).toEqual({ keep: false, text: '' });
    expect(startEntry(failed, undefined)).toEqual({ keep: false, text: '' });
  });

  test('start left out when the other readings teach every shape', () => {
    const readings = [read('12'), read('13'), read('24')];
    expect(entries(readings, new Map(), new Map()).map((entry) => entry.keep)).toEqual([false, true, true]);
    // The text stays, so keeping it takes one click.
    expect(entries(readings, new Map(), new Map())[0]).toEqual({ keep: false, text: '12' });
    // One kept before stays kept.
    expect(entries(readings, new Map(), new Map([[0, '12']])).map((entry) => entry.keep)).toEqual([true, true, true]);
    // The reader found three characters where 13 shows two shapes, so 13 teaches nothing and 12 is needed.
    const misread = [read('12'), read('1 3 3', shapesOf('13')), read('24')];
    expect(entries(misread, new Map(), new Map()).map((entry) => entry.keep)).toEqual([true, true, true]);
  });

  test('count as checked once kept before or edited, while kept', () => {
    const all = [
      { keep: true, text: '1' },
      { keep: true, text: '2' },
      { keep: false, text: '3' },
      { keep: true, text: '4' },
    ];
    const edits = new Map([
      [1, { keep: true, text: '2' }],
      [2, { keep: false, text: '3' }],
    ]);
    expect([...checkedReadings(all, edits, new Map([[3, '4']]))]).toEqual([1, 3]);
    expect(checkedReadings(all, new Map(), new Map()).size).toBe(0);
  });

  test("take the user's edits over the start", () => {
    const all = entries([read('1'), read('2')], new Map([[1, { keep: false, text: '2' }]]), new Map());
    expect(all).toEqual([
      { keep: true, text: '1' },
      { keep: false, text: '2' },
    ]);
  });

  test('keep only kept readings with text, trimmed', () => {
    const all = [
      { keep: true, text: ' 1 ' },
      { keep: false, text: '2' },
      { keep: true, text: ' ' },
      { keep: true, text: '4' },
    ];
    expect(keptLabels(all)).toEqual([
      { reading: 0, text: '1' },
      { reading: 3, text: '4' },
    ]);
  });
});

describe('tile notes', () => {
  test('say why a kept reading cannot be learned first', () => {
    expect(tileNote(read('9l%'), { keep: true, text: '9l%' }, 'Its l has another shape.')).toBe(
      'Its l has another shape.',
    );
    expect(tileNote(read('9l%'), { keep: false, text: '9l%' }, 'Its l has another shape.')).toBeNull();
  });

  test('ask for text the reader did not find', () => {
    expect(tileNote(read(''), { keep: true, text: '' }, undefined)).toBe('Type what it shows, or leave it out.');
    expect(tileNote(read(''), { keep: false, text: '' }, undefined)).toBe('No text found. Type it, or leave it out.');
    expect(tileNote(failed, { keep: false, text: '' }, undefined)).toBe('No model.');
    expect(tileNote(failed, { keep: false, text: '5' }, undefined)).toBeNull();
    expect(tileNote(pending, { keep: false, text: '' }, undefined)).toBe('Reading…');
    expect(tileNote(read('7'), { keep: true, text: '7' }, undefined)).toBeNull();
  });
});

describe('reviewing the kept readings', () => {
  const kept = [
    { reading: 1, text: '1' },
    { reading: 4, text: '4' },
  ];

  test('flags each reading by its index, from answers in the order of the kept ones', () => {
    expect([...flaggedReadings(kept, [null, 'Its 4 has another shape.'])]).toEqual([[4, 'Its 4 has another shape.']]);
    expect(flaggedReadings(kept, [null, null, 'Extra.']).size).toBe(0);
  });

  test('Keep waits for the check, and for every flagged reading to be fixed or left out', () => {
    expect(keepBlocked(kept, new Map(), false)).toBe('Checking the readings…');
    expect(keepBlocked(kept, new Map([[4, 'No.']]), true)).toBe('Fix or leave out the flagged readings to keep them.');
    expect(keepBlocked(kept, new Map([[2, 'No.']]), true)).toBeNull();
  });

  test('Keep all and Keep none edit every reading but one still being read with no edit yet', () => {
    const readings = [read('1'), pending, read(''), pending];
    const edits = new Map([[3, { keep: false, text: '9' }]]);
    const all = entries(readings, edits, new Map());
    expect([...editAll(edits, all, readings, true)]).toEqual([
      [3, { keep: true, text: '9' }],
      [0, { keep: true, text: '1' }],
      [2, { keep: false, text: '' }],
    ]);
    expect([...editAll(edits, all, readings, false)].map(([index, entry]) => [index, entry.keep])).toEqual([
      [3, false],
      [0, false],
      [2, false],
    ]);
  });
});
