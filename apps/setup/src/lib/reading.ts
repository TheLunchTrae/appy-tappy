// A reading in words: what a value reads, and what its live reading says of an automation's condition.

import type { Live, Reading, TargetDef, WhenDef } from './bindings.ts';
import type { ValueChoice } from './automation.ts';

/** The most decimals a reading shows when that is enough to tell it from the target. */
const READING_DECIMALS = 4;

/**
 * A number as read, such as `87`, `12.46` or `33.3333`: exact up to four
 * decimals, and rounded beyond that only while the rounded number still falls
 * on the same side of `target` as the reading, so it never contradicts
 * whether a condition holds.
 */
function numberText(value: number, target?: number): string {
  for (let decimals = 0; decimals <= READING_DECIMALS; decimals += 1) {
    const shown = value.toFixed(decimals);
    if (Number(shown) === value) {
      return shown;
    }
  }
  const rounded = value.toFixed(READING_DECIMALS);
  if (target !== undefined && Math.sign(Number(rounded) - target) !== Math.sign(value - target)) {
    return String(value);
  }
  return rounded;
}

/**
 * A reading as the UI shows it, such as `87`, `12.46`, `Yes` or `Paused`, never contradicting a comparison with
 * `target`.
 */
export function readingText(reading: Reading, target?: TargetDef): string {
  switch (reading.kind) {
    case 'number':
      return numberText(reading.value, typeof target === 'number' ? target : undefined);
    case 'flag':
      return reading.value ? 'Yes' : 'No';
    case 'state':
      return reading.value;
  }
}

/** A live reading, or an error that stopped one, or null before the first answer arrives. */
export type LiveState = Live | { readonly kind: 'error'; readonly message: string } | null;

/** A live reading in a sentence, such as `Progress reads 87 now, so the condition is true.` */
export function liveText(live: LiveState, choice: ValueChoice, when: WhenDef | null): string {
  const name = choice.value.name;
  if (live === null) {
    return `Looking for ${choice.appName}…`;
  }
  switch (live.kind) {
    case 'waiting':
      return `Waiting for ${choice.appName} to show ${name}…`;
    case 'error':
    case 'problem':
    case 'lost':
      return live.message;
    case 'read': {
      const reads = `${name} reads ${readingText(live.reading, when?.target)} now`;
      if (live.holds === null) {
        return `${reads}.`;
      }
      return live.holds ? `${reads}, so the condition is true.` : `${reads}, so the condition is not true.`;
    }
  }
}

/**
 * What a screen reader announces of a live reading: only whether the
 * condition holds, or what is wrong, so a moving number is not announced
 * twice a second.
 */
export function liveAnnouncement(live: LiveState): string {
  if (live === null || live.kind === 'waiting') {
    return '';
  }
  if (live.kind === 'read') {
    if (live.holds === null) {
      return '';
    }
    return live.holds ? 'The condition is true.' : 'The condition is not true.';
  }
  return live.message;
}
