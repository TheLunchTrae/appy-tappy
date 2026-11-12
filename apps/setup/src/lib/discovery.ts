// What a recording has found so far, by character: what is still to show, what the text reader guessed, and what the
// user has checked.
//
// A character is found when a kept reading's text names a shape as it. That text is the text reader's guess until
// the user checks the reading in review, and the reference learns it only once the user keeps the reading. So the
// three are told apart, and nothing here counts a guess as learned.

import type { RecordedReading } from './bindings.ts';
import { coverage } from './expected.ts';
import { knownCharacters } from './reference.ts';
import { type ReviewEntry, named } from './review.ts';

/** The characters a recording found, against the ones its reference needs. */
export interface Discovery {
  /** The needed characters that nothing shows yet: neither the reference nor a kept reading. None when none is needed. */
  readonly missing: readonly string[];
  /** The characters only the text reader's guesses show, which the user has yet to check in review. */
  readonly unchecked: readonly string[];
  /** The characters the reference knows or a reading the user checked shows, the needed ones first. */
  readonly found: readonly string[];
  /** How many of the recording's shapes no reading's text names, kept or left out. */
  readonly unnamed: number;
}

/**
 * What the recording's `readings` found, for a reference that needs `expected` and knows `learned` without them.
 * `entries` holds review's choice for each reading, `checked` the readings the user has checked, and `shapes` how
 * many different shapes the recording saw.
 */
export function discovery(
  expected: readonly string[],
  learned: readonly string[],
  readings: readonly RecordedReading[],
  entries: readonly ReviewEntry[],
  checked: ReadonlySet<number>,
  shapes: number,
): Discovery {
  const sure: string[] = [];
  const guessed: string[] = [];
  const namedShapes = new Set<number>();
  readings.forEach((reading, index) => {
    const entry = entries[index];
    const characters = entry === undefined ? null : named(reading, entry.text);
    if (characters === null) {
      return;
    }
    for (const shape of reading.shapes) {
      namedShapes.add(shape);
    }
    // Only a kept reading finds its characters: one left out teaches nothing.
    if (entry?.keep === true) {
      (checked.has(index) ? sure : guessed).push(...characters);
    }
  });
  const have = knownCharacters([...learned, ...sure]);
  const compared = coverage(expected, have);
  const unchecked = knownCharacters(guessed).filter((character) => !have.includes(character));
  return {
    missing: compared.missing.filter((character) => !unchecked.includes(character)),
    unchecked,
    found: [...compared.known, ...compared.extra],
    unnamed: shapes - namedShapes.size,
  };
}
