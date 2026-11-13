import { describe, expect, test } from 'bun:test';

import type { FoundView } from '../src/lib/bindings.ts';
import {
  addBlocked,
  askable,
  asLabels,
  fitsNeeds,
  flaggedFinds,
  foundRows,
  keepAll,
  keptFound,
  keptState,
  rowFlags,
} from '../src/lib/found.ts';

const at = (x: number) => ({ x, y: 10, width: 30, height: 12 });

const view: FoundView = {
  id: 1,
  screenshot: 2,
  finds: [
    { region: at(0), text: { kind: 'read', text: '93.4%' } },
    { region: at(40), text: { kind: 'read', text: '12O5' } },
    { region: at(80), text: { kind: 'pending' } },
    { region: at(120), text: { kind: 'failed', text: 'The text reader stopped. Type what this shows.' } },
  ],
  more: 0,
  otherSizes: 2,
};

describe('foundRows', () => {
  const kept = new Map(view.finds.map((_, index) => [index, { keep: true, text: '', typed: false }]));

  test("start left out with the reader's guess", () => {
    const rows = foundRows(view, new Map(), ['87%'], []);
    expect(rows.map((row) => [row.keep, row.text, row.reading, row.flag])).toEqual([
      [false, '93.4%', false, null],
      [false, '12O5', false, null],
      [false, '', true, null],
      [false, '', false, null],
    ]);
  });

  test('flag a kept letter and an empty failure, but not text still read', () => {
    const rows = foundRows(view, kept, ['87%'], []);
    expect(rows.map((row) => [row.keep, row.text, row.reading, row.flag])).toEqual([
      [true, '93.4%', false, null],
      [true, '12O5', false, 'O is a letter, and the reference has learned none. Type what this shows.'],
      [true, '', true, null],
      [true, '', false, 'The text reader stopped. Type what this shows.'],
    ]);
  });

  test("take the user's edits, and flag no letter once the reference learned one", () => {
    const edits = new Map([
      [0, { keep: true, text: '', typed: false }],
      [1, { keep: true, text: '1205', typed: true }],
      [2, { keep: false, text: '', typed: false }],
      [3, { keep: true, text: '  ', typed: true }],
    ]);
    const rows = foundRows(view, edits, ['Lv 3'], []);
    expect(rows.map((row) => [row.keep, row.text, row.reading, row.flag])).toEqual([
      [true, '93.4%', false, null],
      [true, '1205', false, null],
      [false, '', true, null],
      [true, '  ', false, 'Type what this shows, or leave it out.'],
    ]);
    expect(foundRows(view, kept, ['Lv 3'], [])[1]?.flag).toBeNull();
  });

  test('take letters the user typed, but not a guess only kept by ticking its box', () => {
    const typed = foundRows(view, new Map([[1, { keep: true, text: 'Lv 3', typed: true }]]), ['87%'], []);
    expect(typed[1]?.flag).toBeNull();
    const ticked = foundRows(view, new Map([[1, { keep: true, text: '', typed: false }]]), ['87%'], []);
    expect(ticked[1]?.flag).toBe('O is a letter, and the reference has learned none. Type what this shows.');
  });

  test('list only text that may show the characters the reference needs, a lookalike digit included', () => {
    const digits = ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'];
    const rows = foundRows(view, kept, ['87%'], digits);
    // 93.4% has a . and a %; 12O5's O may be a 0; text still read and a failure to read are listed.
    expect(rows.map((row) => [row.fits, row.keep, row.flag])).toEqual([
      [false, false, null],
      [true, true, 'O is a letter, and the reference has learned none. Type what this shows.'],
      [true, true, null],
      [true, true, 'The text reader stopped. Type what this shows.'],
    ]);
    // Text the user typed is listed, whatever it shows.
    const typed = foundRows(view, new Map([[0, { keep: true, text: '93', typed: true }]]), [], digits);
    expect(typed[0]?.fits).toBe(true);
  });
});

