import type { AppFile } from '../lib/bindings.ts';
import { coverage, knowsNone } from '../lib/expected.ts';
import { readers, referenceCharacters } from '../lib/reference.ts';
import { REFERENCE_HINT } from '../lib/hints.ts';
import { CharacterChips, MissingNote } from './CharacterChips.tsx';
import { ExpectedCharacters } from './ExpectedCharacters.tsx';
import { Hint } from './Hint.tsx';

interface Props {
  readonly appName: string;
  /** The application's saved file, or null until it is read. */
  readonly file: AppFile | null;
  readonly selected: string | null;
  readonly whole: boolean;
  /** The characters the value needs, or none for every character its reference knows. */
  readonly characters: readonly string[];
  readonly onSelect: (reference: string) => void;
  readonly onWhole: (whole: boolean) => void;
  readonly onCharacters: (characters: readonly string[]) => void;
}

/**
 * How a number value is read: the characters it needs, one of its application's saved references, and whether only
 * whole numbers count. References are made and taught on the References page, never from here.
 */
export function ReferencePicker(props: Props) {
  const { appName, file, selected, whole, characters } = props;
  const references = file?.references ?? [];
  const chosen = references.find((reference) => reference.id === selected) ?? null;
  return (
    <>
      <ExpectedCharacters expected={characters} onExpected={props.onCharacters} />
      <div className="field">
        <span className="name">
          <span id="reference-title">Reference</span>
          <Hint topic="Reference">{REFERENCE_HINT}</Hint>
        </span>
        <span className="note">
          The shapes of the characters this value is read with. Pick one made for {appName}. References are made and
          taught on the References page.
        </span>
        {references.length === 0 ? (
          <p className="note">
            {appName} has no references yet. Make one on the References page, then pick it here. Save as draft keeps
            this value until then.
          </p>
        ) : (
          <div className="refs" role="group" aria-labelledby="reference-title">
            {references.map((reference) => {
              const used = file === null ? 0 : readers(file, reference.id).length;
              return (
                <button
                  key={reference.id}
                  type="button"
                  className="ref"
                  aria-pressed={reference.id === selected}
                  onClick={() => {
                    props.onSelect(reference.id);
                  }}
                >
                  <span className="ref-head">
                    <b>{reference.name}</b>
                    <small>{used === 0 ? 'No values yet' : `Used by ${used} ${used === 1 ? 'value' : 'values'}`}</small>
                  </span>
                  <CharacterChips characters={referenceCharacters(reference)} ink={reference.ink} />
                  <MissingNote characters={coverage(characters, referenceCharacters(reference)).missing} />
                </button>
              );
            })}
          </div>
        )}
        {chosen !== null && knowsNone(characters, referenceCharacters(chosen)) ? (
          <span className="warn" role="status">
            {chosen.name} knows none of these characters yet. Teach it them on the References page, or choose another
            reference.
          </span>
        ) : null}
      </div>
      <div className="field">
        <label className="toggle">
          <input
            type="checkbox"
            className="switch"
            checked={whole}
            onChange={(event) => {
              props.onWhole(event.target.checked);
            }}
          />
          Whole number only
        </label>
        <span className="note">
          Reads 87.5% as 87. Every digit after the decimal point is ignored, though the reference still has to know the
          point.
        </span>
      </div>
    </>
  );
}
