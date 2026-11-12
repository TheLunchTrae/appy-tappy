/** The words and numbers the debug details under a live reading show. */

import type { GlyphView, ReadingColor, StateView } from './bindings.ts';

/** How close a character is to a glyph, as a whole percent of the cells either inks that agree. */
export function closeness(glyph: GlyphView): number {
  if (glyph.inked === 0) {
    return 100;
  }
  return Math.round(100 * (1 - glyph.differing / glyph.inked));
}

/** A share of `whole` as a whole percent, or null when there is nothing to share. */
export function percentOf(part: number, whole: number): number | null {
  return whole === 0 ? null : Math.round((100 * part) / whole);
}

/** A color as CSS reads it. */
export function cssColor(color: ReadingColor): string {
  return `rgb(${color.red}, ${color.green}, ${color.blue})`;
}

/** A color's channels, as `R 200 G 40 B 40`. */
export function channelsText(color: ReadingColor): string {
  return `R ${color.red} G ${color.green} B ${color.blue}`;
}

/** A state's average difference per channel, to one decimal, or null when nothing was compared. */
export function averageDifference(state: StateView): number | null {
  return state.channels === 0 ? null : Math.round((10 * state.total) / state.channels) / 10;
}

/**
 * The state a read takes: the closest within its tolerance, the first one on a tie, or null when none is within.
 */
export function readState(states: readonly StateView[]): string | null {
  let best: { readonly name: string; readonly average: number } | null = null;
  for (const state of states) {
    const average = averageDifference(state);
    if (state.within && average !== null && (best === null || average < best.average)) {
      best = { name: state.name, average };
    }
  }
  return best?.name ?? null;
}
