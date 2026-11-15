// What may keep a reference from reading as its samples do, which the reference editor warns about: characters it may
// read as each other, because the shapes it learned for them match each other, and an outline or a shadow that its
// tolerance leaves out of its samples.

import { useEffect, useRef, useState } from 'react';

import type { CharactersSpec, ReadingRisks } from './bindings.ts';
import { readingRisks } from './api.ts';
import type { Color } from './pixels.ts';
import { ask } from './request.ts';

/** Two characters whose learned shapes match each other. */
type Pair = readonly [string, string];

/** How long the samples must rest before the risks are asked for, so a label asks once it is typed. */
const CHECK_DELAY_MS = 250;

/**
 * The pairs of `pairs` a value that needs `needs` can misread: those with a character it needs, or every pair when it
 * needs none and so reads every character the reference knows. A read takes a character it does not need for a
 * lookalike it does, and a lookalike it does not need that is a digit fails the read.
 */
export function neededPairs(pairs: readonly Pair[], needs: readonly string[]): readonly Pair[] {
  return needs.length === 0 ? pairs : pairs.filter(([a, b]) => needs.includes(a) || needs.includes(b));
}

/**
 * What the reference editor says about `pairs`, the characters a reference with the text color `ink`, null for
 * contrast, and `tolerance` may read as each other, or null for none. The fix it names is the one that changes what
 * counts as the characters: a text color, with one a lower tolerance, and at none a color picked again.
 */
export function lookalikeWarning(pairs: readonly Pair[], ink: Color | null, tolerance: number): string | null {
  const [first, ...rest] = pairs;
  if (first === undefined) {
    return null;
  }
  const which =
    rest.length === 0
      ? `${first[0]} and ${first[1]} look alike in what it learned, so it may read one as the other.`
      : `Some characters look alike in what it learned, so it may read one of each pair as the other: ${pairs.map(([a, b]) => `${a} and ${b}`).join('; ')}.`;
  let fix = 'Try a lower tolerance, so less of the edge around the characters counts.';
  if (ink === null) {
    fix =
      'Text with a thick outline often does this. Pick a text color below, so only the inside of the characters counts.';
  } else if (tolerance === 0) {
    fix = 'Pick the text color again from the middle of a character, so less of its edge counts.';
  }
  return `${which} ${fix}`;
}

/**
 * What the reference editor says of a reference whose tolerance leaves an outline or a shadow out of its samples. It
 * reads while the background hides that tone as the samples' does, and not over another.
 */
export const HIDDEN_TONE_WARNING =
  "The characters have an outline or a shadow that doesn't count here, because it is close to the background in what it learned. Over a background of another color it counts, and the number can't be read. Pick a text color below, so only the inside of the characters counts.";

/** A name for the risks `spec` asks for, which stays while nothing they depend on changes, or null for none. */
export function riskKey(spec: CharactersSpec | null): string | null {
  return spec === null ? null : JSON.stringify(spec);
}

/** An answer of the owner, and the key it was asked under. */
interface Answer {
  readonly key: string;
  readonly risks: ReadingRisks;
}

/** What a reference risks before it is answered. */
const NO_RISKS: ReadingRisks = { lookalikes: [], hidesTone: false };

/** What the reference has been answered before it asks. */
const NO_ANSWER: Answer = { key: '', risks: NO_RISKS };

/**
 * The risks to show while the reference asks under `wanted`: those of `answer` when it was asked under that key, and
 * none otherwise, so an answer for samples, a region, or a text color since changed is dropped.
 */
export function risksArrived(answer: Answer, wanted: string | null): ReadingRisks {
  return answer.key === wanted ? answer.risks : NO_RISKS;
}

/**
 * What may keep the reference `spec` teaches from reading as its samples do, as the setup app answers for that spec,
 * or nothing while `spec` is null or the answer for it has not arrived. It asks again once a sample, the region, the
 * text color, or the tolerance changed and then rested. A spec that cannot be taught answers nothing, and the check
 * of the characters says what is wrong with it.
 */
export function useReadingRisks(spec: CharactersSpec | null): ReadingRisks {
  const key = riskKey(spec);
  const [answer, setAnswer] = useState<Answer>(NO_ANSWER);
  const latest = useRef(spec);
  latest.current = spec;
  useEffect(() => {
    const asked = latest.current;
    if (asked === null || key === null) {
      return undefined;
    }
    const cancel = ask(
      () => readingRisks(asked),
      (risks) => {
        setAnswer({ key, risks });
      },
      (error) => {
        console.error(error);
      },
      CHECK_DELAY_MS,
    );
    return () => {
      cancel();
      // A reference saved again keeps its ID and so its key, so an editor that opens again on that key asks afresh
      // rather than showing the answer about the samples it had before.
      setAnswer(NO_ANSWER);
    };
  }, [key]);
  return risksArrived(answer, key);
}
