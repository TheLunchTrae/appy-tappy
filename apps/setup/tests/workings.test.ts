import { describe, expect, test } from 'bun:test';

import type { StateView } from '../src/lib/bindings.ts';
import { averageDifference, channelsText, cssColor, distanceText, percentOf } from '../src/lib/workings.ts';

describe('distanceText', () => {
  test('gives the cells apart of the cells set, then the cells that differ', () => {
    expect(distanceText({ character: '7', apart: 3, differing: 5, set: 67, matches: false })).toBe(
      '3 of 67 apart, 5 differ',
    );
  });
});

describe('percentOf', () => {
  test('rounds, and has no share of nothing', () => {
    expect(percentOf(1, 3)).toBe(33);
    expect(percentOf(0, 0)).toBe(null);
  });
});

describe('colors', () => {
  test('read as CSS and as channels', () => {
    const color = { red: 200, green: 40, blue: 4 };
    expect(cssColor(color)).toBe('rgb(200, 40, 4)');
    expect(channelsText(color)).toBe('R 200 G 40 B 4');
  });
});

describe('states', () => {
  const state = (name: string, total: number, within: boolean): StateView => ({
    name,
    total,
    channels: 6,
    tolerance: 10,
    within,
    read: false,
  });

  test('average their difference per channel, rounded up', () => {
    expect(averageDifference(state('On', 61, true))).toBe(10.2);
    expect(averageDifference({ ...state('On', 0, true), channels: 0 })).toBe(null);
    // 10.04 is outside a tolerance of 10, so it never shows as 10.
    expect(averageDifference({ ...state('On', 251, false), channels: 25 })).toBe(10.1);
  });
});
