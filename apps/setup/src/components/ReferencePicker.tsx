import type { AppFile } from '../lib/bindings.ts';
import { referenceCoverage } from '../lib/expected.ts';
import { readers, referenceCharacters } from '../lib/reference.ts';
import { REFERENCE_HINT } from '../lib/hints.ts';
import { draftHolds } from '../lib/teaching.ts';
import { CharacterChips, MissingNote } from './CharacterChips.tsx';
import { CharactersRead } from './CharactersRead.tsx';
import { Hint } from './Hint.tsx';

/** The id of the note that says the picked reference has a draft of changes, which Record readings points at. */
const DRAFT_HOLDS_ID = 'reference-draft-holds';

interface Props {
  readonly appName: string;
  /** The application's saved file, or null until it is read. */
  readonly file: AppFile | null;
  readonly selected: string | null;
  /** Whether a region is drawn to make a reference from. */
  readonly hasRegion: boolean;
  readonly whole: boolean;
  /** The characters the value reads of its reference, or null for every character it knows. */
  readonly characters: readonly string[] | null;
  readonly onSelect: (reference: string) => void;
  readonly onEdit: (reference: string) => void;
  /** The references that have a draft of changes, which are changed by finishing it instead: no Edit, no recording. */
  readonly drafted: ReadonlySet<string>;
  readonly onNew: () => void;
  /** Opens the picked reference, or a new one, to record the region's readings. */
  readonly onRecord: () => void;
  readonly onWhole: (whole: boolean) => void;
  readonly onCharacters: (characters: readonly string[] | null) => void;
}

/**
 * How a number value is read: one of its application's references, which of its characters, and whether only whole
 * numbers count.
 */
export function ReferencePicker(props: Props) {
  const { appName, file, selected, hasRegion, whole } = props;
  const references = file?.references ?? [];
  const chosen = references.find((reference) => reference.id === selected) ?? null;
  const held = chosen === null ? null : draftHolds(chosen, props.drafted);
  return (
    <>
      <div className="field">
        <span className="name">
          <span id="reference-title">Reference</span>
          <Hint topic="Reference">{REFERENCE_HINT}</Hint>
        </span>
        <span className="note">
          The characters&apos; shapes and color this value is read with. Pick one made for {appName}, or make one.
        </span>
        {references.length === 0 ? (
          <p className="note">{appName} has no references yet. Make one from this region.</p>
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
                  <MissingNote characters={referenceCoverage(reference).missing} />
                </button>
              );
            })}
          </div>
        )}
        <div className="choices">
          {chosen === null ? null : held !== null ? (
            // Changing it here would change it apart from its draft, which finishing later would undo.
            <span id={DRAFT_HOLDS_ID} className="note">
              {held}
            </span>
          ) : (
            <button
              type="button"
              className="btn small"
              onClick={() => {
                props.onEdit(chosen.id);
              }}
            >
              Edit {chosen.name}
            </button>
          )}
          <button
            type="button"
            className="btn small"
            disabled={!hasRegion}
            title={hasRegion ? undefined : 'Draw the region first'}
            onClick={props.onNew}
          >
            New reference from this region
          </button>
          <button
            type="button"
            className="btn small"
            disabled={!hasRegion || held !== null}
            title={hasRegion ? (held ?? undefined) : 'Draw the region first'}
            aria-describedby={held === null ? undefined : DRAFT_HOLDS_ID}
            onClick={props.onRecord}
          >
            Record readings
          </button>
        </div>
        <span className="note">
          Record readings watches the region while you use {appName} and keeps each different reading, to teach{' '}
          {chosen === null ? 'a new reference' : chosen.name} its characters.
        </span>
      </div>
      {chosen === null ? null : (
        <CharactersRead reference={chosen} characters={props.characters} onCharacters={props.onCharacters} />
      )}
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
