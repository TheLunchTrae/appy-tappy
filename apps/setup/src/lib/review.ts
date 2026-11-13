import type { RecordedLabel, RecordedReading, RecordedShape } from './bindings.ts';
import { fitsNeeds, mayBeNeeded } from './found.ts';
import { andList, knownCharacters, labelCharacters } from './reference.ts';
// Reviewing a recording: which readings the user keeps, what each shows, and what each adds.
//
// A recording keeps a reading for a shape no reading before it showed, and a reference learns one character for
// each shape of a kept reading from the reading's text. So a reading is worth keeping in review for the shapes no
// other kept reading teaches, and review says which those are.
//
// Review starts a reading left out when the others teach all its shapes. Three rules keep that from hiding anything.
// A start is decided once, so Keep never changes under the user. Texts that give one shape two characters are
// compared across every reading, kept or not, and neither is left out for the other. And a shape that only readings
// left out could teach is named above the readings, with the readings to keep for it.
//
// A recording of the whole window also keeps text that is not the reference's, such as a menu's label in the same
// color and size. A reading whose text shows none of the characters the reference needs starts left out too, and
// says so. The user can still keep it.

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

/** `sizes` as a list of heights, such as "8 px", "8 px and 16 px", or "8 px, 16 px and 24 px". */
function heights(sizes: readonly number[]): string {
  const each = sizes.map((size) => `${size} px`);
  const last = each.at(-1) ?? '';
  return each.length < 2 ? last : `${each.slice(0, -1).join(', ')} and ${last}`;
}

/**
 * The text a recording of the whole window looks for, by its `sizes`, such as "text 8 px tall" or "text of 2 sizes:
 * 8 px and 16 px tall"; null for a recording of one place, which looks for no size.
 */
export function sizesNote(sizes: readonly number[]): string | null {
  if (sizes.length === 0) {
    return null;
  }
  return sizes.length === 1 ? `text ${heights(sizes)} tall` : `text of ${sizes.length} sizes: ${heights(sizes)} tall`;
}

/**
 * Which of the recording's `sizes` of text `reading` is, such as "text 16 px tall"; null when the recording looked
 * for one size or none, so there is nothing to tell apart.
 */
