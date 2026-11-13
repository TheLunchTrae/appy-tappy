// Recording a value's region for its looks on the capture screen: its state, and its commands.

import { useCallback, useEffect, useRef, useState } from 'react';

import type { LookRecordingView } from './bindings.ts';
import { discardRecording, errorText, lookRecordingState, startLookRecording, stopRecording } from './api.ts';
import type { Rect } from './region.ts';
import { background, useRepeat } from './repeat.ts';
import { useNewest } from './request.ts';

/** How long the recording's state waits between reads, while it records. */
const RECORDING_PAUSE_MS = 250;

/** What a recording of looks records: a region drawn on a screenshot, and how far a look may be from another. */
interface Recordable {
  readonly region: Rect;
  readonly tolerance: number;
  readonly drawnOn: number;
}

/**
 * The session's recording of looks, kept current while it records. Each screenshot it keeps goes to `adopt` once,
 * which must stay the same function. Leaving the component that uses it stops the recording, since it samples the
 * window until it stops.
 */
export function useLookRecording(adopt: (index: number) => void): {
  readonly view: LookRecordingView | null;
  /** Why the last start, stop, or discard failed, or else why the recording's state could not be read. */
  readonly error: string | null;
  /** Starts a new recording of `settings`; `onStarted` runs once it has started. */
  readonly start: (settings: Recordable, onStarted: () => void) => Promise<void>;
  /** Stops recording and answers the stopped recording, or null when it could not stop. */
  readonly stop: () => Promise<LookRecordingView | null>;
  /** Forgets the recording, if there is one. The screenshots it kept stay. */
  readonly discard: () => Promise<void>;
} {
  const [view, setView] = useState<LookRecordingView | null>(null);
  // Each error clears only when what set it next succeeds: a start, stop or
  // discard, or reading the recording's state.
  const [commandError, setCommandError] = useState<string | null>(null);
  const [pollError, setPollError] = useState<string | null>(null);
  // Each start, stop, and discard begins a turn, so a poll sent before one is ignored.
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
      const state = await lookRecordingState();
      if (current()) {
        setView(state);
        setPollError(null);
      }
    } catch (error: unknown) {
      setPollError(errorText(error));
    }
  }, [turn]);
  useRepeat(poll, RECORDING_PAUSE_MS, view?.recording === true);

  // Hands each kept screenshot over once. The session never gives an index twice.
  const adopted = useRef<Set<number>>(new Set());
  const kept = view?.kept;
  useEffect(() => {
    for (const index of kept ?? []) {
      if (!adopted.current.has(index)) {
        adopted.current.add(index);
        adopt(index);
      }
    }
  }, [kept, adopt]);

  const start = async (settings: Recordable, onStarted: () => void): Promise<void> => {
    turn.begin();
    try {
      await startLookRecording(settings.region, settings.tolerance, settings.drawnOn);
      setCommandError(null);
      onStarted();
      setView(await lookRecordingState());
    } catch (error: unknown) {
      setCommandError(errorText(error));
    }
  };

  const stop = async (): Promise<LookRecordingView | null> => {
    turn.begin();
    try {
      await stopRecording();
      const stopped = await lookRecordingState();
      setView(stopped);
      setCommandError(null);
      return stopped;
    } catch (error: unknown) {
      setCommandError(errorText(error));
      return null;
    }
  };

  const discard = async (): Promise<void> => {
    if (view === null) {
      return;
    }
    turn.begin();
    try {
      await discardRecording();
      setView(null);
      setCommandError(null);
      setPollError(null);
    } catch (error: unknown) {
      setCommandError(errorText(error));
    }
  };

  return { view, error: commandError ?? pollError, start, stop, discard };
}
