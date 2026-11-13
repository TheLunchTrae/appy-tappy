// What the capture screen's two recordings share: the recording's state kept current, its errors, and its commands.

import { useCallback, useEffect, useState } from 'react';

import { errorText, stopRecording } from './api.ts';
import { background, useRepeat } from './repeat.ts';
import { useNewest } from './request.ts';

/** How long a recording's state waits between reads, while it is live. */
const RECORDING_PAUSE_MS = 250;

/** One of the session's recordings, of readings or of looks, as the hook for its kind runs it. */
export interface Recorder<View> {
  readonly view: View | null;
  /** Why the last start, stop, or discard failed, until the next one succeeds. */
  readonly commandError: string | null;
  /** Why the recording's state could not be read, until it next is. */
  readonly pollError: string | null;
  readonly clearCommandError: () => void;
  /** Shows `view` as the recording's state, as a command does with what it read after it. */
  readonly show: (view: View | null) => void;
  /** Shows no recording and no failed read, once a discard forgot it. */
  readonly forget: () => void;
  /**
   * Runs a start, stop, or discard, and answers whether it worked. It makes the state reads sent before it stale,
   * and its failure is the command error, which its success clears.
   */
  readonly command: (work: () => Promise<void>) => Promise<boolean>;
}

/**
 * A recording whose state `read` answers, kept current while `live` says its view still changes. Leaving the
 * component that uses it stops the recording, since it samples the window until it stops. Both must stay the same
 * functions.
 */
export function useRecorder<View>(read: () => Promise<View | null>, live: (view: View) => boolean): Recorder<View> {
  const [view, setView] = useState<View | null>(null);
  // Each error clears only when what set it next succeeds: a start, stop or discard, or reading the recording's
  // state.
  const [commandError, setCommandError] = useState<string | null>(null);
  const [pollError, setPollError] = useState<string | null>(null);
  // Each start, stop, and discard makes the polls sent before it stale.
  const turn = useNewest();

  useEffect(
    () => () => {
      background(stopRecording());
    },
    [],
  );

  const poll = useCallback(async (): Promise<void> => {
    const current = turn.join();
    try {
      const state = await read();
      if (current()) {
        setView(state);
        setPollError(null);
      }
    } catch (error: unknown) {
      setPollError(errorText(error));
    }
  }, [turn, read]);
  useRepeat(poll, RECORDING_PAUSE_MS, view !== null && live(view));

  const command = useCallback(
    async (work: () => Promise<void>): Promise<boolean> => {
      turn.drop();
      try {
        await work();
        setCommandError(null);
        return true;
      } catch (error: unknown) {
        setCommandError(errorText(error));
        return false;
      }
    },
    [turn],
  );
  const clearCommandError = useCallback(() => {
    setCommandError(null);
  }, []);
  const forget = useCallback(() => {
    setView(null);
    setPollError(null);
  }, []);
  return { view, commandError, pollError, clearCommandError, show: setView, forget, command };
}
