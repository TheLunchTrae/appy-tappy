import { type ChangeEvent, useReducer, useRef } from 'react';

import { errorText, importApp, readSharedApp } from '../lib/api.ts';
import type { AntiCheatDef } from '../lib/bindings.ts';
import { checkedText } from '../lib/build.ts';
import { ANTI_CHEAT_CHOICES, ANTI_CHEAT_NAMES } from '../lib/describe.ts';
import { IMPORT_ANTI_CHEAT_HINT, ORIGIN_HINT, SHARED_CHECKED_HINT } from '../lib/hints.ts';
import { matchSummary } from '../lib/matching.ts';
import { background } from '../lib/repeat.ts';
import { type ImportState, NO_IMPORT, importBusy, nextImport, originText, sizeProblem } from '../lib/sharing.ts';
import { Dialog } from './Dialog.tsx';
import { Hint } from './Hint.tsx';

/** `names` as a count and a list, such as `2: Progress, Status`, or `None`. */
function listed(names: readonly string[]): string {
  return names.length === 0 ? 'None' : `${names.length}: ${names.join(', ')}`;
}

interface AskingProps {
  readonly state: Extract<ImportState, { kind: 'asking' | 'importing' }>;
  readonly onName: (name: string) => void;
  readonly onClass: (antiCheat: AntiCheatDef) => void;
}

/** What a shared file holds, and the two things the user settles before it is imported: its name and its anti-cheat. */
function Asking({ state, onName, onClass }: AskingProps) {
  const { file } = state.view;
  const busy = state.kind === 'importing';
  const summary = matchSummary(file.window);
  const checked = checkedText(file, (at) => at.toLocaleDateString(undefined, { dateStyle: 'medium' }));
  return (
    <>
      <div className="field">
        <label htmlFor="import-name">Name</label>
        <input
          id="import-name"
          className="input"
          value={state.name}
          disabled={busy}
          onChange={(event) => {
            onName(event.target.value);
          }}
        />
      </div>
      <dl className="facts">
        <dt>Found by</dt>
        <dd>
          {summary.must}
          {summary.then === null ? null : <span className="note"> {summary.then}</span>}
        </dd>
        <dt>Values</dt>
        <dd>{listed(file.values.map((value) => value.name))}</dd>
        <dt>References</dt>
        <dd>{listed(file.references.map((reference) => reference.name))}</dd>
        <dt>
          <span className="with-hint">
            Values checked
            <Hint topic="Values checked">{SHARED_CHECKED_HINT}</Hint>
          </span>
        </dt>
        <dd>{checked === null ? 'Not recorded' : checked.replace(/^checked /u, '')}</dd>
        <dt>
          <span className="with-hint">
            How it was made
            <Hint topic="How it was made">{ORIGIN_HINT}</Hint>
          </span>
        </dt>
        <dd>{originText(file.origin)}</dd>
      </dl>
      <div className="field decide">
        <span className="with-hint">
          <label htmlFor="import-anti-cheat">Anti-cheat</label>
          <Hint topic="Anti-cheat of an imported application">{IMPORT_ANTI_CHEAT_HINT}</Hint>
        </span>
        <select
          id="import-anti-cheat"
          className="input"
          aria-describedby="import-anti-cheat-note"
          value={state.antiCheat}
          disabled={busy}
          onChange={(event) => {
            const picked = ANTI_CHEAT_CHOICES.find((choice) => choice === event.target.value);
            if (picked !== undefined) {
              onClass(picked);
            }
          }}
        >
          {ANTI_CHEAT_CHOICES.map((choice) => (
            <option key={choice} value={choice}>
              {ANTI_CHEAT_NAMES[choice]}
            </option>
          ))}
        </select>
        <span className="note" id="import-anti-cheat-note">
          The file says {ANTI_CHEAT_NAMES[file.antiCheat]}. Pick what you know the application has.
        </span>
        {state.antiCheat === 'none' ? (
          <span className="warn" role="status">
            Only pick None if the application has no anti-cheat. Pressing keys into an application with anti-cheat can
            get your account banned.
          </span>
        ) : null}
      </div>
    </>
  );
}

