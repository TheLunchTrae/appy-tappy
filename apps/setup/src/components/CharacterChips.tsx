import { type Color, hex } from '../lib/pixels.ts';

/** A reference's characters, each in a tile drawn in the characters' own color. */
export function CharacterChips({
  characters,
  ink,
  large = false,
  pending = [],
}: {
  readonly characters: readonly string[];
  readonly ink: Color;
  readonly large?: boolean;
  /** Characters of `characters` that only text not added yet would teach, which are outlined. */
  readonly pending?: readonly string[];
}) {
  if (characters.length === 0) {
    return <span className="note">No characters yet</span>;
  }
  return (
    <span
      className={large ? 'glyphs large' : 'glyphs'}
      aria-label={
        pending.length === 0
          ? `Characters: ${characters.join(' ')}`
          : `Characters: ${characters.join(' ')}. Only text not added yet teaches ${pending.join(' ')}.`
      }
    >
      {characters.map((character) => (
        <span
          key={character}
          className={pending.includes(character) ? 'glyph pending' : 'glyph'}
          style={{ color: hex(ink) }}
          aria-hidden="true"
        >
          {character}
        </span>
      ))}
    </span>
  );
}

/** The characters a reference needs and has yet to learn, each in an outlined tile, since it has no shape for them. */
export function MissingChips({
  characters,
  large = false,
}: {
  readonly characters: readonly string[];
  readonly large?: boolean;
}) {
  return (
    <span
      className={large ? 'glyphs large missing' : 'glyphs missing'}
      aria-label={`Missing characters: ${characters.join(' ')}`}
    >
      {characters.map((character) => (
        <span key={character} className="glyph" aria-hidden="true">
          {character}
        </span>
      ))}
    </span>
  );
}

/** The characters a reference expects and has yet to learn, in one line, or nothing when it has them all. */
export function MissingNote({ characters }: { readonly characters: readonly string[] }) {
  return characters.length === 0 ? null : (
    <span className="missing-note">
      <span className="warn" aria-hidden="true">
        Missing
      </span>
      <MissingChips characters={characters} />
    </span>
  );
}
