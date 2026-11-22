import type { AppFile, ReferenceDef, ValueDef } from './bindings.ts';
// How references read on the capture screen and the References page.

/** The digits `0` to `9` before other characters, then by code point, so the order is the same on every machine. */
function order(left: string, right: string): number {
  const digits = Number(!/\d/u.test(left)) - Number(!/\d/u.test(right));
  if (digits !== 0) {
    return digits;
  }
  return (left.codePointAt(0) ?? 0) - (right.codePointAt(0) ?? 0);
}

/**
 * The characters a label teaches, in order: every code point of `text` but white space, which is how the Rust side
 * counts them when a reference learns from the label.
 */
export function labelCharacters(text: string): readonly string[] {
  const characters = [];
  for (const character of text) {
    if (!/\p{White_Space}/u.test(character)) {
      characters.push(character);
    }
  }
  return characters;
}

/** Every character `texts` teach, once each, digits first. */
export function knownCharacters(texts: Iterable<string>): readonly string[] {
  const found = new Set<string>();
  for (const text of texts) {
    for (const character of labelCharacters(text)) {
      found.add(character);
    }
  }
  return [...found].toSorted(order);
}

/**
 * The characters `reference` reads: those it learned from its saved pictures, of the characters it is made to read
 * when it says, as the runtime reads with it.
 */
export function referenceCharacters(reference: ReferenceDef): readonly string[] {
  const learned = knownCharacters(reference.samples.map((sample) => sample.text));
  if (reference.characters === undefined) {
    return learned;
  }
  const reads = labelCharacters(reference.characters);
  return learned.filter((character) => reads.includes(character));
}

/**
 * The characters of `needs`, the ones a value needs, that `reference` does not read, so the value cannot read them
 * with it until the reference reads them too. None when it reads every character it learns.
 */
export function unreadBy(reference: ReferenceDef, needs: readonly string[]): readonly string[] {
  if (reference.characters === undefined) {
    return [];
  }
  const reads = labelCharacters(reference.characters);
  return needs.filter((character) => !reads.includes(character));
}

/** `file`'s values that read with the reference `id`, in their check or in a fix. */
function readingValues(file: AppFile, id: string): readonly ValueDef[] {
  return file.values.filter(
    ({ screen }) =>
      (screen.check.kind === 'number' && screen.check.reference === id) ||
      screen.fixes.some((fix) => fix.reference === id),
  );
}

/** The names of `file`'s values that read with the reference `id`, in their check or in a fix. */
export function readers(file: AppFile, id: string): readonly string[] {
  return readingValues(file, id).map((value) => value.name);
}

/**
 * The characters `file`'s values that read with the reference `id` need between them, of those the reference reads,
 * or none for every character. A value that needs none needs every character the reference reads, which is every
 * character when it does not say, and so is a reference no value reads with. A fix reads the characters its value
 * needs.
 */
export function readersNeed(file: AppFile, id: string): readonly string[] {
  const own = file.references.find((reference) => reference.id === id)?.characters ?? '';
  const needs = readingValues(file, id).map(({ screen }) =>
    screen.check.kind === 'number' ? (screen.check.characters ?? '') : '',
  );
  if (needs.length === 0 || needs.includes('')) {
    return knownCharacters([own]);
  }
  const needed = knownCharacters(needs);
  const reads = knownCharacters([own]);
  return reads.length === 0 ? needed : needed.filter((character) => reads.includes(character));
}

/** `names` as a sentence's list: `A`, `A and B`, or `A, B and C`. */
export function andList(names: readonly string[]): string {
  if (names.length <= 1) {
    return names.join('');
  }
  return `${names.slice(0, -1).join(', ')} and ${names.at(-1) ?? ''}`;
}

/** Why a reference that the values `names` read with cannot be deleted, or nothing when no value does. */
export function keptByReaders(names: readonly string[]): string | undefined {
  if (names.length === 0) {
    return undefined;
  }
  return names.length === 1
    ? `${andList(names)} reads with it. Point it at another reference first.`
    : `${andList(names)} read with it. Point them at another reference first.`;
}
