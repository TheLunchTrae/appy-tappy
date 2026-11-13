import { describe, expect, test } from 'bun:test';

import type { RecordedReading } from '../src/lib/bindings.ts';
import { discovery } from '../src/lib/discovery.ts';

/** A reading of `shapes`; what the text reader read does not matter here, only review's entry does. */
const reading = (...shapes: number[]): RecordedReading => ({
  firstMs: 0,
  seen: 1,
  size: null,
  text: { kind: 'pending' },
  shapes,
  width: 0,
  characters: [],
});
const kept = (text: string) => ({ keep: true, text });
const DIGITS = Array.from('0123456789');

describe('what a recording found', () => {
  test('a guess is found but not checked, and its character is no longer missing', () => {
    const found = discovery(
      ['0', '1', '%'],
      [],
      [reading(0, 2), reading(1, 2)],
      [kept('0%'), kept('1%')],
      new Set(),
      3,
    );
    expect(found).toEqual({ missing: [], unchecked: ['0', '1', '%'], found: [], unnamed: 0 });
  });

  test('a checked reading and what the reference knows count as found', () => {
    const found = discovery(
      DIGITS,
      ['7', '8', '%'],
      [reading(0, 1), reading(2)],
      [kept('12'), kept('3')],
      new Set([0]),
      3,
    );
    expect(found).toEqual({
      missing: ['0', '4', '5', '6', '9'],
      unchecked: ['3'],
      found: ['1', '2', '7', '8', '%'],
      unnamed: 0,
    });
  });

  test('a guess of a character already found adds nothing to check', () => {
    const found = discovery(['1'], ['1'], [reading(0)], [kept('1')], new Set(), 1);
    expect(found).toEqual({ missing: [], unchecked: [], found: ['1'], unnamed: 0 });
  });

  test('a reading left out, or whose text does not name its shapes, finds nothing', () => {
    const readings = [reading(0), reading(1, 2), reading(3)];
    const entries = [{ keep: false, text: '1' }, kept('2'), kept('')];
    // The reading left out still names its shape, so only the other three are without text.
    expect(discovery(['1', '2'], [], readings, entries, new Set([0, 1, 2]), 4)).toEqual({
      missing: ['1', '2'],
      unchecked: [],
      found: [],
      unnamed: 3,
    });
    // A shape counts as named once any reading names it.
    expect(discovery([], [], readings, [kept('1'), kept('12'), kept('')], new Set(), 4).unnamed).toBe(1);
  });

  test('with no characters needed, nothing is missing and everything found shows', () => {
    const found = discovery([], ['a'], [reading(0), reading(1)], [kept('5'), kept('O')], new Set([0]), 2);
    expect(found).toEqual({ missing: [], unchecked: ['O'], found: ['5', 'a'], unnamed: 0 });
  });
});