interface Props {
  /** Called once an application was imported, to list it. */
  readonly onImported: () => Promise<void>;
}

/**
 * The Import button and its dialog: the user picks a shared file, sees what it holds, names the application and
 * confirms its anti-cheat, and it is saved as a new application. `nextImport` holds the dialog's states.
 */
export function ImportApp({ onImported }: Props) {
  const [state, send] = useReducer(nextImport, NO_IMPORT);
  const picker = useRef<HTMLInputElement>(null);
  const busy = importBusy(state);

  const read = async (picked: File): Promise<void> => {
    const problem = sizeProblem(picked.name, picked.size);
    send({ kind: 'picked', file: picked.name, problem });
    if (problem !== null) {
      return;
    }
    try {
      const bytes = new Uint8Array(await picked.arrayBuffer());
      send({ kind: 'read', bytes, view: await readSharedApp(bytes, picked.name) });
    } catch (failure: unknown) {
      send({ kind: 'failed', problem: errorText(failure) });
    }
  };
  const pick = (event: ChangeEvent<HTMLInputElement>): void => {
    const picked = event.target.files?.[0];
    // Emptied, so picking the same file again after a refusal asks again.
    event.target.value = '';
    if (picked !== undefined) {
      background(read(picked));
    }
  };
  const save = async (asked: Extract<ImportState, { kind: 'asking' }>): Promise<void> => {
    send({ kind: 'import' });
    try {
      await importApp(asked.bytes, { name: asked.name, antiCheat: asked.antiCheat, from: asked.file });
      send({ kind: 'imported' });
      await onImported();
    } catch (failure: unknown) {
      send({ kind: 'failed', problem: errorText(failure) });
    }
  };
  const close = (): void => {
    send({ kind: 'close' });
  };

  return (
    <>
      <button
        type="button"
        className="btn"
        onClick={() => {
          picker.current?.click();
        }}
      >
        Import
      </button>
      <input
        ref={picker}
        type="file"
        accept=".zip,application/zip"
        hidden
        aria-label="Application file to import"
        onChange={pick}
      />
      {state.kind === 'closed' ? null : (
        <Dialog titleId="import-title" describedBy="import-what" busy={busy} onClose={close}>
          <h2 id="import-title">Import application</h2>
          {state.kind === 'reading' ? (
            <p id="import-what" role="status">
              Reading {state.file}…
            </p>
          ) : null}
          {state.kind === 'refused' ? (
            <p id="import-what" className="warn" role="alert">
              {state.problem} Nothing was imported.
            </p>
          ) : null}
          {state.kind === 'asking' || state.kind === 'importing' ? (
            <>
              <p id="import-what">
                {state.file} holds one application and its values. It is added beside your applications and replaces
                none of them, and nothing uses its values until you make an automation with them.
              </p>
              <Asking
                state={state}
                onName={(name) => {
                  send({ kind: 'named', name });
                }}
                onClass={(antiCheat) => {
                  send({ kind: 'classed', antiCheat });
                }}
              />
            </>
          ) : null}
          {state.kind === 'asking' && state.problem !== null ? (
            <p className="warn" role="alert">
              {state.problem}
            </p>
          ) : null}
          <div className="dialog-actions">
            <button type="button" className="btn" disabled={busy} onClick={close}>
              {state.kind === 'refused' ? 'Close' : 'Cancel'}
            </button>
            {state.kind === 'asking' || state.kind === 'importing' ? (
              <button
                type="button"
                className="btn primary"
                aria-busy={busy}
                disabled={busy}
                onClick={() => {
                  if (state.kind === 'asking') {
                    background(save(state));
                  }
                }}
              >
                {busy ? 'Importing…' : 'Import'}
              </button>
            ) : null}
          </div>
        </Dialog>
      )}
    </>
  );
}
