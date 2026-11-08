import type { AppFile, ReferenceDef } from './bindings.ts';
// How references read on the capture screen and the References page.

/** Digits before other characters, then by code point, so the order is the same on every machine. */
function order(left: string, right: string): number {
  const digits = Number(!/\d/u.test(left)) - Number(!/\d/u.test(right));
  if (digits !== 0) {
    return digits;
  }
  return left < right ? -1 : Number(left > right);
}

/** Every character `texts` show, once each, digits first, without whitespace. */
export function knownCharacters(texts: Iterable<string>): readonly string[] {
  const found = new Set<string>();
  for (const text of texts) {
    for (const character of text) {
      if (character.trim() !== '') {
        found.add(character);
      }
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
