// Text found like a reference's first sample: which finds to add, as what, and what stops them.

import type { FoundView, Label } from './bindings.ts';
import type { Rect } from './region.ts';

/** A find the user added to the reference: text at its own region of a screenshot. */
export interface FoundLabel {
  readonly screenshot: number;
  readonly region: Rect;
  readonly text: string;
}

/** The user's choice for one find: whether to add it, its text, and whether the user typed that text. */
export interface FoundEntry {
  readonly keep: boolean;
  readonly text: string;
  readonly typed: boolean;
}

/** One find as the panel lists it. */
export interface FoundRow {
  readonly index: number;
  readonly region: Rect;
  readonly keep: boolean;
  readonly text: string;
  /** Whether the user typed its text, rather than keeping the text reader's guess. */
  readonly typed: boolean;
  /** Whether the text reader has not read it yet and the user typed nothing. */
  readonly reading: boolean;
  /** Why it cannot be added as it is, found without asking the reference, or null. */
  readonly flag: string | null;
}

/** The first letter in `text`, or null. */
function firstLetter(text: string): string | null {
  return /\p{L}/u.exec(text)?.[0] ?? null;
}

/**
 * The finds of `view` with the user's `edits`. A find starts kept, with the text reader's guess. A kept one is flagged
 * when it has no text, or, while the reference has `learned` no letter, when the reader's guess has one: the reader
 * often takes a 0 for an O. Text the user typed is theirs, letters and all.
 */
export function foundRows(
  view: FoundView,
  edits: ReadonlyMap<number, FoundEntry>,
  learned: readonly string[],
): readonly FoundRow[] {
  const letters = learned.some((text) => firstLetter(text) !== null);
  return view.finds.map((find, index) => {
    const edit = edits.get(index);
    const guess = find.text.kind === 'read' ? find.text.text : '';
    const keep = edit?.keep ?? true;
    const text = edit?.text ?? guess;
    const typed = edit?.typed ?? false;
    const reading = edit === undefined && find.text.kind === 'pending';
    const letter = letters || typed ? null : firstLetter(text);
    let flag: string | null = null;
    if (keep && !reading) {
      if (text.trim() === '') {
        flag =
          find.text.kind === 'failed' && edit === undefined ? find.text.text : 'Type what this shows, or leave it out.';
      } else if (letter !== null) {
        flag = `${letter} is a letter, and the reference has learned none. Type what this shows.`;
      }
    }
    return { index, region: find.region, keep, text, typed, reading, flag };
  });
}

/** The kept rows whose text the reference is asked about: read, typed, and not flagged already. */
export function askable(rows: readonly FoundRow[]): readonly FoundRow[] {
  return rows.filter((row) => row.keep && !row.reading && row.flag === null);
}

/** `rows` as labels of screenshot `screenshot`, each at its own region. */
export function asLabels(screenshot: number, rows: readonly FoundRow[]): readonly Label[] {
  return rows.map((row) => ({ screenshot, region: row.region, text: row.text.trim() }));
}

/** Why each of `asked` cannot be learned, by its index, from `answers`, the reference's answers in the same order. */
export function flaggedFinds(
  asked: readonly FoundRow[],
  answers: readonly (string | null)[],
): ReadonlyMap<number, string> {
  const flagged = new Map<number, string>();
  asked.forEach((row, position) => {
    const answer = answers[position];
    if (answer !== null && answer !== undefined) {
      flagged.set(row.index, answer);
    }
  });
  return flagged;
}

/** Each row's flag: its own, or else why the reference cannot learn it. */
export function rowFlags(
  rows: readonly FoundRow[],
  learnProblems: ReadonlyMap<number, string>,
): ReadonlyMap<number, string> {
  const flags = new Map<number, string>();
  for (const row of rows) {
    const flag = row.keep ? (row.flag ?? learnProblems.get(row.index) ?? null) : null;
    if (flag !== null) {
      flags.set(row.index, flag);
    }
  }
  return flags;
}

/** Why the kept rows cannot be added yet, or null when they can; `checked` says the reference has answered for them. */
export function addBlocked(
  rows: readonly FoundRow[],
  flags: ReadonlyMap<number, string>,
  checked: boolean,
): string | null {
  const kept = rows.filter((row) => row.keep);
  if (kept.length === 0) {
    return 'Keep at least one to add it.';
  }
  if (kept.some((row) => row.reading)) {
    return 'Reading the text…';
  }
  const flagged = kept.filter((row) => flags.has(row.index)).length;
  if (flagged > 0) {
    return flagged === 1
      ? 'Fix or leave out the flagged one to add them.'
      : 'Fix or leave out the flagged ones to add them.';
  }
  return checked ? null : 'Checking the text…';
}

/** The kept rows as labels the draft adds, on screenshot `screenshot`. */
export function keptFound(screenshot: number, rows: readonly FoundRow[]): readonly FoundLabel[] {
  return rows
    .filter((row) => row.keep && row.text.trim() !== '')
    .map((row) => ({ screenshot, region: row.region, text: row.text.trim() }));
}
