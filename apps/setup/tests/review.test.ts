import { describe, expect, test } from 'bun:test';

import type { RecordedReading } from '../src/lib/bindings.ts';
import * as review from '../src/lib/review.ts';
import {
  NO_CROPS,
  addedShapes,
  additions,
  checkedReadings,
  clock,
  disagreements,
  editAll,
  flaggedReadings,
  keepBlocked,
  keptAdditions,
  keptLabels,
  keptNote,
  named,
  reviewOrder,
  seen,
  sighted,
  startEntry,
  unrelated,
  untaughtNote,
  withCrop,
  cropAskFailed,
  cropsToAsk,
  NO_CROP_ASKS,
} from '../src/lib/review.ts';

/** Where a reading's characters are in its picture, which review does not look at. */
const cut = { width: 0, characters: [] };
// Most tests are of a reference that does not say which characters it needs, with nothing decided and no clash.
const NO_NEEDS: readonly string[] = [];
type Entries = ReadonlyMap<number, review.ReviewEntry>;
const decide = (
  decided: Entries,
  readings: readonly RecordedReading[],
  edits: Entries,
  kept: ReadonlyMap<number, string>,
  needs = NO_NEEDS,
) => review.decide(decided, readings, edits, kept, needs);
const entries = (
  readings: readonly RecordedReading[],
  edits: Entries,
  kept: ReadonlyMap<number, string>,
  decided: Entries = new Map(),
  needs = NO_NEEDS,
) => review.entries(readings, edits, kept, decided, needs);
const untaught = (
  readings: readonly RecordedReading[],
  all: readonly review.ReviewEntry[],
  adds: readonly (readonly number[])[],
  needs = NO_NEEDS,
) => review.untaught(readings, all, adds, needs);
const tileNote = (
  reading: RecordedReading,
  entry: review.ReviewEntry,
  problem: string | undefined,
  clash?: string,
  needs = NO_NEEDS,
) => review.tileNote(reading, entry, problem, clash, needs);

/** A shape for each character, the same for the same character, as when the reader read every reading right. */
const shapesOf = (text: string): number[] => Array.from(text.trim(), (character) => character.codePointAt(0) ?? 0);
/** A reading the reader read as `text`, of `shapes`, that first showed `firstMs` in. */
const read = (text: string, shapes: readonly number[] = shapesOf(text), firstMs = 0): RecordedReading => ({
  firstMs,
  seen: 1,
  size: null,
  text: { kind: 'read', text },
  shapes,
  ...cut,
});
/** A reading of `shapes` still being read. */
const unread = (shapes: readonly number[]): RecordedReading => ({
  firstMs: 0,
  seen: 1,
  size: null,
  text: { kind: 'pending' },
  shapes,
  ...cut,
});
const keeps = (all: readonly { readonly keep: boolean }[]): boolean[] => all.map((entry) => entry.keep);
const pending: RecordedReading = { firstMs: 0, seen: 1, size: null, text: { kind: 'pending' }, shapes: [1], ...cut };
const failed: RecordedReading = {
  firstMs: 0,
  seen: 1,
  size: null,
  text: { kind: 'failed', text: 'No model.' },
  shapes: [2],
  ...cut,
};

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

describe('text sizes', () => {
  test('a recording says which sizes of text it looks for, and one place says nothing', () => {
    expect(review.sizesNote([])).toBeNull();
    expect(review.sizesNote([8])).toBe('text 8 px tall');
    expect(review.sizesNote([8, 16])).toBe('text of 2 sizes: 8 px and 16 px tall');
    expect(review.sizesNote([8, 16, 24])).toBe('text of 3 sizes: 8 px, 16 px and 24 px tall');
  });

  test('a reading says its size only when the recording looked for more than one', () => {
    const large = { ...read('5'), size: { height: 16, example: 16 } };
    expect(review.readingSize(large, [8, 16])).toBe('text 16 px tall');
    expect(review.readingSize(large, [16])).toBeNull();
    expect(review.readingSize(read('5'), [8, 16])).toBeNull();
  });

  test('a reading taken at another window size says how tall it is and which size it was scaled from', () => {
    const scaled = { ...read('5'), size: { height: 16, example: 8 } };
    expect(review.readingSize(scaled, [8, 16])).toBe('text 16 px tall, the 8 px size scaled');
  });
});

