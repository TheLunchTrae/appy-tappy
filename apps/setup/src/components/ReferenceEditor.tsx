import type { ReactNode } from 'react';

import type { Color } from '../lib/pixels.ts';
import { andList, knownCharacters } from '../lib/reference.ts';
import { CharacterChips } from './CharacterChips.tsx';
import { ColorField, ToleranceField } from './Fields.tsx';

interface Props {
  readonly name: string;
  readonly appName: string;
  readonly ink: Color | null;
  readonly tolerance: number;
  readonly picking: boolean;
  readonly canPick: boolean;
  /** The selected screenshot's index, or null while the live window shows. */
  readonly selected: number | null;
  /** The selected screenshot's label. */
  readonly label: string;
  /** What the saved reference learned from already. */
  readonly saved: readonly string[];
  /** The labeled screenshots' and kept readings' texts. */
  readonly labeled: readonly string[];
  readonly recorded: readonly string[];
  /** The values that read with the saved reference. */
  readonly readers: readonly string[];
  /** The recording panel. */
  readonly record: ReactNode;
  readonly onName: (name: string) => void;
  readonly onPick: () => void;
  readonly onTolerance: (tolerance: number) => void;
  readonly onLabel: (text: string) => void;
}

function plural(count: number, one: string, many: string): string {
  return `${count} ${count === 1 ? one : many}`;
}

function learnedNote(saved: number, labeled: number, recorded: number): string {
  const parts = [
    saved === 0 ? null : plural(saved, 'saved picture', 'saved pictures'),
    labeled === 0 ? null : plural(labeled, 'labeled screenshot', 'labeled screenshots'),
    recorded === 0 ? null : plural(recorded, 'recorded reading', 'recorded readings'),
  ].filter((part) => part !== null);
  return parts.length === 0 ? 'Nothing learned yet.' : `Learned from ${andList(parts)}.`;
}

/** The inspector of a reference: its name and color, and the characters it learns. */
export function ReferenceEditor(props: Props) {
  const { name, appName, ink, tolerance, selected, saved, labeled, recorded, readers } = props;
  const characters = knownCharacters([...saved, ...labeled, ...recorded]);
  return (
    <>
      <div className="field">
        <label htmlFor="reference-name">Name</label>
        <input
          id="reference-name"
          className="input"
          placeholder="Percent font"
          value={name}
          onChange={(event) => {
            props.onName(event.target.value);
          }}
        />
      </div>
      <div className="field">
        <label htmlFor="reference-app">Application</label>
        <input id="reference-app" className="input" readOnly value={appName} />
      </div>
      <ColorField
        label="Characters' color"
        color={ink}
        picking={props.picking}
        canPick={props.canPick}
        onPick={props.onPick}
      />
      <ToleranceField
        help="How far a pixel's color may be from the characters' color."
        value={tolerance}
        onChange={props.onTolerance}
      />
      <div className="field">
        <label htmlFor="label-text">What the region shows on this screenshot</label>
        {selected === null ? (
          <p className="note">Select a screenshot to type what it shows, such as 87%.</p>
        ) : (
          <input
            id="label-text"
            className="input"
            placeholder="87%"
            value={props.label}
            onChange={(event) => {
              props.onLabel(event.target.value);
            }}
          />
        )}
      </div>
      <div className="field">
        <span>Characters it knows</span>
        {ink === null ? (
          <span className="note">{characters.length === 0 ? 'None yet.' : characters.join(' ')}</span>
        ) : (
          <CharacterChips characters={characters} ink={ink} large />
        )}
        <span className="note">
          {learnedNote(saved.length, labeled.length, recorded.length)} Draw a region around text on any screenshot, then
          type what it shows or record it.
        </span>
      </div>
      {props.record}
      {readers.length === 0 ? null : (
        <p className="note">
          Used by {andList(readers)}. Changing it changes how {readers.length === 1 ? 'it is' : 'they are'} read.
        </p>
      )}
    </>
  );
}
