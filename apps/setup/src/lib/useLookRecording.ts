// Recording a value's region for its looks on the capture screen: its state, and its commands.

import { useEffect, useRef } from 'react';

import type { LookRecordingView } from './bindings.ts';
import { discardRecording, lookRecordingState, startLookRecording } from './api.ts';
import type { Rect } from './region.ts';
import { useRecorder } from './useRecorder.ts';

/** Whether the recording's state still changes: it records. */
function stillRecords(view: LookRecordingView): boolean {
  return view.recording;
}

/** What a recording of looks records: a region drawn on a screenshot, and how far a look may be from another. */
interface Recordable {
  readonly region: Rect;
  readonly tolerance: number;
  readonly drawnOn: number;
}

/**
 * The session's recording of looks, kept current while it records. Each screenshot it keeps goes to `adopt` once,
 * which must stay the same function. Leaving the component that uses it stops the recording it started, since it
 * samples the window until it stops.
 */
export function useLookRecording(adopt: (index: number) => void): {
  readonly view: LookRecordingView | null;
  /** Why the last start, stop, or discard failed, or else why the recording's state could not be read. */
  readonly error: string | null;
  /** Starts a new recording of `settings`; `onStarted` runs once it has started. */
  readonly start: (settings: Recordable, onStarted: () => void) => Promise<void>;
  /** Stops the recording it started and answers the stopped recording, or null when it could not stop. */
  readonly stop: () => Promise<LookRecordingView | null>;
  /** Forgets the recording, if there is one. The screenshots it kept stay. */
  readonly discard: () => Promise<void>;
} {
  const recorder = useRecorder(lookRecordingState, stillRecords);
  const { view, command, show, forget, started } = recorder;

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
    await command(async () => {
      started(await startLookRecording(settings.region, settings.tolerance, settings.drawnOn));
      onStarted();
      show(await lookRecordingState());
    });
  };

  const stop = async (): Promise<LookRecordingView | null> => {
    let stopped: LookRecordingView | null = null;
    const worked = await command(async () => {
      await recorder.stop();
      stopped = await lookRecordingState();
      show(stopped);
    });
    return worked ? stopped : null;
  };

  const discard = async (): Promise<void> => {
    if (view !== null) {
      await command(async () => {
        await discardRecording();
        forget();
      });
    }
  };

  return { view, error: recorder.commandError ?? recorder.pollError, start, stop, discard };
}
