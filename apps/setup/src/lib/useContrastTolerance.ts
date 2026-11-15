// The tolerance of a reference that finds its characters by contrast, which the setup app chooses from the samples
// the reference learns from.

import { useEffect, useRef } from 'react';

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
 * Keeps the tolerance of the reference `spec` teaches at one its samples read with by contrast, through
 * `setTolerance`. It asks again once a sample, the region, or the tolerance changed and then rested. The setup app
 * answers with the tolerance asked about while every sample reads with it, so a fit settles after one more ask. A
 * fit that fails leaves the tolerance alone, and the check of the characters says what is wrong with the samples.
 */
export function useContrastTolerance(spec: CharactersSpec | null, setTolerance: (tolerance: number) => void): void {
  const key = fitKey(spec);
  const latest = useRef({ spec, key, setTolerance });
  latest.current = { spec, key, setTolerance };
  useEffect(() => {
    const asked = latest.current;
    if (asked.spec === null || asked.key === null) {
      return undefined;
    }
    const { key: askedKey, spec: askedSpec } = asked;
    return ask(
      () => contrastTolerance(askedSpec),
      (fitted) => {
        const now = latest.current;
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
  }, [key]);
}
