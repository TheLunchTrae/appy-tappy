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
