import { type ReactNode, useState } from 'react';

import type { NeedsField } from '../lib/captureDraft.ts';
import type { Coverage } from '../lib/expected.ts';
import type { Color } from '../lib/pixels.ts';
import { andList, knownCharacters } from '../lib/reference.ts';
import { INK_HINT, INK_TOLERANCE_HINT, LOOKALIKE_HINT } from '../lib/hints.ts';
import { lookalikeWarning } from '../lib/useLookalikes.ts';
import { CharacterChips, MissingChips } from './CharacterChips.tsx';
import { ExpectedCharacters } from './ExpectedCharacters.tsx';
import { ColorField, ToleranceField } from './Fields.tsx';
import { Hint } from './Hint.tsx';

interface Props {
  readonly name: string;
  readonly appName: string;
  /** The reference's text color, or null while it finds its characters by contrast. */
  readonly ink: Color | null;
  /** How far a pixel may be from the text color, which shows only with one. */
  readonly tolerance: number;
  readonly picking: boolean;
  readonly canPick: boolean;
  /** What the saved reference learned from already. */
  readonly saved: readonly string[];
  /** The labeled boxes' texts. */
  readonly labeled: readonly string[];
  readonly recorded: readonly string[];
  /** The characters the value it is taught for needs, each once; none for a reference made by itself. */
  readonly expected: readonly string[];
  /** How the characters in those texts compare with the ones it needs. */
  readonly coverage: Coverage;
  /** The pairs of characters it may read as each other, of those with a character the value needs. */
  readonly lookalikes: readonly (readonly [string, string])[];
  /** The texts of found text not added yet, whose new characters are outlined. */
  readonly finding: readonly string[];
  /** The values that read with the saved reference. */
  readonly readers: readonly string[];
  /** The list of boxes the reference learns from. */
  readonly boxes: ReactNode;
  /** The panel that finds more text like the selected box. */
  readonly findPanel: ReactNode;
  /** The matching pixels preview of what counts as the characters. */
  readonly preview: ReactNode;
  /** The recording panel. */
  readonly record: ReactNode;
  readonly onName: (name: string) => void;
  readonly onPick: () => void;
  /** Goes back to finding the characters by contrast. */
  readonly onNoTextColor: () => void;
  readonly onTolerance: (tolerance: number) => void;
  /** How the characters the value needs show. */
  readonly needsField: NeedsField;
  readonly onExpected: (expected: readonly string[]) => void;
}

/** What the editor calls the list of the value it is taught for, since "it" here is the reference. */
const NEEDS_LABEL = 'Characters the value needs';

function plural(count: number, one: string, many: string): string {
  return `${count} ${count === 1 ? one : many}`;
}

function learnedNote(saved: number, labeled: number, recorded: number): string {
  const parts = [
    saved === 0 ? null : plural(saved, 'saved picture', 'saved pictures'),
    labeled === 0 ? null : plural(labeled, 'labeled box', 'labeled boxes'),
    recorded === 0 ? null : plural(recorded, 'recorded reading', 'recorded readings'),
  ].filter((part) => part !== null);
  return parts.length === 0 ? 'Nothing learned yet.' : `Learned from ${andList(parts)}.`;
}

/**
 * The text color, folded away since most references need none. It stays open for a reference that has one, which
 * also shows the tolerance and the way back to contrast.
 */
function TextColor(
  props: Pick<Props, 'ink' | 'tolerance' | 'picking' | 'canPick' | 'onPick' | 'onNoTextColor' | 'onTolerance'>,
) {
  const { ink } = props;
  const [opened, setOpened] = useState(false);
  return (
    <details
      className="tucked"
      open={opened || ink !== null}
      onToggle={(event) => {
        setOpened(event.currentTarget.open);
      }}
    >
      <summary>Text color (optional)</summary>
      <p className="note">
        AppyTappy finds the characters by how they stand out from the background behind them. Pick a text color only
        when that misreads: the background is busy, such as a scene that shows through behind the text, or a thick
        outline makes characters look alike.
      </p>
      <ColorField
        label="Text color"
        hint={INK_HINT}
        color={ink}
        unset="None"
        picking={props.picking}
        canPick={props.canPick}
        onPick={props.onPick}
      />
      {ink === null ? null : (
        <>
          <ToleranceField
            help="How far a pixel's color may be from the text color."
            hint={INK_TOLERANCE_HINT}
            value={props.tolerance}
            onChange={props.onTolerance}
          />
          <button type="button" className="btn small" onClick={props.onNoTextColor}>
            Remove text color
          </button>
        </>
      )}
    </details>
  );
}

/** The inspector of a reference: its name, the characters its value needs, the ones it learns, and its text color. */
export function ReferenceEditor(props: Props) {
  const { name, appName, ink, tolerance, saved, labeled, recorded, finding } = props;
  const { expected, coverage, readers } = props;
  const lookalike = lookalikeWarning(props.lookalikes, ink, props.tolerance);
  // With no characters needed, every character it learned shows as one group.
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
  const isKnown = (character: string): boolean => known.includes(character);
  const isExtra = (character: string): boolean => extra.includes(character);
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
      {props.needsField === 'editable' ? (
        <ExpectedCharacters label={NEEDS_LABEL} expected={expected} onExpected={props.onExpected} />
      ) : null}
      {props.needsField === 'readOnly' ? (
        <div className="field">
          <span>{NEEDS_LABEL}</span>
          {chosen ? <CharacterChips characters={expected} ink={ink} large /> : null}
          <span className="note">
            {chosen
              ? 'A fix needs the characters its value does. To change them, edit the value.'
              : 'The value needs no characters, so the fix reads every character this reference knows.'}
          </span>
        </div>
      ) : null}
      {props.boxes}
      {props.findPanel}
      {chosen ? (
        <div className="field">
          <span>Characters still missing</span>
          {coverage.missing.length === 0 ? (
            <span className="ok">None. It knows every character the value needs.</span>
          ) : (
            <>
              <MissingChips characters={coverage.missing} large />
              <span className="note">
                Draw a box around them and type what it shows, find more text like a box, or record until they have
                shown.
              </span>
            </>
          )}
        </div>
      ) : null}
      <div className="field">
        <span>Characters it knows</span>
        {known.length === 0 && extra.length > 0 ? null : (
          <CharacterChips characters={known} ink={ink} pending={pending.filter(isKnown)} large />
        )}
        {extra.length === 0 ? null : (
          <>
            <span className="note">Not among the characters the value needs, which is fine:</span>
            <CharacterChips characters={extra} ink={ink} pending={pending.filter(isExtra)} large />
          </>
        )}
        <span className="note">
          {learnedNote(saved.length, labeled.length, recorded.length)}
          {pending.length === 0 ? '' : ' Outlined characters come from the found text, once you add it.'}
        </span>
        {lookalike === null ? null : (
          <div className="row">
            <p className="warn">{lookalike}</p>
            <Hint topic="Characters that look alike">{LOOKALIKE_HINT}</Hint>
          </div>
        )}
      </div>
      {props.record}
      <TextColor
        ink={ink}
        tolerance={tolerance}
        picking={props.picking}
        canPick={props.canPick}
        onPick={props.onPick}
        onNoTextColor={props.onNoTextColor}
        onTolerance={props.onTolerance}
      />
      {props.preview}
      {readers.length === 0 ? null : (
        <p className="note">
          Used by {andList(readers)}. Changing it changes how {readers.length === 1 ? 'it is' : 'they are'} read.
        </p>
      )}
    </>
  );
}
