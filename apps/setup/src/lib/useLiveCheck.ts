// The check the capture screen sets up on the live frame, and what it reads.

import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from 'react';

import type { EditPreview } from './bindings.ts';
import { errorText, readValue } from './api.ts';
import type { Built, CheckState } from './check.ts';
import {
  afterRead,
  type InForce,
  type LiveRead,
  type LiveReads,
  liveReads,
  NOTHING_READ,
  type ReadAnswer,
} from './liveCheck.ts';
import { useRepeat } from './repeat.ts';
import { ask } from './request.ts';

/** How long the check waits after the last change before it is tried. */
const CHECK_DELAY_MS = 250;
/** How long the live reading waits between reads. */
const READING_PAUSE_MS = 120;

/** The trial being set up on the live window. */
export interface LiveCheck {
  readonly state: CheckState;
  /** What saving the edit that is set would do; null for any other check and until one is set. */
  readonly preview: EditPreview | null;
  /** What the trial reads on the live window, for {@link useLiveRead}. */
  readonly reads: LiveReads;
}

/** What `reads` shows now: the component that calls it renders again with each new reading, and no other does. */
export function useLiveRead(reads: LiveReads): LiveRead {
  return useSyncExternalStore(reads.subscribe, reads.shown);
}

/**
 * What the capture screen tries on the live window, or what it still needs. Trying a saved value's edit answers
 * what saving it would do; trying a check that is being set up answers null. Its `key` names what is tried by its
 * content, so two trials of the same spec are one trial however often the draft they are built from changes.
 */
export type Trial =
  | { readonly ok: true; readonly key: string; readonly attempt: () => Promise<EditPreview | null> }
  | { readonly ok: false; readonly next: string };

/** How a trial is tried: the characters of a reference, a check being set up, or a saved value's edit. */
type TrialKind = 'characters' | 'check' | 'edit';

/**
 * The trial of `built` on the live window as a `kind` of trial, or what `built` still needs. `attempt` tries it:
 * `set`, which sets a check and answers nothing, or a try that answers what saving would do.
 */
export function trialOf<Spec>(
  kind: TrialKind,
  built: Built<Spec>,
  attempt: { readonly set: (spec: Spec) => Promise<void> } | ((spec: Spec) => Promise<EditPreview>),
): Trial {
  if (!built.ok) {
    return built;
  }
  const { spec } = built;
  const key = `${kind}:${JSON.stringify(spec)}`;
  if (typeof attempt === 'function') {
    return { ok: true, key, attempt: async () => attempt(spec) };
  }
  return {
    ok: true,
    key,
    attempt: async () => {
      await attempt.set(spec);
      return null;
    },
  };
}

/**
 * Tries `trial` on the live window once what it tries has not changed for a moment, and reads the window with it
 * while it is set. A trial with the same key as the one before is the same trial: an edit of the draft that leaves
 * the spec as it was, such as an empty box added or moved, tries nothing again, even after it failed: the Rust side
 * judges the spec against the saved files and the session's screenshots and recording, which such an edit leaves as
 * they were. It is tried again when `again`
 * changes, which says that the same trial may answer otherwise now, as an edit does once the window has another
 * size.
 */
export function useLiveCheck(trial: Trial, again: string | null): LiveCheck {
  const [state, setState] = useState<CheckState>({ kind: 'idle' });
  // The window is read several times a second, so what it reads is no state of the screen that calls this.
  const [reads] = useState(liveReads);
  const [preview, setPreview] = useState<EditPreview | null>(null);
  // The number of the newest trial, a check or a saved value's edit, and of the one the Rust side reads with: see
  // `afterRead`. A trial tried again takes a new number too.
  const tried = useRef(0);
  const inForce = useRef<InForce>(null);
  // The effect below runs when the trial's key changes, not for each new trial with the same key, so it reads the
  // trial through a ref.
  const newest = useRef(trial);
  newest.current = trial;
  const key = trial.ok ? trial.key : null;
  useEffect(() => {
    const trial = newest.current;
    tried.current += 1;
    inForce.current = null;
    const number = tried.current;
    if (!trial.ok) {
      setState({ kind: 'idle' });
      reads.change(() => NOTHING_READ);
      setPreview(null);
      return undefined;
    }
    setState({ kind: 'pending' });
    reads.change((shown) => (shown.problem === null ? shown : { ...shown, problem: null }));
    return ask(
      trial.attempt,
      (answer) => {
        inForce.current = number;
        setPreview(answer);
        setState({ kind: 'set' });
      },
      (error) => {
        setPreview(null);
        setState({ kind: 'error', message: errorText(error) });
      },
      CHECK_DELAY_MS,
    );
  }, [key, again, reads]);

  const readNow = useCallback(async (): Promise<void> => {
    const asked = inForce.current;
    let answer: ReadAnswer;
    try {
      answer = { ok: true, reading: await readValue() };
    } catch (error: unknown) {
      answer = { ok: false, problem: errorText(error) };
    }
    const now = inForce.current;
    reads.change((shown) => afterRead(shown, asked, now, answer));
  }, [reads]);
  useRepeat(readNow, READING_PAUSE_MS, state.kind === 'set');
  return { state, preview, reads };
}
