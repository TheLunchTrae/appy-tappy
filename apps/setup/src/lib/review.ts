// Reviewing a recording: which readings the user keeps, and what each shows.

import type { RecordedLabel, RecordedReading } from './api.ts';

/** The user's choice for one reading in review. */
export interface ReviewEntry {
  readonly keep: boolean;
  readonly text: string;
}

/** The time `ms` into a recording, as minutes and seconds, such as 1:05. */
export function clock(ms: number): string {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${String(seconds % 60).padStart(2, '0')}`;
}

/** How often a reading showed, such as "seen 3 times". */
export function seen(times: number): string {
  return times === 1 ? 'seen once' : `seen ${times} times`;
}

/**
 * What review starts a reading with: the text it was kept with before, or
 * else what the text reader read, kept when there is text to keep.
 */
export function startEntry(reading: RecordedReading, kept: string | undefined): ReviewEntry {
  if (kept !== undefined) {
    return { keep: true, text: kept };
  }
  const text = reading.text.kind === 'read' ? reading.text.text.trim() : '';
  return { keep: text.length > 0, text };
}

/** Each reading's entry: the user's edit where there is one, or its start. */
export function entries(
  readings: readonly RecordedReading[],
  edits: ReadonlyMap<number, ReviewEntry>,
  kept: ReadonlyMap<number, string>,
): readonly ReviewEntry[] {
  return readings.map((reading, index) => edits.get(index) ?? startEntry(reading, kept.get(index)));
}

/** The readings to keep, with their text, in the order they showed. One without text is left out. */
export function keptLabels(all: readonly ReviewEntry[]): readonly RecordedLabel[] {
  return all.flatMap((entry, reading) => {
    const text = entry.text.trim();
    return entry.keep && text.length > 0 ? [{ reading, text }] : [];
  });
}

/** What a reading's tile says under it, if anything: why the check cannot learn it, or what the reader made of it. */
export function tileNote(reading: RecordedReading, entry: ReviewEntry, problem: string | undefined): string | null {
  if (problem !== undefined && entry.keep) {
    return problem;
  }
  if (entry.keep && entry.text.trim() === '') {
    return 'Type what it shows, or leave it out.';
  }
  switch (reading.text.kind) {
    case 'pending':
      return 'Reading…';
    case 'failed':
      return entry.text.trim() === '' ? reading.text.text : null;
    case 'read':
      return reading.text.text.trim() === '' && entry.text.trim() === ''
        ? 'No text found. Type it, or leave it out.'
        : null;
  }
}

/** The kept readings the check cannot learn, each by its index with why, from `answers` in the order of `recorded`. */
export function flaggedReadings(
  recorded: readonly RecordedLabel[],
  answers: readonly (string | null)[],
): ReadonlyMap<number, string> {
  const found = new Map<number, string>();
  answers.forEach((answer, position) => {
    const label = recorded[position];
    if (answer !== null && label !== undefined) {
      found.set(label.reading, answer);
    }
  });
  return found;
}

/**
 * `edits` after Keep all or Keep none: each reading kept with its text, or left out. A reading still being read
 * gets no edit, so its text and keep come from the reader once it answers.
 */
export function editAll(
  edits: ReadonlyMap<number, ReviewEntry>,
  all: readonly ReviewEntry[],
  readings: readonly RecordedReading[],
  keep: boolean,
): ReadonlyMap<number, ReviewEntry> {
  const next = new Map(edits);
  all.forEach((entry, index) => {
    if (readings[index]?.text.kind !== 'pending' || edits.has(index)) {
      next.set(index, { ...entry, keep: keep && entry.text.trim() !== '' });
    }
  });
  return next;
}

/**
 * Why Keep waits, or null when it can keep `kept`: a flagged reading is kept, or the readings were not checked yet
 * for what review shows now.
 */
export function keepBlocked(
  kept: readonly RecordedLabel[],
  flagged: ReadonlyMap<number, string>,
  checked: boolean,
): string | null {
  if (kept.some((label) => flagged.has(label.reading))) {
    return 'Fix or leave out the flagged readings to keep them.';
  }
  return checked ? null : 'Checking the readings…';
}
