import type { AppFile } from '../lib/bindings.ts';
import { coverage, knowsNone } from '../lib/expected.ts';
import { readers, referenceCharacters } from '../lib/reference.ts';
import { REFERENCE_HINT } from '../lib/hints.ts';
import { draftHolds } from '../lib/teaching.ts';
import { CharacterChips, MissingNote } from './CharacterChips.tsx';
import { ExpectedCharacters } from './ExpectedCharacters.tsx';
import { Hint } from './Hint.tsx';

/** The id of the note that says the picked reference has a draft of changes, which Record readings points at. */
const DRAFT_HOLDS_ID = 'reference-draft-holds';

/** The id of the note that says a region has to be drawn, which the buttons that start from the region point at. */
const NEEDS_REGION_ID = 'reference-needs-region';

interface Props {
  readonly appName: string;
  /** The application's saved file, or null until it is read. */
  readonly file: AppFile | null;
  readonly selected: string | null;
  /** Whether a region is drawn to make a reference from. */
  readonly hasRegion: boolean;
  readonly whole: boolean;
  /** The characters the value needs, or none for every character its reference knows. */
  readonly characters: readonly string[];
  readonly onSelect: (reference: string) => void;
  readonly onEdit: (reference: string) => void;
  /** The references that have a draft of changes, which are changed by finishing it instead: no Edit, no recording. */
  readonly drafted: ReadonlySet<string>;
  readonly onNew: () => void;
  /** Opens the picked reference, or a new one, to record the region's readings. */
  readonly onRecord: () => void;
  readonly onWhole: (whole: boolean) => void;
  readonly onCharacters: (characters: readonly string[]) => void;
}

/**
 * How a number value is read: the characters it needs, one of its application's references, and whether only whole
 * numbers count.
 */
export function ReferencePicker(props: Props) {
  const { appName, file, selected, hasRegion, whole } = props;
  const references = file?.references ?? [];
  const chosen = references.find((reference) => reference.id === selected) ?? null;
  const held = chosen === null ? null : draftHolds(chosen, props.drafted);
  const { characters } = props;
  const recordWaits = [hasRegion ? null : NEEDS_REGION_ID, held === null ? null : DRAFT_HOLDS_ID].filter(
    (id) => id !== null,
  );
  return (
    <>
      <ExpectedCharacters expected={characters} onExpected={props.onCharacters} />
      <div className="field">
        <span className="name">
          <span id="reference-title">Reference</span>
          <Hint topic="Reference">{REFERENCE_HINT}</Hint>
        </span>
        <span className="note">
          The shapes of the characters this value is read with. Pick one made for {appName}, or make one.
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
                  <MissingNote characters={coverage(characters, referenceCharacters(reference)).missing} />
                </button>
              );
            })}
          </div>
        )}
        {chosen !== null && knowsNone(characters, referenceCharacters(chosen)) ? (
          <span className="warn" role="status">
            {chosen.name} knows none of these characters yet. Teach it them, or choose another reference.
          </span>
        ) : null}
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
            // Not disabled without a region, so it takes focus and a screen reader reads why.
            aria-disabled={!hasRegion}
            aria-describedby={hasRegion ? undefined : NEEDS_REGION_ID}
            onClick={() => {
              if (hasRegion) {
                props.onNew();
              }
            }}
          >
            New reference from this region
          </button>
          <button
            type="button"
            className="btn small"
            aria-disabled={recordWaits.length > 0}
            aria-describedby={recordWaits.length === 0 ? undefined : recordWaits.join(' ')}
            onClick={() => {
              if (recordWaits.length === 0) {
                props.onRecord();
              }
            }}
          >
            Record readings
          </button>
        </div>
        {hasRegion ? null : (
          <span id={NEEDS_REGION_ID} className="note">
            Draw the region first.
          </span>
        )}
        <span className="note">
          Record readings watches the region while you use {appName} and keeps each different reading, to teach{' '}
          {chosen === null ? 'a new reference' : chosen.name} its characters.
        </span>
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
