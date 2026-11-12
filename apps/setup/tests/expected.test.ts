import { expect, test } from 'bun:test';

import type { ReferenceDef } from '../src/lib/bindings.ts';
import {
  coverage,
  expectedSet,
  LETTER_PRESETS,
  NUMBER_PRESETS,
  otherCharacters,
  type Preset,
  presetCount,
  presetState,
  referenceCoverage,
  startingExpected,
  withOther,
  withPreset,
} from '../src/lib/expected.ts';

const PRESETS = [...NUMBER_PRESETS, ...LETTER_PRESETS];

function named(name: string): Preset {
  const found = PRESETS.find((each) => each.name === name);
  if (found === undefined) {
    throw new Error(`no preset named ${name}`);
  }
  return found;
}

const digits = named('Digits');
const DIGITS = ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'];

function reference(texts: readonly string[], expects?: string): ReferenceDef {
  return {
    id: 'percent',
    name: 'Percent font',
    ink: { red: 255, green: 255, blue: 255 },
    tolerance: 40,
    samples: texts.map((text, index) => ({ image: `ref-percent-${index}.png`, text })),
    ...(expects === undefined ? {} : { expects }),
  };
}

test('expectedSet keeps each character a label teaches once, digits first', () => {
  expect(expectedSet(' 9a%9\t\u0007A0\u0085')).toEqual(['0', '9', '%', 'A', 'a']);
  expect(expectedSet('')).toEqual([]);
});

test('a new reference starts with the digits, and a saved one with what it expects', () => {
  expect(startingExpected(null)).toEqual(DIGITS);
  expect(startingExpected(reference(['87%']))).toEqual([]);
  expect(startingExpected(reference(['87%'], '%10'))).toEqual(['0', '1', '%']);
});

test('a preset is on with all of its characters, partial with some, and off with none', () => {
  expect(presetState(DIGITS, digits)).toBe('on');
  expect(presetState(['3', '7', '%'], digits)).toBe('partial');
  expect(presetCount(['3', '7', '%'], digits)).toBe(2);
  expect(presetState(['%', 'a'], digits)).toBe('off');
  expect(presetState(['%', 'a'], named('Percent'))).toBe('on');
  expect(presetState(['%', 'a'], named('Lowercase'))).toBe('partial');
  expect(presetState(['%', 'a'], named('Uppercase'))).toBe('off');
});

test('turning a preset on adds its characters and off removes only them', () => {
  expect(withPreset(['3', '%'], digits, true)).toEqual([...DIGITS, '%']);
  expect(withPreset([...DIGITS, '%', '/'], digits, false)).toEqual(['%', '/']);
  expect(withPreset(['3', '%'], digits, false)).toEqual(['%']);
  expect(withPreset(['A'], named('Lowercase'), true).length).toBe(27);
});

test('each preset turns on whole and leaves the others off', () => {
  for (const each of PRESETS) {
    const only = withPreset([], each, true);
    expect(PRESETS.filter((other) => presetState(only, other) !== 'off')).toEqual([each]);
    expect(presetState(only, each)).toBe('on');
  }
});

test('the other characters are the ones no whole preset covers', () => {
  expect(otherCharacters([...DIGITS, '%', '/', '°'])).toBe('/°');
  // Part of a preset shows, so each of its characters can be removed.
  expect(otherCharacters(['0', '1', '2', '%'])).toBe('012');
  expect(otherCharacters([])).toBe('');
});

test('typing other characters replaces them and keeps the whole presets', () => {
  const start = [...DIGITS, '%', '/'];
  expect(withOther(start, '/x')).toEqual([...DIGITS, '%', '/', 'x']);
  expect(withOther(start, '')).toEqual([...DIGITS, '%']);
  // A character a preset covers is harmless, and whitespace is no character.
  expect(withOther(start, '5 %')).toEqual([...DIGITS, '%']);
  // Typed digits complete the preset, and deleting one takes it away again.
  expect(withOther(['%'], '0123456789')).toEqual([...DIGITS, '%']);
  expect(withOther(['%'], '012345678')).toEqual([...DIGITS.slice(0, 9), '%']);
});

test('a set shown as presets and other characters builds the same set again', () => {
  const sets = ['', '0123456789', '0123456789.,%-+:', '01234', '37%/°x', 'ABCDEFGHIJKLMNOPQRSTUVWXYZabc?!', '¥円'];
  for (const text of sets) {
    const set = expectedSet(text);
    const fromPresets = PRESETS.filter((each) => presetState(set, each) === 'on').reduce<readonly string[]>(
      (built, each) => withPreset(built, each, true),
      [],
    );
    expect(withOther(fromPresets, otherCharacters(set))).toEqual(set);
    expect(withOther(set, otherCharacters(set))).toEqual(set);
  }
});

test('coverage splits the needed characters by whether they are learned, and lists the rest', () => {
  expect(coverage([...DIGITS, '%'], ['0', '1', '8', '%', '/'])).toEqual({
    known: ['0', '1', '8', '%'],
    missing: ['2', '3', '4', '5', '6', '7', '9'],
    extra: ['/'],
  });
  // Without a choice of what is needed, nothing is missing and nothing learned is flagged.
  expect(coverage([], ['8', '%'])).toEqual({ known: [], missing: [], extra: ['8', '%'] });
  expect(coverage(['8'], [])).toEqual({ known: [], missing: ['8'], extra: [] });
});

test("a saved reference's coverage compares its samples with what it expects", () => {
  expect(referenceCoverage(reference(['87%', '10'], '0123456789%')).missing).toEqual(['2', '3', '4', '5', '6', '9']);
  expect(referenceCoverage(reference(['87%'], '78%'))).toEqual({ known: ['7', '8', '%'], missing: [], extra: [] });
  // A reference saved without a choice has nothing missing.
  expect(referenceCoverage(reference(['87%']))).toEqual({ known: [], missing: [], extra: ['7', '8', '%'] });
});
