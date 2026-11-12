import type { RecordedLabel, RecordedReading, RecordedShape } from './bindings.ts';
import { labelCharacters } from './reference.ts';
// Reviewing a recording: which readings the user keeps, what each shows, and what each adds.
//
// A recording keeps a reading for a shape no reading before it showed, and a reference learns one character for
// each shape of a kept reading from the reading's text. So a reading is worth keeping in review for the shapes no
// other kept reading teaches, and review says which those are.

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
 * How often a shape showed, such as "seen 214 times in 37 readings". These are counts of what the recording saw. They
 * say nothing of whether the shape's character is right, which only its text decides.
 */
export function sighted(shape: RecordedShape): string {
  if (shape.frames === 1) {
    return 'seen once';
  }
  return `seen ${shape.frames} times in ${shape.readings === 1 ? '1 reading' : `${shape.readings} readings`}`;
}

/**
 * The character of each of `reading`'s shapes, left to right, by `text`: null unless the text has one character for
 * each shape, since a reference can learn the reading only then.
 */
export function named(reading: RecordedReading, text: string): readonly string[] | null {
  const characters = labelCharacters(text);
  return characters.length > 0 && characters.length === reading.shapes.length ? characters : null;
}

/** Whether review keeps `reading` with a text that names its shapes, so that it teaches them. */
function teaches(reading: RecordedReading, entry: ReviewEntry): boolean {
  return entry.keep && named(reading, entry.text) !== null;
}

/**
 * What each reading adds: the places in it, left to right, of the shapes no other reading teaches, each shape once.
 * `teaching` says which readings teach their shapes.
 *
 * Going through the teaching readings in order, one whose every shape another teaching reading still shows is
 * dropped, so the rest still show every shape. Each shape then counts for the first of the rest that shows it. A
 * dropped reading, and one that teaches nothing, adds the shapes none of the rest shows. A reading that adds none is
 * not worth keeping.
 */
export function additions(
  readings: readonly RecordedReading[],
  teaching: readonly boolean[],
): readonly (readonly number[])[] {
  const standing = readings.map((_, index) => teaching[index] === true);
  readings.forEach((reading, index) => {
    const shownElsewhere = (shape: number): boolean =>
      readings.some((other, at) => at !== index && standing[at] === true && other.shapes.includes(shape));
    if (standing[index] === true && reading.shapes.every(shownElsewhere)) {
      standing[index] = false;
    }
  });
  const taught = new Set<number>();
  const firsts = (reading: RecordedReading, known: ReadonlySet<number>): readonly number[] =>
    reading.shapes.flatMap((shape, place) =>
      known.has(shape) || reading.shapes.indexOf(shape) !== place ? [] : [place],
    );
  const added = readings.map((reading, index) => {
    if (standing[index] !== true) {
      return null;
    }
    const places = firsts(reading, taught);
    for (const shape of reading.shapes) {
      taught.add(shape);
    }
    return places;
  });
  return readings.map((reading, index) => added[index] ?? firsts(reading, taught));
}

/**
 * The order review shows the readings in, as their indexes: the ones that show a shape no other reading is needed
 * for first, then the ones the others make up for, each group in the order it showed. It follows the shapes alone,
 * so it stays put while the user types and keeps.
 */
export function reviewOrder(readings: readonly RecordedReading[]): readonly number[] {
  const adds = additions(
    readings,
    readings.map(() => true),
  );
  const indexes = readings.map((_, index) => index);
  return [
    ...indexes.filter((index) => (adds[index]?.length ?? 0) > 0),
    ...indexes.filter((index) => (adds[index]?.length ?? 0) === 0),
  ];
}

/** What each reading adds to the ones `all` keeps with a text that names their shapes, as {@link additions} tells. */
export function keptAdditions(
  readings: readonly RecordedReading[],
  all: readonly ReviewEntry[],
): readonly (readonly number[])[] {
  return additions(
    readings,
    readings.map((reading, index) => {
      const entry = all[index];
      return entry !== undefined && teaches(reading, entry);
    }),
  );
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

/**
 * Each reading's entry: the user's edit where there is one, or its start. A reading that would add nothing to the
 * others' starts begins left out, unless it was kept before.
 */
export function entries(
  readings: readonly RecordedReading[],
  edits: ReadonlyMap<number, ReviewEntry>,
  kept: ReadonlyMap<number, string>,
): readonly ReviewEntry[] {
  const starts = readings.map((reading, index) => startEntry(reading, kept.get(index)));
  const adds = keptAdditions(readings, starts);
  return starts.map((start, index) => {
    const idle = !kept.has(index) && adds[index]?.length === 0;
    return edits.get(index) ?? (idle ? { ...start, keep: false } : start);
  });
}

/**
 * The readings the user has checked: kept, and either kept before with their text or edited in review. The rest of
 * what review keeps is the text reader's guess until the user keeps it.
 */
export function checkedReadings(
  all: readonly ReviewEntry[],
  edits: ReadonlyMap<number, ReviewEntry>,
  kept: ReadonlyMap<number, string>,
): ReadonlySet<number> {
  return new Set(all.flatMap((entry, index) => (entry.keep && (edits.has(index) || kept.has(index)) ? [index] : [])));
}

/** One shape a reading adds: its character by the reading's text, or null when the text does not name it. */
export interface Added {
  readonly character: string | null;
  readonly shape: RecordedShape;
}

/** The shapes `reading` adds, at `places` in it, each with its character by `text` and how often it showed. */
export function addedShapes(
  reading: RecordedReading,
  text: string,
  places: readonly number[],
  shapes: readonly RecordedShape[],
): readonly Added[] {
  const characters = named(reading, text);
  return places.flatMap((place) => {
    const shape = shapes[reading.shapes[place] ?? -1];
    return shape === undefined ? [] : [{ character: characters?.[place] ?? null, shape }];
  });
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
