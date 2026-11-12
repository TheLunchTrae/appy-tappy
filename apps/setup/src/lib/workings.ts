/** The words and numbers the debug details under a live reading show. */

import type { GlyphView, ReadingColor, StateView } from './bindings.ts';

/**
 * How close a character is to a glyph, as a whole percent of the cells either inks that agree. It rounds down, so a
 * character that does not match never shows the 75% a match needs.
 */
export function closeness(glyph: GlyphView): number {
  if (glyph.inked === 0) {
    return 100;
  }
  return Math.floor((100 * (glyph.inked - glyph.differing)) / glyph.inked);
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

/**
 * A state's average difference per channel, to one decimal, or null when nothing was compared. It rounds up, so a
 * state outside its tolerance never shows as within it.
 */
export function averageDifference(state: StateView): number | null {
  return state.channels === 0 ? null : Math.ceil((10 * state.total) / state.channels) / 10;
}