test('fitsNeeds takes any text while nothing is needed, and ignores spaces', () => {
  expect(fitsNeeds('Lv 3', [])).toBe(true);
  expect(fitsNeeds('1 2', ['1', '2'])).toBe(true);
  expect(fitsNeeds('l2', ['1', '2'])).toBe(true);
  expect(fitsNeeds('12%', ['1', '2'])).toBe(false);
  expect(fitsNeeds('A', ['A'])).toBe(true);
});

describe('select all', () => {
  const digits = ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'];

  test('keeps or leaves out every listed find, and only those', () => {
    const rows = foundRows(view, new Map(), [], digits);
    expect([...keepAll(rows, true).keys()]).toEqual([1, 2, 3]);
    const all = foundRows(view, keepAll(rows, true), [], digits);
    expect(all.map((row) => row.keep)).toEqual([false, true, true, true]);
    expect(keptState(all)).toBe('all');
    expect(keptState(foundRows(view, keepAll(all, false), [], digits))).toBe('none');
    expect(keptState(foundRows(view, new Map([[1, { keep: true, text: '', typed: false }]]), [], digits))).toBe('some');
  });

  test("keeps the text the user typed, and leaves the reader's guess to the reader", () => {
    const typed = foundRows(view, new Map([[1, { keep: false, text: '1205', typed: true }]]), [], []);
    expect(keepAll(typed, true).get(1)).toEqual({ keep: true, text: '1205', typed: true });
    expect(keepAll(typed, true).get(0)).toEqual({ keep: true, text: '93.4%', typed: false });
  });
});

describe('adding found text', () => {
  const edits = new Map([
    [0, { keep: true, text: '', typed: false }],
    [1, { keep: true, text: '1205', typed: true }],
    [2, { keep: true, text: '', typed: false }],
    [3, { keep: false, text: '', typed: false }],
  ]);

  test('asks the reference about the kept, read, unflagged rows, and maps its answers back', () => {
    const rows = foundRows(
      { ...view, finds: view.finds.with(2, { region: at(80), text: { kind: 'read', text: '60%' } }) },
      edits,
      [],
      [],
    );
    const asked = askable(rows);
    expect(asked.map((row) => row.index)).toEqual([0, 1, 2]);
    expect(asLabels(view.screenshot, asked)).toEqual([
      { screenshot: 2, region: at(0), text: '93.4%' },
      { screenshot: 2, region: at(40), text: '1205' },
      { screenshot: 2, region: at(80), text: '60%' },
    ]);
    const learned = flaggedFinds(asked, [null, 'The region shows 1 characters but the text has 4.', null]);
    expect([...learned]).toEqual([[1, 'The region shows 1 characters but the text has 4.']]);
    expect(addBlocked(rows, rowFlags(rows, learned), true)).toBe('Fix or leave out the flagged one to add them.');
    expect(addBlocked(rows, rowFlags(rows, new Map()), false)).toBe('Checking the text…');
    expect(addBlocked(rows, rowFlags(rows, new Map()), true)).toBeNull();
    expect(keptFound(view.screenshot, rows)).toEqual([
      { screenshot: 2, region: at(0), text: '93.4%' },
      { screenshot: 2, region: at(40), text: '1205' },
      { screenshot: 2, region: at(80), text: '60%' },
    ]);
  });

  test('waits for text still read, and needs one ticked', () => {
    const rows = foundRows(view, edits, [], []);
    expect(addBlocked(rows, rowFlags(rows, new Map()), true)).toBe('Reading the text…');
    const none = foundRows(view, new Map(), [], []);
    expect(addBlocked(none, rowFlags(none, new Map()), true)).toBe('Tick at least one to add it.');
  });

  test('a flag on a row left out is no flag', () => {
    const rows = foundRows(view, new Map([[1, { keep: false, text: '12O5', typed: false }]]), [], []);
    expect(rowFlags(rows, new Map([[1, 'Nope.']])).has(1)).toBe(false);
  });
});
