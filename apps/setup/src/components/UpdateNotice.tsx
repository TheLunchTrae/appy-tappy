import { useEffect, useReducer } from 'react';

import { checkForUpdate, errorText, installUpdate } from '../lib/api.ts';
import { background } from '../lib/repeat.ts';
import { NO_UPDATE, nextUpdate } from '../lib/update.ts';
import { Question, useQuestion } from './Question.tsx';

interface Props {
  /** Whether updating now would lose work, such as an unsaved value; the update waits until it is saved or left. */
  readonly busy: boolean;
  /** Whether another question has the nav's place; the notice keeps its state while it waits. */
  readonly hidden: boolean;
}

/**
 * The nav's update notice. AppyTappy Setup checks once when it opens; a newer release shows here and installs only
 * after the user agrees, since the update closes AppyTappy Setup and the background part. A failed check shows
 * nothing, since the next opening checks again.
 */
export function UpdateNotice({ busy, hidden }: Props) {
  const [state, dispatch] = useReducer(nextUpdate, NO_UPDATE);
  const { control, refs } = useQuestion<HTMLButtonElement>(state.kind === 'asking');
  useEffect(() => {
    background(
      checkForUpdate().then((offer) => {
        dispatch({ kind: 'checked', version: offer?.version ?? null });
      }),
    );
  }, []);
  // Work that starts while the question is open would be lost, so the question goes back to the offer.
  useEffect(() => {
    if (busy) {
      dispatch({ kind: 'keep' });
    }
  }, [busy]);

  if (hidden) {
    return null;
  }
  switch (state.kind) {
    case 'none':
      return null;
    case 'offered':
      return (
        <span className="update" role="status">
          <span className={state.problem === null ? undefined : 'warn'}>
            {state.problem ?? `AppyTappy ${state.version} is available.`}
          </span>
          <button
            ref={control}
            type="button"
            className="btn small primary"
            disabled={busy}
            title={busy ? 'Save or leave what you are working on first.' : undefined}
            onClick={() => {
              dispatch({ kind: 'update' });
            }}
          >
            {state.problem === null ? 'Update' : 'Try again'}
          </button>
        </span>
      );
    case 'asking':
      return (
        <Question
          refs={refs}
          label={`Update to AppyTappy ${state.version}?`}
          keepLabel="Not now"
          actLabel="Update"
          onKeep={() => {
            dispatch({ kind: 'keep' });
          }}
          onAct={() => {
            const { version } = state;
            dispatch({ kind: 'confirm' });
            background(
              installUpdate(version).catch((error: unknown) => {
                dispatch({ kind: 'failed', problem: errorText(error) });
              }),
            );
          }}
        >
          AppyTappy Setup closes, and automations stop and undo their changes while it updates.
        </Question>
      );
    case 'installing':
      return (
        <span className="update" role="status">
          Downloading AppyTappy {state.version}…
        </span>
      );
  }
}