describe('a recording at another window size', () => {
  const drawn = { width: 1920, height: 1080 };
  const settings = {
    region: { x: 0, y: 0, width: 10, height: 10 },
    ink: { red: 255, green: 255, blue: 255 },
    tolerance: 10,
    windowSize: drawn,
  };

  test('says nothing at the size its place was drawn at', () => {
    expect(review.followingNote({ at: { ...drawn }, settings: { ...settings, scope: 'window' } })).toBeNull();
  });

  test('says what is scaled to the size the window has now', () => {
    const narrower = { width: 1280, height: 1080 };
    expect(review.followingNote({ at: narrower, settings: { ...settings, scope: 'place' } })).toBe(
      'The window is 1280 × 1080 now, so the place is scaled to it.',
    );
    const smaller = { width: 1920, height: 720 };
    expect(review.followingNote({ at: smaller, settings: { ...settings, scope: 'window' } })).toBe(
      'The window is 1920 × 720 now, so the place and the text sizes are scaled to it.',
    );
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

  test('a settled reading stays, and the ones after it are dropped instead', () => {
    const ring = [read('12'), read('13'), read('23')];
    expect(additions(ring, all(ring), [true, true, false])).toEqual([[0, 1], [1], []]);
    // Only 12 is settled, so 13 is the first the others make up for.
    expect(additions(ring, all(ring), [true, false, false])).toEqual([[0, 1], [], [1]]);
    expect(additions(ring, all(ring), [false, false, true])).toEqual([[], [0, 1], [0]]);
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

  test('never leave out a reading whose text and another give one shape two characters', () => {
    // The reader read the 1 of 13 as an l. Left to the shapes, 12 would start left out and hide it.
    const misread = [read('12'), read('l3', shapesOf('13'), 2_000), read('24')];
    const all = entries(misread, new Map(), new Map());
    expect(keeps(all)).toEqual([true, true, true]);
    expect(keptLabels(all).map((label) => label.text)).toEqual(['12', 'l3', '24']);
    expect([...disagreements(misread, all)]).toEqual([
      [0, 'Its 1 has the same shape as the l of the reading at 0:02. Fix the text that is wrong.'],
      [1, 'Its l has the same shape as the 1 of the reading at 0:00. Fix the text that is wrong.'],
    ]);
    // A 0 read as the letter O, as on a counter at 90%: both readings with that shape stay and are flagged.
    const letter = [read('87%'), read('9O%', shapesOf('90%')), read('80%'), read('89%')];
    const guessed = entries(letter, new Map(), new Map());
    expect(keeps(guessed)).toEqual([true, true, true, false]);
    expect([...disagreements(letter, guessed).keys()]).toEqual([1, 2]);
  });

  test('compare texts on a shape whether the readings are kept or left out, once the texts name their shapes', () => {
    const readings = [read('12'), read('l3', shapesOf('13'))];
    const out = [
      { keep: false, text: '12' },
      { keep: true, text: 'l3' },
    ];
    expect([...disagreements(readings, out).keys()]).toEqual([0, 1]);
    // A text with another count of characters names no shape, and equal texts agree.
    const short = [
      { keep: false, text: '12' },
      { keep: true, text: 'l' },
    ];
    expect(disagreements(readings, short).size).toBe(0);
    expect(disagreements(readings, [{ keep: true, text: '12' }]).size).toBe(0);
    expect(disagreements([read('12'), read('13')], entries([read('12'), read('13')], new Map(), new Map())).size).toBe(
      0,
    );
  });

  test('decide a start once, so Keep changes only by the user', () => {
    const ring = [read('12'), read('13'), read('23')];
    const decided = decide(new Map(), ring, new Map(), new Map());
    expect(keeps(entries(ring, new Map(), new Map(), decided))).toEqual([false, true, true]);
    // Deciding again changes nothing, and answers the same starts.
    expect(decide(decided, ring, new Map(), new Map())).toBe(decided);
    // The user leaves 23 out. 12 stays left out, though the 2 is now taught by no kept reading.
    const edits = new Map([[2, { keep: false, text: '23' }]]);
    expect(decide(decided, ring, edits, new Map())).toBe(decided);
    const all = entries(ring, edits, new Map(), decided);
    expect(keeps(all)).toEqual([false, true, false]);
    const left = untaught(ring, all, keptAdditions(ring, all));
    expect(left).toEqual({ characters: ['2'], readings: [0] });
    expect(untaughtNote(left.characters)).toBe('No kept reading teaches 2.');
    // Keeping the reading it offers teaches the 2.
    const fixed = entries(ring, new Map([...edits, [0, { keep: true, text: '12' }]]), new Map(), decided);
    expect(untaught(ring, fixed, keptAdditions(ring, fixed))).toEqual({ characters: [], readings: [] });
    expect(untaughtNote([])).toBeNull();
  });

  test('a text that arrives later decides its own reading and leaves the others as they were', () => {
    const early = [read('12'), unread(shapesOf('13')), unread(shapesOf('23'))];
    const first = decide(new Map(), early, new Map(), new Map());
    expect([...first.keys()]).toEqual([0]);
    expect(keeps(entries(early, new Map(), new Map(), first))).toEqual([true, false, false]);
    // With every text read at once, 12 would start left out. Decided already, it stays kept, and 13 is the first
    // of the two new ones that the others make up for.
    const late = [read('12'), read('13'), read('23')];
    const second = decide(first, late, new Map(), new Map());
    expect(keeps(entries(late, new Map(), new Map(), second))).toEqual([true, false, true]);
    // A reading the user edited stays as edited, and the ones decided beside it count it.
    const edits = new Map([[1, { keep: true, text: '13' }]]);
    const beside = decide(new Map(), [read('12'), unread(shapesOf('13')), read('23')], edits, new Map());
    expect(keeps(entries(late, edits, new Map(), beside))).toEqual([false, true, true]);
  });

  test('name what only readings left out could teach, and the readings to keep for it', () => {
    const readings = [read('12'), read('13'), read('2', shapesOf('2')), read('45'), read('4?', shapesOf('45'))];
    const all = [
      { keep: false, text: '12' },
      { keep: true, text: '13' },
      { keep: false, text: '2' },
      { keep: false, text: '4' },
      { keep: false, text: '45' },
    ];
    // 12 is offered for the 2, so the 2 alone is not. A text that does not name its shapes teaches nothing.
    expect(untaught(readings, all, keptAdditions(readings, all))).toEqual({
      characters: ['2', '4', '5'],
      readings: [0, 4],
    });
    expect(untaughtNote(['2', '4', '5'])).toBe('No kept reading teaches 2, 4 and 5.');
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

describe("text that is not the reference's", () => {
  const digits = Array.from('0123456789');
  const readings = [read('12'), read('MENU'), read('PLAY 1'), read('SOS')];

  test('has no needed character and is not a needed text read as its lookalikes', () => {
    expect(unrelated('MENU', digits)).toBe(true);
    // Letters that are digits' lookalikes among others do not make a word the reference's.
    expect(['Score', 'Time', 'Lives', 'Lv'].map((text) => unrelated(text, digits))).toEqual([true, true, true, true]);
    expect(unrelated('PLAY 1', digits)).toBe(false);
    // Every character may be a digit misread: 505, 1205.
    expect(unrelated('SOS', digits)).toBe(false);
    expect(unrelated('12O5', digits)).toBe(false);
    expect(unrelated('', digits)).toBe(false);
    expect(unrelated('MENU', [])).toBe(false);
  });

  test('starts left out, says so, and is not offered as something to teach', () => {
    const all = entries(readings, new Map(), new Map(), new Map(), digits);
    expect(all.map((entry) => entry.keep)).toEqual([true, false, true, true]);
    expect(entries(readings, new Map(), new Map()).map((entry) => entry.keep)).toEqual([true, true, true, true]);
    const menu = all[1] ?? { keep: true, text: '' };
    expect(tileNote(read('MENU'), menu, undefined, undefined, digits)).toBe(
      'Shows none of the characters the reference needs, so it is left out unless you keep it.',
    );
    expect(tileNote(read('MENU'), { ...menu, keep: true }, undefined, undefined, digits)).toBeNull();
    const adds = keptAdditions(readings, all);
    expect(untaught(readings, all, adds, digits)).toEqual({ characters: [], readings: [] });
    expect(untaught(readings, all, adds).readings).toEqual([1]);
  });

  test('stays kept when it was kept before', () => {
    const all = entries(readings, new Map(), new Map([[1, 'MENU']]), new Map(), digits);
    expect(all[1]).toEqual({ keep: true, text: 'MENU' });
  });
});

describe('tile notes', () => {
  test('say why a kept reading cannot be learned first', () => {
    expect(tileNote(read('9l%'), { keep: true, text: '9l%' }, 'Its l has another shape.')).toBe(
      'Its l has another shape.',
    );
    expect(tileNote(read('9l%'), { keep: false, text: '9l%' }, 'Its l has another shape.')).toBeNull();
    // What the check says of a kept reading comes before a clash, which shows on a reading left out too.
    expect(tileNote(read('9l%'), { keep: true, text: '9l%' }, 'Its l has another shape.', 'Clash.')).toBe(
      'Its l has another shape.',
    );
    expect(tileNote(read('9l%'), { keep: false, text: '9l%' }, 'Its l has another shape.', 'Clash.')).toBe('Clash.');
    expect(tileNote(read(''), { keep: true, text: '' }, undefined, 'Clash.')).toBe('Clash.');
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

test('the recording part counts kept readings, and says the reference learns them', () => {
  // Nothing is learned before the reference is saved, and the References page has no value.
  expect(keptNote(12, 0, null)).toBe('12 readings recorded, none kept yet.');
  expect(keptNote(1, 0, null)).toBe('1 reading recorded, none kept yet.');
  expect(keptNote(12, 3, null)).toBe('3 of 12 readings kept.');
  expect(keptNote(1, 1, null)).toBe('1 of 1 reading kept.');
  expect(keptNote(12, 3, 5)).toBe('5 of 12 readings will be kept. The reference learns only the readings you keep.');
});

test("a late picture of a replaced recording leaves the newest recording's pictures alone", () => {
  const picture = (width: number) => ({ width, height: 1, data: new Uint8ClampedArray(width * 4) });
  const first = withCrop(NO_CROPS, 2, 2, 0, picture(3));
  expect(first).toEqual({ id: 2, pictures: new Map([[0, picture(3)]]) });
  // Recording 1 was discarded and recording 2 started at once; the answer for recording 1 arrives last.
  expect(withCrop(first, 2, 1, 0, picture(9))).toBe(first);
  // The newest recording's own pictures add up, and its first picture replaces an older recording's.
  expect(withCrop(first, 2, 2, 1, picture(4)).pictures).toEqual(
    new Map([
      [0, picture(3)],
      [1, picture(4)],
    ]),
  );
  expect(withCrop(first, 3, 3, 0, picture(5))).toEqual({ id: 3, pictures: new Map([[0, picture(5)]]) });
});

test('a reading whose picture ask failed is asked for again, and each other reading once', () => {
  const first = cropsToAsk(NO_CROP_ASKS, 2, 3);
  expect(first.indexes).toEqual([0, 1, 2]);
  expect(cropsToAsk(first.asks, 2, 4).indexes).toEqual([3]);
  // The ask for reading 1 failed; a failure reported for another recording changes nothing.
  expect(cropAskFailed(first.asks, 1, 1)).toBe(first.asks);
  const failed = cropAskFailed(first.asks, 2, 1);
  expect(cropsToAsk(failed, 2, 3).indexes).toEqual([1]);
  // Another recording's readings are all asked for.
  expect(cropsToAsk(failed, 3, 2).indexes).toEqual([0, 1]);
});
