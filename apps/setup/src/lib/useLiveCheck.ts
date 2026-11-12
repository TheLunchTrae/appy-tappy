// The check the capture screen sets up on the live frame, and what it reads.

import { useCallback, useEffect, useState } from 'react';

import type { EditPreview, EditSpec, Reading } from './bindings.ts';
import { errorText, readValue, setCheck, tryEdit } from './api.ts';
import type { Built, CheckState } from './check.ts';
import { useRepeat } from './repeat.ts';

/** How long the check waits after the last change before it is tried. */
const CHECK_DELAY_MS = 250;
/** How long the live reading waits between reads. */
const READING_PAUSE_MS = 120;

/**
 * What the capture screen tries on the live window, or what it still needs. Trying a saved value's edit answers
 * what saving it would do; trying a check that is being set up answers null.
 */
export type Trial = Built<() => Promise<EditPreview | null>>;

/** The trial of `edit`, a saved value's edit, or of `check` while the capture edits no saved value. */
export function trialOf(check: Built, edit: Built<EditSpec> | null): Trial {
  if (edit !== null) {
    const { ok } = edit;
    return ok ? { ok, spec: async () => tryEdit(edit.spec) } : edit;
  }
  if (!check.ok) {
    return check;
  }
  return {
    ok: true,
    spec: async () => {
      await setCheck(check.spec);
      return null;
    },
  };
}

/**
 * Tries `trial` on the live window once it has not changed for a moment, and reads the window with it while it is
 * set. It is tried again when `again` changes: the owner places an edit where the value reads at the window's size
 * when it is tried, so the capture screen passes the live view's size.
 */
export function useLiveCheck(
  trial: Trial,
  again: unknown,
): {
  readonly state: CheckState;
  readonly reading: Reading | null;
  /** What saving the edit that is set would do; null for any other check and until one is set. */
  readonly preview: EditPreview | null;
} {
  const [state, setState] = useState<CheckState>({ kind: 'idle' });
  const [reading, setReading] = useState<Reading | null>(null);
  const [preview, setPreview] = useState<EditPreview | null>(null);
  useEffect(() => {
    if (!trial.ok) {
      setState({ kind: 'idle' });
      setReading(null);
      setPreview(null);
      return undefined;
    }
    setState({ kind: 'pending' });
    let current = true;
    const timer = setTimeout(() => {
      trial
        .spec()
        .then((answer) => {
          if (current) {
            setPreview(answer);
            setState({ kind: 'set' });
          }
        })
        .catch((error: unknown) => {
          if (current) {
            setPreview(null);
            setState({ kind: 'error', message: errorText(error) });
          }
        });
    }, CHECK_DELAY_MS);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [trial, again]);

  const readNow = useCallback(async (): Promise<void> => {
    try {
      setReading(await readValue());
    } catch {
      setReading(null);
    }
  }, []);
  useRepeat(readNow, READING_PAUSE_MS, state.kind === 'set');
  return { state, reading, preview };
}
