import type { AppFile, ReferenceDef } from './bindings.ts';
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

/** The characters `reference` learned from its saved pictures. */
export function referenceCharacters(reference: ReferenceDef): readonly string[] {
  return knownCharacters(reference.samples.map((sample) => sample.text));
}

/** The names of `file`'s values that read with the reference `id`, in their check or in a fix. */
export function readers(file: AppFile, id: string): readonly string[] {
  return file.values
    .filter(
      ({ screen }) =>
        (screen.check.kind === 'number' && screen.check.reference === id) ||
        screen.fixes.some((fix) => fix.reference === id),
    )
    .map((value) => value.name);
}

/** `names` as a sentence's list: `A`, `A and B`, or `A, B and C`. */
export function andList(names: readonly string[]): string {
  if (names.length <= 1) {
    return names.join('');
  }
  return `${names.slice(0, -1).join(', ')} and ${names.at(-1) ?? ''}`;
}
