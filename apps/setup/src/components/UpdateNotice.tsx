import { useEffect, useReducer, useRef } from 'react';

import { checkForUpdate, downloadUpdate, errorText, installUpdate } from '../lib/api.ts';
import { background } from '../lib/repeat.ts';
import { NO_UPDATE, nextUpdate } from '../lib/update.ts';
import { Question, useQuestion } from './Question.tsx';

interface Props {
  /** Whether installing now would lose work, such as an unsaved value; the install waits until it is saved or left. */
  readonly busy: boolean;
  /** Whether another question has the nav's place; the notice keeps its state while it waits. */
  readonly hidden: boolean;
}

/**
 * The nav's update notice. AppyTappy Setup checks once when it opens; a newer release shows here, downloads when the
 * user asks, and installs only after the user agrees with nothing left unsaved, since the install closes AppyTappy
 * Setup and the background part. A failed check shows nothing, since the next opening checks again.
 */
export function UpdateNotice({ busy, hidden }: Props) {
  const [state, dispatch] = useReducer(nextUpdate, NO_UPDATE);
  const { control, refs } = useQuestion<HTMLButtonElement>(state.kind === 'asking');
  // The notice holds focus while its button gives way to text, and hands it to the next button once one shows.
  const notice = useRef<HTMLSpanElement>(null);
  const following = useRef(false);
  useEffect(() => {
    background(
      checkForUpdate().then((offer) => {
        dispatch({ kind: 'checked', version: offer?.version ?? null });
      }),
    );
  }, []);
  // Work that starts while the question is open would be lost, so the question goes back to the download.
  useEffect(() => {
    if (busy) {
      dispatch({ kind: 'keep' });
    }
  }, [busy]);
  useEffect(() => {
    const focused = document.activeElement;
    if (!following.current || (focused !== document.body && focused !== notice.current)) {
      following.current = false;
      return;
    }
    (control.current ?? notice.current)?.focus();
  }, [state.kind, control]);

  if (hidden) {
    return null;
  }
  const hold = (): void => {
    following.current = true;
    notice.current?.focus();
  };
  switch (state.kind) {
    case 'none':
      return null;
    case 'offered':
      return (
        <span ref={notice} className="update" role="status" tabIndex={-1}>
          <span className={state.problem === null ? undefined : 'warn'}>
            {state.problem ?? `AppyTappy ${state.version} is available.`}
          </span>
          <button
            ref={control}
            type="button"
            className="btn small primary"
            onClick={() => {
              hold();
              dispatch({ kind: 'update' });
              background(
                downloadUpdate()
                  .then((offer) => {
                    dispatch({ kind: 'downloaded', version: offer.version });
                  })
                  .catch((error: unknown) => {
                    dispatch({ kind: 'failed', problem: errorText(error) });
                  }),
              );
            }}
          >
            {state.problem === null ? 'Update' : 'Try again'}
          </button>
        </span>
      );
    case 'downloading':
      return (
        <span ref={notice} className="update" role="status" tabIndex={-1}>
          Downloading AppyTappy {state.version}…
        </span>
      );
    case 'ready':
      return (
        <span ref={notice} className="update" role="status" tabIndex={-1}>
          <span>AppyTappy {state.version} is ready to install.</span>
          <button
            ref={control}
            type="button"
            className="btn small primary"
            disabled={busy}
            title={busy ? 'Save or leave what you are working on first.' : undefined}
            onClick={() => {
              dispatch({ kind: 'install' });
            }}
          >
            Install
          </button>
        </span>
      );
    case 'asking':
      return (
        <Question
          refs={refs}
          label={`Install AppyTappy ${state.version}?`}
          keepLabel="Not now"
          actLabel="Install"
          onKeep={() => {
            dispatch({ kind: 'keep' });
          }}
          onAct={() => {
            const { version } = state;
            following.current = true;
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
        <span ref={notice} className="update" role="status" tabIndex={-1}>
          Installing AppyTappy {state.version}…
        </span>
      );
  }
}
