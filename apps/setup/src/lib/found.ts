// Text found like a box a reference learns from: which finds to add, as what, and what stops them.

import type { ColorSpec, FoundView, Label } from './bindings.ts';
import type { NewBox } from './boxes.ts';
import type { Rect } from './region.ts';

/** The user's choice for one find: whether to add it, and the text the user typed, which `typed` says is theirs. */
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
  /** Whether the text reader could not read it and the user typed nothing. */
  readonly failed: boolean;
  /** The characters' color and tolerance the text reader last read it with. */
  readonly readWith: { readonly ink: ColorSpec; readonly tolerance: number };
  /** Why it cannot be added as it is, found without asking the reference, or null. */
  readonly flag: string | null;
  /**
   * Whether it may show only characters the reference needs, so the panel lists it. Text being read, and text the
   * user typed, is listed.
   */
  readonly fits: boolean;
}

/**
 * The digit each letter or mark the text reader often reads in place of one stands for. A reference that needs only
 * digits still lists a find read as 12O5, whose O may be a 0.
 */
const LOOKALIKES: Readonly<Record<string, string>> = {
  O: '0',
  o: '0',
  D: '0',
  Q: '0',
  I: '1',
  l: '1',
  i: '1',
  '|': '1',
  Z: '2',
  z: '2',
  S: '5',
  s: '5',
  G: '6',
  b: '6',
  T: '7',
  B: '8',
  g: '9',
  q: '9',
};

/** Whether `character` may be one of `needs`: one of them, or a lookalike of a digit among them. */
export function mayBeNeeded(character: string, needs: readonly string[]): boolean {
  const digit = LOOKALIKES[character];
  return needs.includes(character) || (digit !== undefined && needs.includes(digit));
}

/** Whether `text` may show only characters among `needs`, a character or its lookalike digit; any does when it is empty. */
export function fitsNeeds(text: string, needs: readonly string[]): boolean {
  return (
    needs.length === 0 || Array.from(text.replaceAll(/\s/gu, '')).every((character) => mayBeNeeded(character, needs))
  );
}

/** The first letter in `text`, or null. */
function firstLetter(text: string): string | null {
  return /\p{L}/u.exec(text)?.[0] ?? null;
}

/**
 * The finds of `view` with the user's `edits`. A find starts left out, with the text reader's guess, and is listed
 * when that guess may show only characters among `needs`, the ones the reference needs. A kept one is flagged when it
 * has no text, or, while the reference has `learned` no letter, when the reader's guess has one: the reader often
 * takes a 0 for an O. Text the user typed is theirs, letters and all.
 */
export function foundRows(
  view: FoundView,
  edits: ReadonlyMap<number, FoundEntry>,
  learned: readonly string[],
  needs: readonly string[],
): readonly FoundRow[] {
  const letters = learned.some((text) => firstLetter(text) !== null);
  return view.finds.map((find, index) => {
    const edit = edits.get(index);
    const guess = find.text.kind === 'read' ? find.text.text : '';
    const keep = edit?.keep ?? false;
    const typed = edit?.typed ?? false;
    // Text the user did not type is the reader's guess, which may arrive after the find was ticked.
    const text = typed ? (edit?.text ?? '') : guess;
    const reading = !typed && find.text.kind === 'pending';
    const letter = letters || typed ? null : firstLetter(text);
    let flag: string | null = null;
    if (keep && !reading) {
      if (text.trim() === '') {
        flag = find.text.kind === 'failed' && !typed ? find.text.text : 'Type what this shows, or leave it out.';
      } else if (letter !== null) {
        flag = `${letter} is a letter, and the reference has learned none. Type what this shows.`;
      }
    }
    const failed = !typed && find.text.kind === 'failed';
    const fits = reading || typed || fitsNeeds(guess, needs);
    return {
      index,
      region: find.region,
      keep: keep && fits,
      text,
      typed,
      reading,
      failed,
      readWith: { ink: find.ink, tolerance: find.tolerance },
      flag: fits ? flag : null,
      fits,
    };
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
    return 'Tick at least one to add it.';
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

/** The kept rows as the boxes the draft adds, on screenshot `screenshot`. */
export function keptFound(screenshot: number, rows: readonly FoundRow[]): readonly NewBox[] {
  return rows
    .filter((row) => row.keep && row.text.trim() !== '')
    .map((row) => ({ screenshot, region: row.region, text: row.text.trim() }));
}

/** The rows of `rows` the panel lists and the screenshot outlines: those that may show only characters the reference needs. */
export function listed(rows: readonly FoundRow[]): readonly FoundRow[] {
  return rows.filter((row) => row.fits);
}

/** The edits that keep every listed row of `rows`, or with `keep` false leave each out, each with its own text. */
export function keepAll(rows: readonly FoundRow[], keep: boolean): ReadonlyMap<number, FoundEntry> {
  return new Map(listed(rows).map((row) => [row.index, { keep, text: row.text, typed: row.typed }] as const));
}

/** Whether the listed rows of `rows` are all kept, some, or none, for the box that keeps or leaves out them all. */
export function keptState(rows: readonly FoundRow[]): 'all' | 'some' | 'none' {
  const shown = listed(rows);
  const kept = shown.filter((row) => row.keep).length;
  return kept === 0 ? 'none' : kept === shown.length ? 'all' : 'some';
}

/** The edits that leave out every kept row of `rows` that `flags` flags, each with its own text. */
export function withoutFlagged(
  rows: readonly FoundRow[],
  flags: ReadonlyMap<number, string>,
): ReadonlyMap<number, FoundEntry> {
  return new Map(
    rows
      .filter((row) => row.keep && flags.has(row.index))
      .map((row) => [row.index, { keep: false, text: row.text, typed: row.typed }] as const),
  );
}

/** Why Reread waits while every find it would read again was last read with the color and tolerance set now. */
export const REREAD_SAME = "Change the characters' color or tolerance to read them differently.";

/**
 * What Reread would do with `rows`, or null when it has nothing to offer. It is for the listed finds the text reader
 * could not read and the ones `flags` flags whose text is its guess; text the user typed is theirs. Of those it
 * reads again, by index, each one whose read failed, since a failure may pass, and each one last read with another
 * characters' color or tolerance than `ink` and `tolerance`, the ones set now. The same color and tolerance read the
 * same text, so when it would read none again, `blocked` says what to change.
 */
export function rereadOffer(
  rows: readonly FoundRow[],
  flags: ReadonlyMap<number, string>,
  ink: ColorSpec | null,
  tolerance: number,
): { readonly indexes: readonly number[]; readonly blocked: string | null } | null {
  const offered = listed(rows).filter((row) => !row.typed && (row.failed || flags.has(row.index)));
  if (offered.length === 0 || ink === null) {
    return null;
  }
  const indexes = offered
    .filter(
      (row) =>
        row.failed ||
        row.readWith.tolerance !== tolerance ||
        row.readWith.ink.red !== ink.red ||
        row.readWith.ink.green !== ink.green ||
        row.readWith.ink.blue !== ink.blue,
    )
    .map((row) => row.index);
  return { indexes, blocked: indexes.length === 0 ? REREAD_SAME : null };
}
