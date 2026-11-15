// The tolerance of a reference that finds its characters by contrast, which the setup app chooses from the samples
// the reference learns from.

import { useEffect, useRef, useState } from 'react';

import type { CharactersSpec } from './bindings.ts';
import { contrastTolerance } from './api.ts';
import { ask } from './request.ts';

/** How long the samples must rest before a fit is asked for, so a label asks once it is typed, not at each key. */
const FIT_DELAY_MS = 250;

/**
 * A name for the fit `spec` asks for, which stays while nothing the fit depends on changes, or null when it asks
 * for none: no reference is taught, or the reference has a text color, whose tolerance is the user's.
 */
export function fitKey(spec: CharactersSpec | null): string | null {
  return spec === null || spec.ink !== undefined ? null : JSON.stringify(spec);
}

/**
 * The tolerance to set once the fit asked for under `asked` answers `fitted`, or null to leave it. An answer for
 * another fit than `wanted`, the one the reference asks for now, is dropped: a sample, the region, or the tolerance
 * changed since, and that change asked again under its own name.
 */
export function fitArrived(asked: string, wanted: string | null, tolerance: number, fitted: number): number | null {
  return asked === wanted && fitted !== tolerance ? fitted : null;
}

/**
 * Whether the fit asked for under `asked`, answering `fitted` for the tolerance `tolerance`, leaves the reference that
 * asks under `wanted` as it is: it still asks for that fit, and every sample reads with the tolerance it has.
 */
export function fitSettles(asked: string, wanted: string | null, tolerance: number, fitted: number): boolean {
  return asked === wanted && fitted === tolerance;
}

/**
 * Keeps the tolerance of the reference `spec` teaches at one its samples read with by contrast, through
 * `setTolerance`, and answers whether it is there: the fit answered for the spec as it is, or none is asked for. It
 * asks again once a sample, the region, or the tolerance changed and then rested. The setup app answers with the
 * tolerance asked about while every sample reads with it, so a fit settles after one more ask. A fit that fails
 * leaves the tolerance alone and never settles, and the check of the characters says what is wrong with the samples.
 */
export function useContrastTolerance(spec: CharactersSpec | null, setTolerance: (tolerance: number) => void): boolean {
  const key = fitKey(spec);
  const latest = useRef({ spec, key, setTolerance });
  latest.current = { spec, key, setTolerance };
  // The key of the last fit that left the tolerance as it was.
  const [settled, setSettled] = useState<string | null>(null);
  useEffect(() => {
    const asked = latest.current;
    if (asked.spec === null || asked.key === null) {
      return undefined;
    }
    const { key: askedKey, spec: askedSpec } = asked;
    const cancel = ask(
      () => contrastTolerance(askedSpec),
      (fitted) => {
        const now = latest.current;
        if (fitSettles(askedKey, now.key, askedSpec.tolerance, fitted)) {
          setSettled(askedKey);
        }
        const next = fitArrived(askedKey, now.key, askedSpec.tolerance, fitted);
        if (next !== null) {
          now.setTolerance(next);
        }
      },
      (error) => {
        console.error(error);
      },
      FIT_DELAY_MS,
    );
    return () => {
      cancel();
      // A reference saved again keeps its ID and so its key, so a fit for the samples it had before never counts.
      setSettled(null);
    };
  }, [key]);
  return key === null || settled === key;
}
