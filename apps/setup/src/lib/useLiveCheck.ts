import { useCallback, useEffect, useState } from 'react';

import { type Reading, errorText, readValue, setCheck } from './api.ts';
import type { Built, CheckState } from './check.ts';
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
  const [reading, setReading] = useState<Reading | null>(null);
  useEffect(() => {
    if (!built.ok) {
      setState({ kind: 'idle' });
      setReading(null);
      return undefined;
    }
    setState({ kind: 'pending' });
    let current = true;
    const timer = setTimeout(() => {
      setCheck(built.spec)
        .then(() => {
          if (current) {
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

  const readNow = useCallback(async (): Promise<void> => {
    try {
      setReading(await readValue());
    } catch {
      setReading(null);
    }
  }, []);
  useRepeat(readNow, READING_PAUSE_MS, state.kind === 'set');
  return { state, reading };
}
