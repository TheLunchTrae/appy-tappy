// The characters a reference needs to know, as the user chooses them, and how they compare with what it learned.
//
// The choice is one set of characters. The presets are a way to add and remove several of them at once, and each
// reads its own state back from the set, so nothing but the set is saved. The set only guides teaching: AppyTappy
// learns and reads from labels alone, whatever the set holds.

import { LIMITS, type ReferenceDef } from './bindings.ts';
import { knownCharacters, referenceCharacters } from './reference.ts';

/** Characters the user turns on or off as one. */
export interface Preset {
  /** What it is called on screen, such as `Decimal point`. */
  readonly name: string;
  /** Its characters as its name shows them, such as `0–9` or `.`. */
  readonly shown: string;
  readonly characters: readonly string[];
}

/**
 * The set of the characters in `text`, as a reference saves them: each character a label teaches that is no control
 * character, once, the digits first and the rest by code point. A list a person wrote into the file by hand reads
 * as this set, and the next save writes it so.
 */
export function expectedSet(text: string): readonly string[] {
  return knownCharacters([text]).filter((character) => !/\p{Cc}/u.test(character));
}

/** Whether `expected` holds more characters than a reference saves, which the save refuses. */
export function tooMany(expected: readonly string[]): boolean {
  return expected.length > LIMITS.maxExpectedCharacters;
}

/** What to do about a list that {@link tooMany} refuses. */
export const TOO_MANY = `A reference needs at most ${LIMITS.maxExpectedCharacters} characters. Remove some.`;

function preset(name: string, shown: string, characters: string): Preset {
  return { name, shown, characters: expectedSet(characters) };
}

/** The digits `0` to `9`. */
export const DIGITS = preset('Digits', '0–9', '0123456789');

/** What a number can show: its digits, and the signs each font may or may not use. */
export const NUMBER_PRESETS: readonly Preset[] = [
  DIGITS,
  preset('Decimal point', '.', '.'),
  preset('Comma', ',', ','),
  preset('Percent', '%', '%'),
  preset('Minus', '-', '-'),
  preset('Plus', '+', '+'),
  preset('Colon', ':', ':'),
];

/** The English letters, each case on its own, and the punctuation that goes with text. */
export const LETTER_PRESETS: readonly Preset[] = [
  preset('Uppercase', 'A–Z', 'ABCDEFGHIJKLMNOPQRSTUVWXYZ'),
  preset('Lowercase', 'a–z', 'abcdefghijklmnopqrstuvwxyz'),
  preset('Question mark', '?', '?'),
  preset('Exclamation mark', '!', '!'),
];

const PRESETS: readonly Preset[] = [...NUMBER_PRESETS, ...LETTER_PRESETS];

/**
 * The set a reference's editor starts with: what the saved reference `existing` expects, which is nothing when it was
 * saved without a choice. A new reference starts with what `like` expects, the reference it is learned again for
 * at another window size, when that has a list, or else with the digits, since numbers are what references read.
 */
export function startingExpected(existing: ReferenceDef | null, like: ReferenceDef | null): readonly string[] {
  if (existing !== null) {
    return expectedSet(existing.expects ?? '');
  }
  const shared = expectedSet(like?.expects ?? '');
  return shared.length > 0 ? shared : DIGITS.characters;
}

/** How much of a preset a set holds: every character, some of them, or none. */
export type PresetState = 'on' | 'partial' | 'off';

/** How many of `preset`'s characters `expected` holds. */
export function presetCount(expected: readonly string[], preset: Preset): number {
  return preset.characters.filter((character) => expected.includes(character)).length;
}

/** Whether `expected` holds all of `preset`'s characters, some, or none. */
export function presetState(expected: readonly string[], preset: Preset): PresetState {
  const count = presetCount(expected, preset);
  if (count === 0) {
    return 'off';
  }
  return count === preset.characters.length ? 'on' : 'partial';
}

/** `expected` with every character of `preset` when `on`, or with none of them. */
export function withPreset(expected: readonly string[], preset: Preset, on: boolean): readonly string[] {
  return on
    ? expectedSet([...expected, ...preset.characters].join(''))
    : expected.filter((character) => !preset.characters.includes(character));
}

/** The characters of `expected` that belong to a preset it holds whole. */
function presetCharacters(expected: readonly string[]): readonly string[] {
  const whole = PRESETS.filter((preset) => presetState(expected, preset) === 'on').flatMap(
    (preset) => preset.characters,
  );
  return expected.filter((character) => whole.includes(character));
}

/**
 * The characters of `expected` that no preset it holds whole covers, as the text of the field they are typed in. Part
 * of a preset, such as the digits 0 to 5, shows here, where each can be removed.
 */
export function otherCharacters(expected: readonly string[]): string {
  const covered = presetCharacters(expected);
  return expected.filter((character) => !covered.includes(character)).join('');
}

/**
 * `expected` with the characters of `text` in place of its {@link otherCharacters}. The presets it holds whole stay,
 * so typing a character one of them covers changes nothing.
 */
export function withOther(expected: readonly string[], text: string): readonly string[] {
  return expectedSet(presetCharacters(expected).join('') + text);
}

/** How the characters a reference learned compare with the ones it needs. */
export interface Coverage {
  /** The needed characters it learned. */
  readonly known: readonly string[];
  /** The needed characters it has yet to learn. */
  readonly missing: readonly string[];
  /** The characters it learned that are not needed, which is every one it learned when none is needed. */
  readonly extra: readonly string[];
}

/**
 * How `learned`, the characters a reference knows, compares with `expected`, the ones it needs. Each group keeps the
 * order of the list it came from.
 */
export function coverage(expected: readonly string[], learned: readonly string[]): Coverage {
  return {
    known: expected.filter((character) => learned.includes(character)),
    missing: expected.filter((character) => !learned.includes(character)),
    extra: learned.filter((character) => !expected.includes(character)),
  };
}

/** How the characters the saved `reference` learned from its pictures compare with the ones it expects. */
export function referenceCoverage(reference: ReferenceDef): Coverage {
  return coverage(expectedSet(reference.expects ?? ''), referenceCharacters(reference));
}
