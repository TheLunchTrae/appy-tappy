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
 * Whether `shown` already shows what `next` would. It compares every field of a `Reading`, its kind and its value:
 * a field added to a reading is compared here too, or a read that differs only in it would never show.
 */
function sameRead(shown: LiveRead, next: LiveRead): boolean {
  if (shown.problem !== next.problem) {
    return false;
  }
  const [was, now] = [shown.reading, next.reading];
  return was === null || now === null ? was === now : was.kind === now.kind && was.value === now.value;
}

/**
 * What the screen shows once a read answers: the answer, when the check that was in force as the read was `asked` is
 * the one in force `now`, and otherwise `shown` as it was. An answer asked while no check was in force is dropped
 * too, since the Rust side read it with the check before. An answer that says what `shown` says already is `shown`
 * itself: the window is read several times a second, and what reads the same shows the same without a new render.
 */
export function afterRead(shown: LiveRead, asked: InForce, now: InForce, answer: ReadAnswer): LiveRead {
  if (asked === null || asked !== now) {
    return shown;
  }
  const next: LiveRead = answer.ok
    ? { reading: answer.reading, problem: null }
    : { reading: null, problem: answer.problem };
  return sameRead(shown, next) ? shown : next;
}

/** Nothing read: no frame arrived yet, and nothing is wrong. */
export const NOTHING_READ: LiveRead = { reading: null, problem: null };

/**
 * What the live window reads now, kept outside the capture screen's own state, so a new reading renders only the
 * small parts that show it and not the whole screen, with a recording's review under it.
 */
export interface LiveReads {
  readonly shown: () => LiveRead;
  /** Hears each time what shows changes, until the function it returns is called. */
  readonly subscribe: (listener: () => void) => () => void;
  /** Changes what shows; a change that answers what shows already tells no one. */
  readonly change: (change: (shown: LiveRead) => LiveRead) => void;
}

export function liveReads(): LiveReads {
  let shown = NOTHING_READ;
  const listeners = new Set<() => void>();
  return {
    shown: () => shown,
    subscribe: (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    change: (change) => {
      const next = change(shown);
      if (next === shown) {
        return;
      }
      shown = next;
      for (const listener of listeners) {
        listener();
      }
    },
  };
}
