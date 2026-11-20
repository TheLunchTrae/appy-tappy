// What may keep a reference from reading as its samples do, which the reference editor warns about: characters it may
// read as each other, because the shapes it learned for them match each other, and an outline or a shadow that its
// tolerance leaves out of its samples.

import { useEffect, useRef, useState } from 'react';

import type { CharactersSpec, ReadingRisks, ReferenceDef } from './bindings.ts';
import { errorText, readingRisks, savedReadingRisks } from './api.ts';
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
 * reads while the background hides that tone as the samples' does, and may not over another.
 */
export const HIDDEN_TONE_WARNING =
  "An outline or a shadow around the characters is close to the background color in what this reference learned from, so it isn't part of the shapes it learned. Over a background of another color it can count, and the number won't read. Pick a text color below, so only the inside of the characters counts.";

/** A name for the risks `spec` asks for, which stays while nothing they depend on changes, or null for none. */
export function riskKey(spec: CharactersSpec | null): string | null {
  return spec === null ? null : JSON.stringify(spec);
}

/** What the editor shows of a reference's risks: the risks, or why they could not be checked. */
export interface ShownRisks extends ReadingRisks {
  /** Why the check failed, as a sentence, or null when it answered or has yet to. */
  readonly problem: string | null;
}

/** An answer of the owner, and the key it was asked under. */
interface Answer {
  readonly key: string;
  readonly risks: ShownRisks;
}

/** What a reference risks before it is answered. */
const NO_RISKS: ShownRisks = { lookalikes: [], hidesTone: false, problem: null };

/** What the reference has been answered before it asks. */
const NO_ANSWER: Answer = { key: '', risks: NO_RISKS };

/** What the editor says when the check for `error` failed: no risks, which is not the same as none found. */
export function risksFailed(error: unknown): ShownRisks {
  return { ...NO_RISKS, problem: `Can't check for characters that look alike. ${errorText(error)}` };
}

/**
 * The risks to show while the reference asks under `wanted`: those of `answer` when it was asked under that key, and
 * none otherwise, so an answer or a failure for samples, a region, or a text color since changed is dropped.
 */
export function risksArrived(answer: Answer, wanted: string | null): ShownRisks {
  return answer.key === wanted ? answer.risks : NO_RISKS;
}

/**
 * What may keep the reference `spec` teaches from reading as its samples do, as the setup app answers for that spec,
 * or nothing while `spec` is null or the answer for it has not arrived. It asks again once a sample, the region, the
 * text color, or the tolerance changed and then rested. A request that fails says so in place of the risks, until
 * the next one answers.
 */
export function useReadingRisks(spec: CharactersSpec | null): ShownRisks {
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
        setAnswer({ key, risks: { ...risks, problem: null } });
      },
      (error) => {
        console.error(error);
        setAnswer({ key, risks: risksFailed(error) });
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

/**
 * What the References page says of a saved reference that `risks` may keep from reading as its samples do, when a
 * value that reads with it needs `needs`, or null for one with nothing to warn about. Its editor says what to do.
 */
export function riskMark(risks: ReadingRisks, needs: readonly string[]): string | null {
  const pairs = neededPairs(risks.lookalikes, needs);
  const [first] = pairs;
  const reasons = [
    first === undefined
      ? null
      : pairs.length === 1
        ? `${first[0]} and ${first[1]} look alike in what it learned.`
        : 'Some characters look alike in what it learned.',
    risks.hidesTone ? "An outline or a shadow around the characters isn't part of what it learned." : null,
  ].filter((reason) => reason !== null);
  return reasons.length === 0 ? null : `${reasons.join(' ')} Edit it to see what to do.`;
}

/** A saved reference's mark, and the key it was asked under. */
interface MarkAnswer {
  readonly key: string;
  readonly mark: string | null;
}

/**
 * The mark of the saved reference `reference` of the application `app`, as {@link riskMark} words it for values
 * that need `needs`, or null while it has none: before the setup app answers, when it has nothing to warn about, or
 * when the check fails, which is logged. It asks once the row shows, and again when the reference, as its
 * application's file has it, or what its values need changes; an answer for what it was before is dropped.
 */
export function useSavedRiskMark(app: string, reference: ReferenceDef, needs: readonly string[]): string | null {
  const key = JSON.stringify([app, reference, needs]);
  const [answer, setAnswer] = useState<MarkAnswer>({ key: '', mark: null });
  const latest = useRef({ app, id: reference.id, needs });
  latest.current = { app, id: reference.id, needs };
  useEffect(() => {
    const asked = latest.current;
    return ask(
      () => savedReadingRisks({ app: asked.app, id: asked.id }),
      (risks) => {
        setAnswer({ key, mark: riskMark(risks, asked.needs) });
      },
      (error) => {
        console.error(error);
      },
    );
  }, [key]);
  return answer.key === key ? answer.mark : null;
}
