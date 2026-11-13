// The check the capture screen sets up on the live frame, and what it reads.

import { useCallback, useEffect, useRef, useState } from 'react';

import type { CharactersSpec, EditPreview, TrialSpec } from './bindings.ts';
import { errorText, readValue, setCharacters, setCheck } from './api.ts';
import type { Built, CheckState } from './check.ts';
import { afterRead, type InForce, type LiveRead, type ReadAnswer } from './liveCheck.ts';
import { useRepeat } from './repeat.ts';

/** How long the check waits after the last change before it is tried. */
const CHECK_DELAY_MS = 250;
/** How long the live reading waits between reads. */
const READING_PAUSE_MS = 120;

/** What the trial being set up reads on the live window. */
export interface LiveCheck extends LiveRead {
  readonly state: CheckState;
  /** What saving the edit that is set would do; null for any other check and until one is set. */
  readonly preview: EditPreview | null;
}

const NOTHING_READ: LiveRead = { reading: null, problem: null };

/**
 * What the capture screen tries on the live window, or what it still needs. Trying a saved value's edit answers
 * what saving it would do; trying a check that is being set up answers null.
 */
export type Trial = Built<() => Promise<EditPreview | null>>;

/** The trial of `built` that `attempt` makes on the live window, or what `built` still needs. */
export function trialOf<Spec>(built: Built<Spec>, attempt: (spec: Spec) => Promise<EditPreview | null>): Trial {
  if (!built.ok) {
    return built;
  }
  const { spec } = built;
  return { ok: true, spec: async () => attempt(spec) };
}

/**
 * Sets the reference `spec` teaches as the check the live window is read with. It reads only the characters
 * `reads`, those of the value the reference is made for, or every character it is taught for null.
 */
export async function tryCharacters(spec: CharactersSpec, reads: string | null): Promise<null> {
  await setCharacters(spec, reads);
  return null;
}

/** Sets the value `trial` would save as the check the live window is read with. */
export async function tryCheck(trial: TrialSpec): Promise<null> {
  await setCheck(trial);
  return null;
}

/**
 * Tries `trial` on the live window once it has not changed for a moment, and reads the window with it while it is
 * set. It is tried again when `again` changes, which says that the same trial may answer otherwise now, as an edit
 * does once the window has another size.
 */
export function useLiveCheck(trial: Trial, again: string | null): LiveCheck {
  const [state, setState] = useState<CheckState>({ kind: 'idle' });
  const [read, setRead] = useState<LiveRead>(NOTHING_READ);
  const [preview, setPreview] = useState<EditPreview | null>(null);
  // The number of the newest trial, a check or a saved value's edit, and of the one the Rust side reads with: see
  // `afterRead`. A trial tried again takes a new number too.
  const tried = useRef(0);
  const inForce = useRef<InForce>(null);
  useEffect(() => {
    tried.current += 1;
    inForce.current = null;
    const number = tried.current;
    if (!trial.ok) {
      setState({ kind: 'idle' });
      setRead(NOTHING_READ);
      setPreview(null);
      return undefined;
    }
    setState({ kind: 'pending' });
    setRead((shown) => ({ ...shown, problem: null }));
    let current = true;
    const timer = setTimeout(() => {
      trial
        .spec()
        .then((answer) => {
          if (current) {
            inForce.current = number;
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
  return { state, ...read, preview };
}