export function readingSize(reading: RecordedReading, sizes: readonly number[]): string | null {
  return sizes.length < 2 || reading.size === null ? null : `text ${reading.size} px tall`;
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

/**
 * Whether `text` is not the reference's, such as a menu's label: it has none of the characters among `needs`, the
 * ones the reference needs, and it is not a needed text misread either, every character of it a letter the text
 * reader often reads in place of a needed digit. So `SOS` may be `505` to a reference that needs digits, and
 * `Score`, whose `c`, `r`, and `e` are no digit's lookalike, is not its text. Never without text, or when the
 * reference does not say what it needs.
 */
export function unrelated(text: string, needs: readonly string[]): boolean {
  const characters = labelCharacters(text);
  return (
    needs.length > 0 &&
    characters.length > 0 &&
    !characters.some((character) => needs.includes(character)) &&
    !fitsNeeds(text, needs)
  );
}

/** Whether review keeps `reading` with a text that names its shapes, so that it teaches them. */
function teaches(reading: RecordedReading, entry: ReviewEntry): boolean {
  return entry.keep && named(reading, entry.text) !== null;
}

/**
 * What each reading adds: the places in it, left to right, of the shapes no other reading teaches, each shape once.
 * `teaching` says which readings teach their shapes, and `settled` which of them stay whatever the others show.
 *
 * Going through the teaching readings in order, one whose every shape another teaching reading still shows is
 * dropped unless it is settled, so the rest still show every shape. Each shape then counts for the first of the rest
 * that shows it. A dropped reading, and one that teaches nothing, adds the shapes none of the rest shows. A reading
 * that adds none is not worth keeping.
 */
export function additions(
  readings: readonly RecordedReading[],
  teaching: readonly boolean[],
  settled: readonly boolean[] = [],
): readonly (readonly number[])[] {
  const standing = readings.map((_, index) => teaching[index] === true);
  readings.forEach((reading, index) => {
    const shownElsewhere = (shape: number): boolean =>
      readings.some((other, at) => at !== index && standing[at] === true && other.shapes.includes(shape));
    if (standing[index] === true && settled[index] !== true && reading.shapes.every(shownElsewhere)) {
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

/**
 * What each reading adds to the ones `all` keeps with a text that names their shapes, as {@link additions} tells.
 * The readings `settled` names stay among them whatever the others show.
 */
export function keptAdditions(
  readings: readonly RecordedReading[],
  all: readonly ReviewEntry[],
  settled: readonly boolean[] = [],
): readonly (readonly number[])[] {
  return additions(
    readings,
    readings.map((reading, index) => {
      const entry = all[index];
      return entry !== undefined && teaches(reading, entry);
    }),
    settled,
  );
}

/**
 * The readings whose text gives a shape another character than another reading's text gives it, each by its index
 * with what to tell the user. Every reading whose text names its shapes takes part, kept or left out, so leaving one
 * of the two out by default cannot hide that a `1` was read as an `l`. Only one of the two texts can be right, and
 * the reference would learn whichever is kept.
 */
export function disagreements(
  readings: readonly RecordedReading[],
  all: readonly ReviewEntry[],
): ReadonlyMap<number, string> {
  const names = readings.map((reading, index) => named(reading, all[index]?.text ?? ''));
  // The character reading `at`'s text gives `shape`, if its text names its shapes and it shows the shape.
  const nameOf = (at: number, shape: number): string | undefined =>
    names[at]?.[readings[at]?.shapes.indexOf(shape) ?? -1];
  const found = new Map<number, string>();
  readings.forEach((reading, index) => {
    const own = names[index] ?? null;
    if (own === null) {
      return;
    }
    for (const [place, shape] of reading.shapes.entries()) {
      const character = own[place];
      const other = readings.findIndex((_, at) => {
        const theirs = nameOf(at, shape);
        return theirs !== undefined && theirs !== character;
      });
      const theirs = nameOf(other, shape);
      if (character !== undefined && theirs !== undefined) {
        const when = clock(readings[other]?.firstMs ?? 0);
        found.set(
          index,
          `Its ${character} has the same shape as the ${theirs} of the reading at ${when}. Fix the text that is wrong.`,
        );
        return;
      }
    }
  });
  return found;
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
 * `decided` with a start for each reading that has none yet and whose text the reader has answered: review decides a
 * reading's start once, so its Keep never changes but by the user's hand. A reading still being read is decided when
 * its text arrives.
 *
 * A reading starts left out when the readings beside it teach all its shapes: the ones the user edited and the ones
 * decided before stay as they are, and the ones decided together are gone through in order. It also starts left out
 * when its text is {@link unrelated} to `needs`, the characters the reference needs. It starts kept all the same when
 * it was kept before, or when its text and another's give one shape two characters, since leaving it out would hide
 * which of the two is wrong; such a reading stays among the ones the others are measured against.
 */
export function decide(
  decided: ReadonlyMap<number, ReviewEntry>,
  readings: readonly RecordedReading[],
  edits: ReadonlyMap<number, ReviewEntry>,
  kept: ReadonlyMap<number, string>,
  needs: readonly string[],
): ReadonlyMap<number, ReviewEntry> {
  const starts = readings.map((reading, index) => startEntry(reading, kept.get(index)));
  const beside = starts.map((start, index) => edits.get(index) ?? decided.get(index) ?? start);
  const clashing = disagreements(readings, beside);
  const adds = keptAdditions(
    readings,
    beside,
    readings.map((_, index) => edits.has(index) || decided.has(index) || clashing.has(index)),
  );
  const next = new Map(decided);
  readings.forEach((reading, index) => {
    const start = starts[index];
    if (start === undefined || decided.has(index) || reading.text.kind === 'pending') {
      return;
    }
    const idle =
      !kept.has(index) && !clashing.has(index) && (adds[index]?.length === 0 || unrelated(start.text, needs));
    next.set(index, idle ? { ...start, keep: false } : start);
  });
  return next.size === decided.size ? decided : next;
}

/**
 * Each reading's entry: the user's edit where there is one, or the start `decided` for it. A reading with neither
 * shows what {@link decide} would start it with now, which review decides once it shows the reading.
 */
export function entries(
  readings: readonly RecordedReading[],
  edits: ReadonlyMap<number, ReviewEntry>,
  kept: ReadonlyMap<number, string>,
  decided: ReadonlyMap<number, ReviewEntry>,
  needs: readonly string[],
): readonly ReviewEntry[] {
  const starts = decide(decided, readings, edits, kept, needs);
  return readings.map(
    (reading, index) => edits.get(index) ?? starts.get(index) ?? startEntry(reading, kept.get(index)),
  );
}

/** What no kept reading teaches, though a reading that is left out could. */
export interface Untaught {
  /** The characters, each once. */
  readonly characters: readonly string[];
  /** The readings left out to keep for them, by index: each teaches a shape none before it in the list does. */
  readonly readings: readonly number[];
}

/**
 * The characters that no reading `all` keeps teaches, though a reading it leaves out has a text that would, and those
 * readings. `adds` is what each reading adds to the kept ones, from {@link keptAdditions} for the same `all`. When
 * the reference says which characters it `needs`, only those count, so text that is not the reference's is not
 * offered.
 */
export function untaught(
  readings: readonly RecordedReading[],
  all: readonly ReviewEntry[],
  adds: readonly (readonly number[])[],
  needs: readonly string[],
): Untaught {
  const characters: string[] = [];
  const offered: number[] = [];
  const covered = new Set<number>();
  readings.forEach((reading, index) => {
    const entry = all[index];
    const names = entry === undefined || entry.keep ? null : named(reading, entry.text);
    const fresh = (adds[index] ?? []).filter(
      (place) =>
        !covered.has(reading.shapes[place] ?? -1) && (needs.length === 0 || mayBeNeeded(names?.[place] ?? '', needs)),
    );
    if (names === null || fresh.length === 0) {
      return;
    }
    offered.push(index);
    characters.push(...fresh.map((place) => names[place] ?? ''));
    for (const shape of reading.shapes) {
      covered.add(shape);
    }
  });
  return { characters: knownCharacters(characters), readings: offered };
}

/** What to tell the user about the `characters` no kept reading teaches, or null without any. */
export function untaughtNote(characters: readonly string[]): string | null {
  return characters.length === 0 ? null : `No kept reading teaches ${andList(characters)}.`;
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

/**
 * What a reading's tile says under it, if anything: why the check cannot learn it, that its text and another's
 * `clash` over a shape, that it is left out as text {@link unrelated} to the characters the reference `needs`, or
 * what the reader made of it.
 */
export function tileNote(
  reading: RecordedReading,
  entry: ReviewEntry,
  problem: string | undefined,
  clash: string | undefined,
  needs: readonly string[],
): string | null {
  if (problem !== undefined && entry.keep) {
    return problem;
  }
  if (clash !== undefined) {
    return clash;
  }
  if (!entry.keep && unrelated(entry.text, needs)) {
    return 'Shows none of the characters the reference needs, so it is left out unless you keep it.';
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
