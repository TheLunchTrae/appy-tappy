import { describe, expect, test } from 'bun:test';

import { CHECK_KINDS } from '../src/lib/check.ts';
import { movingContent, regionLabel } from '../src/lib/moving.ts';

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
    expect(bar.kind === 'area' && bar.limits.includes('either color')).toBe(true);
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
