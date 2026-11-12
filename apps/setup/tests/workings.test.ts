import { describe, expect, test } from 'bun:test';

import type { StateView } from '../src/lib/bindings.ts';
import { averageDifference, channelsText, closeness, cssColor, percentOf, readState } from '../src/lib/workings.ts';

describe('closeness', () => {
  test('is the share of inked cells that agree', () => {
    expect(closeness({ character: '7', differing: 2, inked: 8, matches: true })).toBe(75);
    expect(closeness({ character: '7', differing: 1, inked: 3, matches: false })).toBe(67);
    expect(closeness({ character: '7', differing: 0, inked: 0, matches: true })).toBe(100);
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
  });

  test('average their difference per channel', () => {
    expect(averageDifference(state('On', 61, true))).toBe(10.2);
    expect(averageDifference({ ...state('On', 0, true), channels: 0 })).toBe(null);
  });

  test('read as the closest within its tolerance, the first on a tie', () => {
    expect(readState([state('On', 30, true), state('Off', 12, true), state('Dim', 12, true)])).toBe('Off');
    expect(readState([state('On', 300, false), state('Off', 12, true)])).toBe('Off');
    expect(readState([state('On', 300, false)])).toBe(null);
    expect(readState([])).toBe(null);
  });
});
