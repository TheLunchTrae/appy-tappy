// What may keep a reference from reading as its samples do, which the reference editor warns about: characters it
// can't tell apart, because the shapes it learned for them match each other.

import { useEffect, useRef, useState } from 'react';

import type { CharactersSpec, ReadingRisks, ReferenceDef } from './bindings.ts';
import { errorText, readingRisks, savedReadingRisks } from './api.ts';
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
 * What the reference editor says about `pairs`, the characters a reference can't tell apart, or null for none. A
 * wrong label is the usual cause, so it says to check the text first.
 */
export function lookalikeWarning(pairs: readonly Pair[]): string | null {
  const [first, ...rest] = pairs;
  if (first === undefined) {
    return null;
  }
  const which =
    rest.length === 0
      ? `${first[0]} and ${first[1]} have the same shape in what it learned, so it can't tell one from the other.`
      : `Some characters have the same shape in what it learned, so it can't tell one of each pair from the other: ${pairs.map(([a, b]) => `${a} and ${b}`).join('; ')}.`;
  return `${which} Check that the text typed for each picture is what it shows. A value that needs both won't read them.`;
}

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
const NO_RISKS: ShownRisks = { lookalikes: [], problem: null };

/** What the reference has been answered before it asks. */
const NO_ANSWER: Answer = { key: '', risks: NO_RISKS };

/**
 * What the editor shows of the answer `risks`. Null is a reference that can't learn one of its lessons: it shows no
 * risks and no problem, since the reading says which lesson and why.
 */
export function risksAnswered(risks: ReadingRisks | null): ShownRisks {
  return risks === null ? NO_RISKS : { ...risks, problem: null };
}

/** What the editor says when the check for `error` failed: no risks, which is not the same as none found. */
export function risksFailed(error: unknown): ShownRisks {
  return { ...NO_RISKS, problem: `Can't check for characters that look alike. ${errorText(error)}` };
}

/**
 * The risks to show while the reference asks under `wanted`: those of `answer` when it was asked under that key, and
 * none otherwise, so an answer or a failure for samples or a region since changed is dropped.
 */
export function risksArrived(answer: Answer, wanted: string | null): ShownRisks {
  return answer.key === wanted ? answer.risks : NO_RISKS;
}

/**
 * What may keep the reference `spec` teaches from reading as its samples do, as the setup app answers for that spec,
 * or nothing while `spec` is null or the answer for it has not arrived. It asks again once a sample or the region
 * changed and then rested. A request that fails says so in place of the risks, until the next one answers; a
 * reference that can't be taught is not a failure of the request.
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
        setAnswer({ key, risks: risksAnswered(risks) });
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
  if (first === undefined) {
    return null;
  }
  const reason =
    pairs.length === 1
      ? `${first[0]} and ${first[1]} have the same shape in what it learned.`
      : 'Some characters have the same shape in what it learned.';
  return `${reason} Edit it to see what to do.`;
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
