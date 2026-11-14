// The characters of its reference that a number value reads, as the user chooses them.
//
// A reference can know a whole font, and a value that shows only some of it reads better against those characters
// alone. The choice is one set of characters, or none for every character the reference knows. The presets are ways
// to set it, and each reads its own state back from the set, so nothing but the set is saved.

import { DIGITS } from './expected.ts';
import { knownCharacters } from './reference.ts';

/** The ways to choose, in the order the field offers them. */
const READ_CHOICES = ['all', 'digits', 'digitsAndSymbols', 'choose'] as const;

export type ReadChoice = (typeof READ_CHOICES)[number];

/** What each choice is called on screen. */
export const READ_CHOICE_NAMES: Readonly<Record<ReadChoice, string>> = {
  all: 'All it knows',
  digits: 'Digits',
  digitsAndSymbols: 'Digits and symbols',
  choose: 'Choose…',
};

/** The characters of `known` that are neither a digit `0` to `9` nor a letter, such as `%` and `.`. */
function symbols(known: readonly string[]): readonly string[] {
  return known.filter((character) => !DIGITS.characters.includes(character) && !/\p{L}/u.test(character));
}

function digitsAndSymbols(known: readonly string[]): readonly string[] {
  return [...DIGITS.characters, ...symbols(known)];
}

function sameSet(left: readonly string[], right: readonly string[]): boolean {
  return left.length === right.length && left.every((character) => right.includes(character));
}

/**
 * The choices to offer for a reference that knows `known`. Digits and symbols is one only while the reference knows a
 * symbol, since without one it is Digits.
 */
export function readChoices(known: readonly string[]): readonly ReadChoice[] {
  return READ_CHOICES.filter((choice) => choice !== 'digitsAndSymbols' || symbols(known).length > 0);
}

/**
 * The choice that `characters` are, the set a value reads of a reference that knows `known`, or null for all of
 * them: a preset whose set it is, or else Choose.
 */
export function readChoice(characters: readonly string[] | null, known: readonly string[]): ReadChoice {
  if (characters === null) {
    return 'all';
  }
  if (sameSet(characters, DIGITS.characters)) {
    return 'digits';
  }
  return readChoices(known).includes('digitsAndSymbols') && sameSet(characters, digitsAndSymbols(known))
    ? 'digitsAndSymbols'
    : 'choose';
}

/**
 * The set a value reads once `choice` is picked, or null for every character its reference knows. Digits is all ten,
 * learned or not, so a digit the reference learns later is read too. Digits and symbols adds the symbols the
 * reference knows now. Choose starts from `current`, what the value reads so far, learned or not, to change one by
 * one, so a digit that Digits reads stays read once the reference learns it.
 */
export function withReadChoice(
  choice: ReadChoice,
  known: readonly string[],
  current: readonly string[] | null,
): readonly string[] | null {
  switch (choice) {
    case 'all':
      return null;
    case 'digits':
      return DIGITS.characters;
    case 'digitsAndSymbols':
      return digitsAndSymbols(known);
    case 'choose':
      return current ?? known;
  }
}

/**
 * What a value that read `characters` of a reference that knows `from` reads once it reads with one that knows `to`
 * instead, so its choice stays the one that was picked: all of it and the digits as they are, or the digits and the
 * new reference's symbols. A chosen set stays as it is, since a character the new reference has yet to learn shows
 * outlined to turn off, and is read once it is learned. `from` is what the reference knew when the choice was made,
 * since the set is a preset's only against that.
 */
export function forReference(
  characters: readonly string[] | null,
  from: readonly string[],
  to: readonly string[],
): readonly string[] | null {
  return withReadChoice(readChoice(characters, from), to, characters);
}

/** `characters` with `character` when `on`, or without it. */
export function withCharacter(characters: readonly string[], character: string, on: boolean): readonly string[] {
  return on ? knownCharacters([[...characters, character].join('')]) : characters.filter((each) => each !== character);
}

/** The set a saved number check reads, from its file's `text`, or null when the file names none. */
export function savedCharacters(text: string | undefined): readonly string[] | null {
  return text === undefined ? null : knownCharacters([text]);
}
