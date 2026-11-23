import type { ReactNode } from 'react';

import type { Coverage } from '../lib/expected.ts';
import { andList, knownCharacters } from '../lib/reference.ts';
import { GUESSES_HINT, LOOKALIKE_HINT } from '../lib/hints.ts';
import { lookalikeWarning } from '../lib/useReadingRisks.ts';
import { CharacterChips, MissingChips } from './CharacterChips.tsx';
import { ExpectedCharacters } from './ExpectedCharacters.tsx';
import { Hint } from './Hint.tsx';

interface Props {
  readonly name: string;
  readonly appName: string;
  /** What the saved reference learned from already. */
  readonly saved: readonly string[];
  /** The labeled boxes' texts. */
  readonly labeled: readonly string[];
  readonly recorded: readonly string[];
  /** The characters it reads, each once; none for every character it learns. What it learned is compared with these. */
  readonly reads: readonly string[];
  readonly onReads: (reads: readonly string[]) => void;
  /** Whether a running automation guesses at the characters it has not learned. */
  readonly guesses: boolean;
  readonly onGuesses: (guesses: boolean) => void;
  /** What saving it reading those would take from the values that read with it, or null. */
  readonly readersLose: string | null;
  /** How the characters in those texts compare with the ones it reads. */
  readonly coverage: Coverage;
  /** The pairs of characters it can't tell apart, of those with a character it reads. */
  readonly lookalikes: readonly (readonly [string, string])[];
  /** Why that could not be checked, or null. */
  readonly risksProblem: string | null;
  /** The texts of found text not added yet, whose new characters are outlined. */
  readonly finding: readonly string[];
  /** The values that read with the saved reference. */
  readonly readers: readonly string[];
  /** The list of boxes the reference learns from. */
  readonly boxes: ReactNode;
  /** The panel that finds more text like the selected box. */
  readonly findPanel: ReactNode;
  /** The recording panel. */
  readonly record: ReactNode;
  /** The saved pictures it can't learn, with a way to leave them out. */
  readonly unlearned: ReactNode;
  readonly onName: (name: string) => void;
}

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

/** The inspector of a reference: its name, the characters it reads, and the ones it learns. */
export function ReferenceEditor(props: Props) {
  const { name, appName, saved, labeled, recorded, finding } = props;
  const { coverage, readers } = props;
  const expected = props.reads;
  const lookalike = lookalikeWarning(props.lookalikes);
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
      <ExpectedCharacters owner="reference" expected={props.reads} onExpected={props.onReads} />
      {props.readersLose === null ? null : (
        <p className="warn" role="status">
          {props.readersLose}
        </p>
      )}
      <div className="field">
        <span className="name">
          <label className="toggle">
            <input
              type="checkbox"
              className="switch"
              aria-describedby="hint-guesses"
              checked={props.guesses}
              onChange={(event) => {
                props.onGuesses(event.target.checked);
              }}
            />
            Guess characters it has not learned
          </label>
          <Hint topic="Guess characters it has not learned" id="hint-guesses">
            {GUESSES_HINT}
          </Hint>
        </span>
      </div>
      {props.boxes}
      {props.findPanel}
      {chosen ? (
        <div className="field">
          <span>Characters still missing</span>
          {coverage.missing.length === 0 ? (
            <span className="ok">None. It knows every character it reads.</span>
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
          <CharacterChips characters={known} pending={pending.filter(isKnown)} large />
        )}
        {extra.length === 0 ? null : (
          <>
            <span className="note">Not among the characters it reads, so it reads none of these:</span>
            <CharacterChips characters={extra} pending={pending.filter(isExtra)} large />
          </>
        )}
        <span className="note">
          {learnedNote(saved.length, labeled.length, recorded.length)}
          {pending.length === 0 ? '' : ' Outlined characters come from the found text, once you add it.'}
        </span>
        {props.risksProblem === null ? null : <p className="warn">{props.risksProblem}</p>}
        {lookalike === null ? null : (
          <div className="row">
            <p className="warn">{lookalike}</p>
            <Hint topic="Characters that look alike">{LOOKALIKE_HINT}</Hint>
          </div>
        )}
      </div>
      {props.record}
      {props.unlearned}
      {readers.length === 0 ? null : (
        <p className="note">
          Used by {andList(readers)}. Changing it changes how {readers.length === 1 ? 'it is' : 'they are'} read.
        </p>
      )}
    </>
  );
}
