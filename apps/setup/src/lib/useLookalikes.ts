// The characters a reference may read as each other, because the shapes it learned for them match each other, which
// the reference editor warns about.

import { useEffect, useRef, useState } from 'react';

import type { CharactersSpec } from './bindings.ts';
import { lookalikeCharacters } from './api.ts';
import type { Color } from './pixels.ts';
import { ask } from './request.ts';

/** Two characters whose learned shapes match each other. */
type Pair = readonly [string, string];

/** How long the samples must rest before the pairs are asked for, so a label asks once it is typed. */
const CHECK_DELAY_MS = 250;

/**
 * The pairs of `pairs` a value that needs `needs` can misread: those whose characters it both needs, or every pair
 * when it needs none and so reads every character the reference knows. A character it does not need is not read as
 * one it does.
 */
export function neededPairs(pairs: readonly Pair[], needs: readonly string[]): readonly Pair[] {
  return needs.length === 0 ? pairs : pairs.filter(([a, b]) => needs.includes(a) && needs.includes(b));
}

/**
 * What the reference editor says about `pairs`, the characters a reference with the text color `ink`, null for
 * contrast, may read as each other, or null for none. The fix it names is the one that changes what counts as the
 * characters: a text color, or with one a lower tolerance.
 */
export function lookalikeWarning(pairs: readonly Pair[], ink: Color | null): string | null {
  const [first, ...rest] = pairs;
  if (first === undefined) {
    return null;
  }
  const which =
    rest.length === 0
      ? `${first[0]} and ${first[1]} look alike in what it learned, so it may read one as the other.`
      : `Some characters look alike in what it learned, so it may read one of each pair as the other: ${pairs.map(([a, b]) => `${a} and ${b}`).join('; ')}.`;
  const fix =
    ink === null
      ? 'Text with a thick outline often does this. Pick a text color below, so only the inside of the characters counts.'
      : 'Try a lower tolerance, so less of the edge around the characters counts.';
  return `${which} ${fix}`;
}

/** A name for the pairs `spec` asks for, which stays while nothing they depend on changes, or null for none. */
export function lookalikeKey(spec: CharactersSpec | null): string | null {
  return spec === null ? null : JSON.stringify(spec);
}

/** An answer of the owner, and the key it was asked under. */
interface Answer {
  readonly key: string;
  readonly pairs: readonly Pair[];
}

/**
 * The pairs to show while the reference asks under `wanted`: those of `answer` when it was asked under that key, and
 * none otherwise, so an answer for samples, a region, or a text color since changed is dropped.
 */
export function pairsArrived(answer: Answer, wanted: string | null): readonly Pair[] {
  return answer.key === wanted ? answer.pairs : [];
}

/**
 * The pairs of characters whose shapes the reference `spec` teaches match each other, as the setup app answers for
 * that spec, or none while `spec` is null or the answer for it has not arrived. It asks again once a sample, the
 * region, the text color, or the tolerance changed and then rested. A spec that cannot be taught answers nothing,
 * and the check of the characters says what is wrong with it.
 */
export function useLookalikes(spec: CharactersSpec | null): readonly Pair[] {
  const key = lookalikeKey(spec);
  const [answer, setAnswer] = useState<Answer>({ key: '', pairs: [] });
  const latest = useRef(spec);
  latest.current = spec;
  useEffect(() => {
    const asked = latest.current;
    if (asked === null || key === null) {
      return undefined;
    }
    return ask(
      () => lookalikeCharacters(asked),
      (pairs) => {
        setAnswer({ key, pairs });
      },
      (error) => {
        console.error(error);
      },
      CHECK_DELAY_MS,
    );
  }, [key]);
  return pairsArrived(answer, key);
}
