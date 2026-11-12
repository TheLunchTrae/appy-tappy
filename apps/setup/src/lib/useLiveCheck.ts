// The check the capture screen sets up on the live frame, and what it reads.

import { useCallback, useEffect, useRef, useState } from 'react';

import type { Reading } from './bindings.ts';
import { errorText, readValue, setCheck } from './api.ts';
import { type Built, type CheckState, type LiveReading, readingOf } from './check.ts';
import { useRepeat } from './repeat.ts';

/** How long the check waits after the last change before it is tried. */
const CHECK_DELAY_MS = 250;
/** How long the live reading waits between reads. */
const READING_PAUSE_MS = 120;

/**
 * Tries `built`'s check on the live window once it has not changed for a moment, and reads the window with it while
 * it is set.
 */
export function useLiveCheck(built: Built): { readonly state: CheckState; readonly reading: Reading | null } {
  const [state, setState] = useState<CheckState>({ kind: 'idle' });
  const [read, setRead] = useState<LiveReading | null>(null);
  // The check the owner reads with: the one whose set-up last went through.
  const checked = useRef<Built | null>(null);
  useEffect(() => {
    if (!built.ok) {
      setState({ kind: 'idle' });
      return undefined;
    }
    setState({ kind: 'pending' });
    let current = true;
    const timer = setTimeout(() => {
      setCheck(built.spec)
        .then(() => {
          if (current) {
            checked.current = built;
            setState({ kind: 'set' });
          }
        })
        .catch((error: unknown) => {
          if (current) {
            setState({ kind: 'error', message: errorText(error) });
          }
        });
    }, CHECK_DELAY_MS);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [built]);

  // Each reading is kept with the check the owner read it with, so one that arrives after the check changed is
  // not shown as the new check's.
  const readNow = useCallback(async (): Promise<void> => {
    const of = checked.current;
    if (of === null) {
      return;
    }
    try {
      setRead({ of, reading: await readValue() });
    } catch {
      setRead({ of, reading: null });
    }
  }, []);
  useRepeat(readNow, READING_PAUSE_MS, state.kind === 'set');
  return { state, reading: readingOf(read, built) };
}
