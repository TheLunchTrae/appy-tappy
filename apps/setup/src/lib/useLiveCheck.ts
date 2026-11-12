// The check the capture screen sets up on the live frame, and what it reads.

import { useCallback, useEffect, useRef, useState } from 'react';

import { errorText, readValue, setCheck } from './api.ts';
import type { Built, CheckState } from './check.ts';
import { afterRead, type InForce, type LiveRead, type ReadAnswer } from './liveCheck.ts';
import { useRepeat } from './repeat.ts';

/** How long the check waits after the last change before it is tried. */
const CHECK_DELAY_MS = 250;
/** How long the live reading waits between reads. */
const READING_PAUSE_MS = 120;

/** What the check being set up reads on the live window. */
export interface LiveCheck extends LiveRead {
  readonly state: CheckState;
}

const NOTHING_READ: LiveRead = { reading: null, problem: null };

/**
 * Tries `built`'s check on the live window once it has not changed for a moment, and reads the window with it while
 * it is set.
 */
export function useLiveCheck(built: Built): LiveCheck {
  const [state, setState] = useState<CheckState>({ kind: 'idle' });
  const [read, setRead] = useState<LiveRead>(NOTHING_READ);
  // The number of the newest check tried, and of the one the Rust side reads with: see `afterRead`.
  const tried = useRef(0);
  const inForce = useRef<InForce>(null);
  useEffect(() => {
    tried.current += 1;
    inForce.current = null;
    const number = tried.current;
    if (!built.ok) {
      setState({ kind: 'idle' });
      setRead(NOTHING_READ);
      return undefined;
    }
    setState({ kind: 'pending' });
    setRead((shown) => ({ ...shown, problem: null }));
    let current = true;
    const timer = setTimeout(() => {
      setCheck(built.spec)
        .then(() => {
          if (current) {
            inForce.current = number;
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
    const asked = inForce.current;
    let answer: ReadAnswer;
    try {
      answer = { ok: true, reading: await readValue() };
    } catch (error: unknown) {
      answer = { ok: false, problem: errorText(error) };
    }
    const now = inForce.current;
    setRead((shown) => afterRead(shown, asked, now, answer));
  }, []);
  useRepeat(readNow, READING_PAUSE_MS, state.kind === 'set');
  return { state, ...read };
}
