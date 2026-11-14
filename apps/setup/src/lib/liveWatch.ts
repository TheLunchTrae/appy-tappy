// The automation editor's watch of one saved value: the owner's capture that reads it, and what follows when the
// watch is lost, replaced, or refused.

import type { Live } from './bindings.ts';
import type { LiveState } from './reading.ts';

/** How long the live reading waits before it looks for the window again, as the runtime does. */
export const RETRY_MS = 1000;

/** The editor's watch of a value. */
export interface LiveWatch {
  /** The owner's number for the watch that runs, or null while none does. */
  readonly watch: number | null;
  /** The time before which no new watch starts. */
  readonly retryAt: number;
  /**
   * Why the owner last refused a reading of the watch, when nothing was read since. Another capture taking the
   * owner's one capture slot refuses once, and watching again then reads; a second refusal in a row is a failure.
   */
  readonly refused: string | null;
}

export const NOT_WATCHING: LiveWatch = { watch: null, retryAt: 0, refused: null };

/** What the watch hears of. */
export type LiveWatchEvent =
  /** The owner started watch `watch`. */
  | { readonly kind: 'started'; readonly watch: number }
  /** The owner could not start a watch, at time `now`. */
  | { readonly kind: 'unstarted'; readonly message: string; readonly now: number }
  /** The watch answered a reading, at time `now`. */
  | { readonly kind: 'read'; readonly live: Live; readonly now: number }
  /** The owner refused to read the watch, which it does once another capture replaced it, at time `now`. */
  | { readonly kind: 'refused'; readonly message: string; readonly now: number };

/**
 * The watch after `event`, and what the editor shows for it, or `undefined` to keep what it shows. A lost watch and
 * one that cannot start wait `RETRY_MS` before the next. A watch another capture replaced starts again at once and
 * says nothing; when the one that follows is refused as well, the refusal is a failure, which shows and waits.
 */
export function liveWatchStep(
  state: LiveWatch,
  event: LiveWatchEvent,
): { readonly state: LiveWatch; readonly shows?: LiveState } {
  switch (event.kind) {
    case 'started':
      return { state: { ...state, watch: event.watch } };
    case 'unstarted':
      return {
        state: { ...state, watch: null, retryAt: event.now + RETRY_MS },
        shows: { kind: 'error', message: event.message },
      };
    case 'read':
      return {
        state:
          event.live.kind === 'lost'
            ? { watch: null, retryAt: event.now + RETRY_MS, refused: null }
            : { ...state, refused: null },
        shows: event.live,
      };
    case 'refused':
      return state.refused === null
        ? { state: { ...state, watch: null, refused: event.message } }
        : {
            state: { watch: null, retryAt: event.now + RETRY_MS, refused: event.message },
            shows: { kind: 'error', message: event.message },
          };
  }
}
