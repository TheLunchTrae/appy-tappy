import { useState } from 'react';

import type { ReferenceDef } from '../lib/bindings.ts';
import { READ_CHOICE_NAMES, readChoice, readChoices, withCharacter, withReadChoice } from '../lib/charactersRead.ts';
import { CHARACTERS_READ_HINT } from '../lib/hints.ts';
import { hex } from '../lib/pixels.ts';
import { referenceCharacters } from '../lib/reference.ts';
import { CharacterChips } from './CharacterChips.tsx';
import { Hint } from './Hint.tsx';

interface Props {
  /** The reference the value reads with. */
  readonly reference: ReferenceDef;
  /** The characters the value reads of it, or null for every character it knows. */
  readonly characters: readonly string[] | null;
  readonly onCharacters: (characters: readonly string[] | null) => void;
}

/**
 * Which of its reference's characters a number value reads: every one, a preset, or the ones turned on one by one.
 * Only characters the reference knows show.
 */
export function CharactersRead({ reference, characters, onCharacters }: Props) {
  const known = referenceCharacters(reference);
  // Choose stays picked while its set happens to be a preset's, until another choice is.
  const [choosing, setChoosing] = useState(false);
  const choice = choosing && characters !== null ? 'choose' : readChoice(characters, known);
  const read = characters === null ? known : known.filter((character) => characters.includes(character));
  return (
    <div className="field">
      <span className="name">
        <label htmlFor="characters-read">Characters it reads</label>
        <Hint topic="Characters it reads">{CHARACTERS_READ_HINT}</Hint>
      </span>
      <select
        id="characters-read"
        className="input"
        value={choice}
        onChange={(event) => {
          const picked = readChoices(known).find((each) => each === event.target.value) ?? 'all';
          setChoosing(picked === 'choose');
          onCharacters(withReadChoice(picked, known, characters));
        }}
      >
        {readChoices(known).map((each) => (
          <option key={each} value={each}>
            {READ_CHOICE_NAMES[each]}
          </option>
        ))}
      </select>
      {characters === null ? (
        <span className="note">Reads every character {reference.name} knows, and any it learns later.</span>
      ) : choice === 'choose' ? (
        <div className="needs" role="group" aria-label={`Characters of ${reference.name} to read`}>
          {known.map((character) => (
            <label key={character} className="need" aria-label={character}>
              <input
                type="checkbox"
                checked={characters.includes(character)}
                onChange={(event) => {
                  onCharacters(withCharacter(characters, character, event.target.checked));
                }}
              />
              <span className="glyph" style={{ color: hex(reference.ink) }} aria-hidden="true">
                {character}
              </span>
            </label>
          ))}
        </div>
      ) : read.length === 0 ? null : (
        <CharacterChips characters={read} ink={reference.ink} />
      )}
      {characters !== null && characters.length > 0 && read.length === 0 ? (
        <span className="warn" role="alert">
          {reference.name} knows none of these characters yet. Choose characters it knows, or teach it them.
        </span>
      ) : null}
    </div>
  );
}
