import { useState } from 'react';

import type { ReferenceDef } from '../lib/bindings.ts';
import { READ_CHOICE_NAMES, readChoice, readChoices, withCharacter, withReadChoice } from '../lib/charactersRead.ts';
import { CHARACTERS_READ_HINT } from '../lib/hints.ts';
import { hex } from '../lib/pixels.ts';
import { knownCharacters, referenceCharacters } from '../lib/reference.ts';
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
 * Choosing one by one shows each character the reference knows, and each one the value reads that the reference has
 * yet to learn, outlined, so every character that is saved shows and can be turned off.
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
          {knownCharacters([...known, ...characters]).map((character) => {
            const learned = known.includes(character);
            return (
              <label
                key={character}
                className="need"
                aria-label={learned ? character : `${character}, not learned yet`}
              >
                <input
                  type="checkbox"
                  checked={characters.includes(character)}
                  onChange={(event) => {
                    onCharacters(withCharacter(characters, character, event.target.checked));
                  }}
                />
                <span className={learned ? undefined : 'glyphs missing'} aria-hidden="true">
                  <span className="glyph" style={learned ? { color: hex(reference.ink) } : undefined}>
                    {character}
                  </span>
                </span>
              </label>
            );
          })}
        </div>
      ) : read.length === 0 ? null : (
        <CharacterChips characters={read} ink={reference.ink} />
      )}
      {choice === 'digits' ? (
        <span className="note">
          For a number of digits alone. A sign after them ends the number, so 87% reads as 87, but {reference.name}{' '}
          still has to know the sign.
        </span>
      ) : null}
      {choice === 'choose' && characters !== null && read.length > 0 && read.length < characters.length ? (
        <span className="note">
          {reference.name} has yet to learn the outlined characters. The value reads each one once it is learned.
        </span>
      ) : null}
      {characters !== null && characters.length > 0 && read.length === 0 ? (
        <span className="warn" role="alert">
          {reference.name} knows none of these characters yet. Choose characters it knows, or teach it them.
        </span>
      ) : null}
    </div>
  );
}
