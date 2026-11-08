import { type Color, hex } from '../lib/pixels.ts';

/** A reference's characters, each in a tile drawn in the characters' own color. */
export function CharacterChips({
  characters,
  ink,
  large = false,
}: {
  readonly characters: readonly string[];
  readonly ink: Color;
  readonly large?: boolean;
}) {
  if (characters.length === 0) {
    return <span className="note">No characters yet</span>;
  }
  return (
    <span className={large ? 'glyphs large' : 'glyphs'} aria-label={`Characters: ${characters.join(' ')}`}>
      {characters.map((character) => (
        <span key={character} className="glyph" style={{ color: hex(ink) }} aria-hidden="true">
          {character}
        </span>
      ))}
    </span>
  );
}
