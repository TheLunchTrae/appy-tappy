// Which answers of the live reading belong to the check now in force.
//
// The capture screen sets a check on the Rust side a moment after the draft last changed, and reads the live window
// with it. A read can answer after the draft changed again, and can even start in that moment, while the Rust side
// still reads with the check before. Each check tried gets a number, and a number is in force only from when the Rust
// side took its check until the draft next changes.

import type { Reading } from './bindings.ts';

/** The number of the check the Rust side reads with, or null from a change of the check until it took the new one. */
export type InForce = number | null;

/** What the live window read, as the capture screen shows it. */
export interface LiveRead {
  /** What the newest frame read, or null before a frame arrived or while it cannot be read. */
  readonly reading: Reading | null;
  /** Why the newest frame could not be read, or null while it can. */
  readonly problem: string | null;
}

/** What a read answered: the reading, which is null before a frame arrived, or why the frame could not be read. */
export type ReadAnswer =
  | { readonly ok: true; readonly reading: Reading | null }
  | { readonly ok: false; readonly problem: string };

/**
 * What the screen shows once a read answers: the answer, when the check that was in force as the read was `asked` is
 * the one in force `now`, and otherwise `shown` as it was. An answer asked while no check was in force is dropped
 * too, since the Rust side read it with the check before.
 */
export function afterRead(shown: LiveRead, asked: InForce, now: InForce, answer: ReadAnswer): LiveRead {
  if (asked === null || asked !== now) {
    return shown;
  }
  return answer.ok ? { reading: answer.reading, problem: null } : { reading: null, problem: answer.problem };
}
