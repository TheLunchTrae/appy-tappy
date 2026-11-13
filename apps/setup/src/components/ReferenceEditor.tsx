import type { ReactNode } from 'react';

import type { Coverage } from '../lib/expected.ts';
import type { Color } from '../lib/pixels.ts';
import { andList, knownCharacters } from '../lib/reference.ts';
import { INK_HINT, INK_TOLERANCE_HINT, LABEL_HINT } from '../lib/hints.ts';
import { CharacterChips, MissingChips } from './CharacterChips.tsx';
import { ExpectedCharacters } from './ExpectedCharacters.tsx';
import { Hint } from './Hint.tsx';
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
  /** The labeled screenshots', kept readings' and added found text's texts. */
  readonly labeled: readonly string[];
  readonly recorded: readonly string[];
  /** The characters it needs, each once. */
  readonly expected: readonly string[];
  /** How the characters in those texts compare with the ones it needs. */
  readonly coverage: Coverage;
  readonly found: readonly string[];
  /** The texts of found text not added yet, whose new characters are outlined. */
  readonly finding: readonly string[];
  /** The values that read with the saved reference. */
  readonly readers: readonly string[];
  /** The panel that finds more text like the first sample. */
  readonly findPanel: ReactNode;
  /** The matching pixels preview of the characters' color. */
  readonly preview: ReactNode;
  /** The recording panel. */
  readonly record: ReactNode;
  readonly onName: (name: string) => void;
  readonly onPick: () => void;
  readonly onTolerance: (tolerance: number) => void;
  readonly onLabel: (text: string) => void;
  readonly onExpected: (expected: readonly string[]) => void;
  /** Takes the added found text back out of the reference. */
  readonly onForgetFound: () => void;
}

function plural(count: number, one: string, many: string): string {
  return `${count} ${count === 1 ? one : many}`;
}

function learnedNote(saved: number, labeled: number, recorded: number, found: number): string {
  const parts = [
    saved === 0 ? null : plural(saved, 'saved picture', 'saved pictures'),
    labeled === 0 ? null : plural(labeled, 'labeled screenshot', 'labeled screenshots'),
    recorded === 0 ? null : plural(recorded, 'recorded reading', 'recorded readings'),
    found === 0 ? null : `${found} found like it`,
  ].filter((part) => part !== null);
  return parts.length === 0 ? 'Nothing learned yet.' : `Learned from ${andList(parts)}.`;
}

/** Characters the reference knows: tiles in their own color, or plain text until that color is picked. */
function Known({
  characters,
  pending,
  ink,
}: {
  readonly characters: readonly string[];
  /** The ones that found text would teach once it is added, which show outlined. */
  readonly pending: readonly string[];
  readonly ink: Color | null;
}) {
  return ink === null ? (
    <span className="note">{characters.length === 0 ? 'None yet.' : characters.join(' ')}</span>
  ) : (
    <CharacterChips
      characters={characters}
      ink={ink}
      pending={pending.filter((character) => characters.includes(character))}
      large
    />
  );
}

/** The inspector of a reference: its name and color, the characters it needs, and the ones it learns. */
export function ReferenceEditor(props: Props) {
  const { name, appName, ink, tolerance, selected, saved, labeled, recorded, found, finding } = props;
  const { expected, coverage, readers } = props;
  // Without a choice of what it needs, every character it learned shows as one group.
  const chosen = expected.length > 0;
  // Characters of found text not added yet show outlined beside the ones it knows.
  const pending = knownCharacters(finding).filter(
    (character) => !coverage.known.includes(character) && !coverage.extra.includes(character),
  );
  const needed = (character: string): boolean => !chosen || expected.includes(character);
  const known = knownCharacters([...(chosen ? coverage.known : coverage.extra), ...pending.filter(needed)]);
  const extra = chosen
    ? knownCharacters([...coverage.extra, ...pending.filter((character) => !needed(character))])
    : [];
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
        <span>Application</span>
        <p>{appName}</p>
      </div>
      <ColorField
        label="Characters' color"
        hint={INK_HINT}
        color={ink}
        picking={props.picking}
        canPick={props.canPick}
        onPick={props.onPick}
      />
      <ToleranceField
        help="How far a pixel's color may be from the characters' color."
        hint={INK_TOLERANCE_HINT}
        value={tolerance}
        onChange={props.onTolerance}
      />
      {props.preview}
      <ExpectedCharacters expected={expected} onExpected={props.onExpected} />
      <div className="field">
        <span className="name">
          <label htmlFor="label-text">What the region shows on this screenshot</label>
          <Hint topic="What the region shows" id="hint-label">
            {LABEL_HINT}
          </Hint>
        </span>
        {selected === null ? (
          <p className="note">Select a screenshot to type what it shows, such as 87%.</p>
        ) : (
          <input
            id="label-text"
            className="input"
            aria-describedby="hint-label"
            placeholder="87%"
            value={props.label}
            onChange={(event) => {
              props.onLabel(event.target.value);
            }}
          />
        )}
      </div>
      {props.findPanel}
      {chosen ? (
        <div className="field">
          <span>Characters still missing</span>
          {coverage.missing.length === 0 ? (
            <span className="ok">None. It knows every character it needs.</span>
          ) : (
            <>
              <MissingChips characters={coverage.missing} large />
              <span className="note">
                Label a screenshot that shows them, find more text like it, or record the region until it has shown
                them.
              </span>
            </>
          )}
        </div>
      ) : null}
      <div className="field">
        <span>Characters it knows</span>
        {known.length === 0 && extra.length > 0 ? null : <Known characters={known} pending={pending} ink={ink} />}
        {extra.length === 0 ? null : (
          <>
            <span className="note">Not among the characters it needs, which is fine:</span>
            <Known characters={extra} pending={pending} ink={ink} />
          </>
        )}
        <span className="note">
          {learnedNote(saved.length, labeled.length, recorded.length, found.length)}
          {pending.length === 0 ? '' : ' Outlined characters come from the found text, once you add it.'} Draw a region
          around text on any screenshot, then type what it shows or record it.
        </span>
        {found.length === 0 ? null : (
          <div className="row">
            <button type="button" className="btn" onClick={props.onForgetFound}>
              Remove found text
            </button>
          </div>
        )}
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
