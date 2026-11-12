import { describe, expect, test } from 'bun:test';

import { CHECK_KINDS } from '../src/lib/check.ts';
import { fixNote, isReadingArea, movingContent, regionLabel } from '../src/lib/moving.ts';

describe('movingContent', () => {
  test('draws the region around the value while its content stays put', () => {
    for (const kind of CHECK_KINDS) {
      expect(movingContent(kind, false)).toEqual({ kind: 'fixed' });
    }
  });

  test('reads a number or a bar anywhere in a reading area, and says what else the area would read', () => {
    const number = movingContent('number', true);
    expect(number.kind === 'area' && number.limits.includes("characters' color")).toBe(true);
    const bar = movingContent('barFill', true);
    expect(bar.kind === 'area' && bar.limits.includes('near the filled or the empty color')).toBe(true);
  });

  test('offers no reading area to a check that compares or averages its whole region', () => {
    for (const kind of ['color', 'state'] as const) {
      const moving = movingContent(kind, true);
      expect(moving.kind === 'unsupported' && moving.why.includes('Make a value for each place')).toBe(true);
    }
  });
});

describe('regionLabel', () => {
  test('names the region after the value, and says when it is a reading area', () => {
    const area = movingContent('number', true);
    expect(regionLabel(' Score ', { kind: 'fixed' })).toBe('Score');
    expect(regionLabel('', { kind: 'fixed' })).toBe('Region');
    expect(regionLabel(' Score ', area)).toBe('Score reading area');
    expect(regionLabel('', area)).toBe('Reading area');
    // A check that cannot read content that moves keeps a plain region.
    expect(regionLabel('Score', movingContent('state', true))).toBe('Score');
  });
});

describe('isReadingArea', () => {
  test('saves a reading area only for a check that can read one', () => {
    expect(isReadingArea(movingContent('number', true))).toBe(true);
    expect(isReadingArea(movingContent('barFill', true))).toBe(true);
    // The option stays on while the check's kind changes, and the value saves without it.
    expect(isReadingArea(movingContent('color', true))).toBe(false);
    expect(isReadingArea(movingContent('state', true))).toBe(false);
    expect(isReadingArea(movingContent('number', false))).toBe(false);
  });
});

describe('fixNote', () => {
  test("asks a reading area's fix to cover everywhere the value can show, not to fit the value", () => {
    const area = movingContent('number', true);
    expect(fixNote(area, false)).toBe(
      'Take a screenshot, then draw the reading area around everywhere the value can show at this size.',
    );
    expect(fixNote(area, true)).toBe(
      'Starts from the scaled reading area. Move or resize it so it covers everywhere the value can show at this size.',
    );
    expect(fixNote({ kind: 'fixed' }, false)).toBe(
      'Take a screenshot, then draw the region where the value shows at this size.',
    );
    expect(fixNote({ kind: 'fixed' }, true)).toBe(
      'Starts from the scaled region. Move it onto the value where it shows at this size.',
    );
  });
});
