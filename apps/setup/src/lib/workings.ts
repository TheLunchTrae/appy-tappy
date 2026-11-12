/** The words and numbers the debug details under a live reading show. */

import type { GlyphView, ReadingColor, StateView } from './bindings.ts';

/**
 * How far a character is from a glyph, in the counts a read decides with: the cells apart, of the cells either sets,
 * and the cells that differ at all, which order glyphs with as many apart.
 */
export function distanceText(glyph: GlyphView): string {
  return `${glyph.apart} of ${glyph.set} apart, ${glyph.differing} differ`;
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
