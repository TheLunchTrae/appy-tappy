import { describe, expect, test } from 'bun:test';

import type { FoundView } from '../src/lib/bindings.ts';
import { addBlocked, askable, asLabels, flaggedFinds, foundRows, keptFound, rowFlags } from '../src/lib/found.ts';

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
  test("start kept with the reader's guess, and flag a letter, an empty failure, but not text still read", () => {
    const rows = foundRows(view, new Map(), ['87%']);
    expect(rows.map((row) => [row.keep, row.text, row.reading, row.flag])).toEqual([
      [true, '93.4%', false, null],
      [true, '12O5', false, 'O is a letter, and the reference has learned none. Type what this shows.'],
      [true, '', true, null],
      [true, '', false, 'The text reader stopped. Type what this shows.'],
    ]);
  });

  test("take the user's edits, and flag no letter once the reference learned one", () => {
    const edits = new Map([
      [1, { keep: true, text: '1205' }],
      [2, { keep: false, text: '' }],
      [3, { keep: true, text: '  ' }],
    ]);
    const rows = foundRows(view, edits, ['Lv 3']);
    expect(rows.map((row) => [row.keep, row.text, row.reading, row.flag])).toEqual([
      [true, '93.4%', false, null],
      [true, '1205', false, null],
      [false, '', false, null],
      [true, '  ', false, 'Type what this shows, or leave it out.'],
    ]);
    expect(foundRows(view, new Map(), ['Lv 3'])[1]?.flag).toBeNull();
  });
});

describe('adding found text', () => {
  const edits = new Map([
    [1, { keep: true, text: '1205' }],
    [3, { keep: false, text: '' }],
  ]);

  test('asks the reference about the kept, read, unflagged rows, and maps its answers back', () => {
    const rows = foundRows(
      { ...view, finds: view.finds.with(2, { region: at(80), text: { kind: 'read', text: '60%' } }) },
      edits,
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

  test('waits for text still read, and needs one kept', () => {
    const rows = foundRows(view, edits, []);
    expect(addBlocked(rows, rowFlags(rows, new Map()), true)).toBe('Reading the text…');
    const none = foundRows(view, new Map(view.finds.map((_, index) => [index, { keep: false, text: '' }])), []);
    expect(addBlocked(none, rowFlags(none, new Map()), true)).toBe('Keep at least one to add it.');
  });

  test('a flag on a row left out is no flag', () => {
    const rows = foundRows(view, new Map([[1, { keep: false, text: '12O5' }]]), []);
    expect(rowFlags(rows, new Map([[1, 'Nope.']])).has(1)).toBe(false);
  });
});
