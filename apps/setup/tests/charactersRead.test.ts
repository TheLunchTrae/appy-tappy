import { describe, expect, test } from 'bun:test';

import {
  forReference,
  readChoice,
  readChoices,
  savedCharacters,
  withCharacter,
  withReadChoice,
} from '../src/lib/charactersRead.ts';
import { checkSummary } from '../src/lib/check.ts';

const digits = ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'];
/** A reference that knows some digits, two symbols, and two letters. */
const known = ['0', '1', '2', '%', '.', 'L', 'v'];

describe('the characters a number reads', () => {
  test('a preset sets them, and reads itself back from them', () => {
    expect(withReadChoice('all', known, ['1'])).toBeNull();
    expect(readChoice(null, known)).toBe('all');
    // All ten digits, learned or not, so a digit learned later is read too.
    expect(withReadChoice('digits', known, null)).toEqual(digits);
    expect(readChoice(digits, known)).toBe('digits');
    expect(readChoice(digits.toReversed(), known)).toBe('digits');
    // The symbols the reference knows, and none of its letters.
    expect(withReadChoice('digitsAndSymbols', known, null)).toEqual([...digits, '%', '.']);
    expect(readChoice([...digits, '%', '.'], known)).toBe('digitsAndSymbols');
    expect(readChoice([...digits, '%'], known)).toBe('choose');
    expect(readChoice([...digits, '%', 'L'], known)).toBe('choose');
    expect(readChoice([], known)).toBe('choose');
  });

  test('choosing starts from what is read so far, learned or not', () => {
    expect(withReadChoice('choose', known, null)).toEqual(known);
    // The digits the reference has yet to learn stay, so reading one once it is learned does not fail.
    expect(withReadChoice('choose', known, digits)).toEqual(digits);
    expect(withReadChoice('choose', known, [...digits, '%', '.'])).toEqual([...digits, '%', '.']);
    expect(withReadChoice('choose', known, ['1', '%', 'x'])).toEqual(['1', '%', 'x']);
  });

  test('a digit turned off from Digits is the only one left out once the reference learns the rest', () => {
    const chosen = withCharacter(withReadChoice('choose', known, digits) ?? [], '9', false);
    const taught = [...known, '3', '4', '5', '6', '7', '8', '9'];
    expect(forReference(chosen, known, taught)).toEqual(digits.filter((digit) => digit !== '9'));
  });

  test('another reference keeps the choice that was picked', () => {
    const other = ['1', '5', ':', 'L'];
    expect(forReference(null, known, other)).toBeNull();
    expect(forReference(digits, known, other)).toEqual(digits);
    // Digits and symbols takes the new reference's symbols, and a chosen set keeps what the new one knows.
    expect(forReference([...digits, '%', '.'], known, other)).toEqual([...digits, ':']);
    expect(forReference(['1', '%', 'L'], known, other)).toEqual(['1', 'L']);
    // A saved set that was the digits and symbols before the reference learned another is a chosen set.
    expect(readChoice([...digits, '%'], known)).toBe('choose');
    // It loses only what the old reference knew and the new one does not: not the digits neither has learned.
    expect(forReference([...digits, '%'], known, known)).toEqual([...digits, '%']);
    expect(forReference([...digits, '%'], known, other)).toEqual(['1', '3', '4', '5', '6', '7', '8', '9']);
  });

  test('digits and symbols is offered only for a reference that knows a symbol', () => {
    expect(readChoices(known)).toEqual(['all', 'digits', 'digitsAndSymbols', 'choose']);
    expect(readChoices(['1', 'L'])).toEqual(['all', 'digits', 'choose']);
    // Without a symbol its set would be the digits, which Digits names.
    expect(readChoice(digits, ['1', 'L'])).toBe('digits');
  });

  test('a character turns on in order, and off', () => {
    expect(withCharacter(['%', '2'], '1', true)).toEqual(['1', '2', '%']);
    expect(withCharacter(['1', '2', '%'], '2', true)).toEqual(['1', '2', '%']);
    expect(withCharacter(['1', '2', '%'], '2', false)).toEqual(['1', '%']);
  });

  test('a saved check names them once each, or names none', () => {
    expect(savedCharacters(undefined)).toBeNull();
    expect(savedCharacters('% 1 01')).toEqual(['0', '1', '%']);
  });

  test("show in a saved check's summary", () => {
    const reference = {
      id: 'percent',
      name: 'Percent font',
      origin: { kind: 'byHand' } as const,
      ink: { red: 255, green: 255, blue: 255 },
      tolerance: 40,
      samples: [],
    };
    const check = { kind: 'number', reference: 'percent', whole: true } as const;
    expect(checkSummary(check, [reference]).name).toBe('Number, read with Percent font, whole only');
    expect(checkSummary({ ...check, characters: '0123456789%' }, [reference]).name).toBe(
      'Number, read with Percent font, whole only, reading 0123456789%',
    );
  });
});
