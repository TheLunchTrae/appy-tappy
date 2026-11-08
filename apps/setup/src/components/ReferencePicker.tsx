import type { AppFile } from '../lib/api.ts';
import { readers, referenceCharacters } from '../lib/reference.ts';
import { CharacterChips } from './CharacterChips.tsx';

interface Props {
  readonly appName: string;
  /** The application's saved file, or null until it is read. */
  readonly file: AppFile | null;
  readonly selected: string | null;
  /** Whether a region is drawn to make a reference from. */
  readonly hasRegion: boolean;
  readonly whole: boolean;
  readonly onSelect: (reference: string) => void;
  readonly onEdit: (reference: string) => void;
  readonly onNew: () => void;
  readonly onWhole: (whole: boolean) => void;
}

/** How a number value is read: one of its application's references, and whether only whole numbers count. */
export function ReferencePicker(props: Props) {
  const { appName, file, selected, hasRegion, whole } = props;
  const references = file?.references ?? [];
  const chosen = references.find((reference) => reference.id === selected) ?? null;
  return (
    <>
      <div className="field">
        <span id="reference-title">Reference</span>
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
                </button>
              );
            })}
          </div>
        )}
        <div className="choices">
          {chosen === null ? null : (
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
        </div>
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
