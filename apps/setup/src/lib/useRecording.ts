// Recording a region on the capture screen: its readings, their pictures, and its commands.

import { useCallback, useEffect, useRef, useState } from 'react';

import type { ExampleSpec, PinDef, RecordingSettings, RecordingView } from './bindings.ts';
import { discardRecording, errorText, recordedReading, recordingState, startRecording, stopRecording } from './api.ts';
import type { Picture } from './pixels.ts';
import { background, useRepeat } from './repeat.ts';
import { useNewest } from './request.ts';

/** How long the recording's state waits between reads, while it records or text is still being read. */
const RECORDING_PAUSE_MS = 250;
/** The widest a recorded reading is asked for: a region is small, and review shows it a few times larger. */
const CROP_WIDTH = 480;

/**
 * What a recording records: a region drawn on a screenshot of `app`'s window, read with the characters' color and
 * tolerance, in that place alone or wherever the window shows text like it or like one of the `examples`, the
 * reference's boxes. As the window changes size the region stays at `pin`, or at the side it is nearest for null.
 */
type Recordable = Omit<RecordingSettings, 'windowSize'> & {
  readonly app: string;
  readonly pin: PinDef | null;
  readonly drawnOn: number;
  readonly examples: readonly ExampleSpec[];
};

/** Each recorded reading's pixels, by its index, for the recording `id`. */
interface Crops {
  readonly id: number;
  readonly pictures: ReadonlyMap<number, Picture>;
}

/** No recording's pixels. */
const NO_CROPS: Crops = { id: -1, pictures: new Map() };

/**
 * The session's recording, kept current while it records or its text is still being read, with each reading's
 * pixels. Leaving the component that uses it stops the recording, since it samples the window until it stops.
 */
export function useRecording(): {
  readonly view: RecordingView | null;
  /** Each recorded reading's pixels, by its index, once they arrive. */
  readonly crops: ReadonlyMap<number, Picture>;
  /** Why the last start, stop, or discard failed, until the next one succeeds. */
  readonly commandError: string | null;
  /** Why the recording's state could not be read, until it next is. */
  readonly pollError: string | null;
  readonly clearCommandError: () => void;
  /**
   * Starts a new recording of `settings`, or with `more` goes on with the stopped one; `onStarted` runs once it has
   * started. It is a callback, not an answer like `stop`'s, so it runs before the recording's state is read and still
   * runs when that read fails.
   */
  readonly start: (settings: Recordable, more: boolean, onStarted: () => void) => Promise<void>;
  /** Stops recording, keeping its readings; true once it stopped. */
  readonly stop: () => Promise<boolean>;
  /** Forgets the recording and its readings; true once it is gone. */
  readonly discard: () => Promise<boolean>;
} {
  const [view, setView] = useState<RecordingView | null>(null);
  // Each error clears only when what set it next succeeds: a start, stop or
  // discard, or reading the recording's state.
  const [commandError, setCommandError] = useState<string | null>(null);
  const [pollError, setPollError] = useState<string | null>(null);
  // Each start, stop, and discard begins a turn, so a poll sent before one is ignored.
  const turn = useNewest();
  // Each recorded reading's pixels, for the recording they belong to.
  const [crops, setCrops] = useState<Crops>(NO_CROPS);

  useEffect(
    () => () => {
      background(stopRecording());
    },
    [],
  );

  const poll = useCallback(async (): Promise<void> => {
    const current = turn.join();
    try {
      const state = await recordingState();
      if (current()) {
        setView(state);
        setPollError(null);
      }
    } catch (error: unknown) {
      setPollError(errorText(error));
    }
  }, [turn]);
  const live = view !== null && (view.recording || view.readings.some((reading) => reading.text.kind === 'pending'));
  useRepeat(poll, RECORDING_PAUSE_MS, live);

  // Fetches the pixels of each reading that has none yet, once.
  const asked = useRef<{ id: number; readings: Set<number> }>({ id: -1, readings: new Set() });
  const readingCount = view?.readings.length ?? 0;
  const recordingId = view?.id ?? null;
  useEffect(() => {
    if (recordingId === null) {
      return;
    }
    if (asked.current.id !== recordingId) {
      asked.current = { id: recordingId, readings: new Set() };
    }
    for (let index = 0; index < readingCount; index += 1) {
      if (asked.current.readings.has(index)) {
        continue;
      }
      asked.current.readings.add(index);
      background(
        recordedReading(index, CROP_WIDTH).then((picture) => {
          if (picture === null) {
            return;
          }
          setCrops((current) => {
            const pictures = current.id === recordingId ? new Map(current.pictures) : new Map<number, Picture>();
            return { id: recordingId, pictures: pictures.set(index, picture) };
          });
        }),
      );
    }
  }, [recordingId, readingCount]);

  // The owner refuses to go on with other settings rather than start over,
  // so kept readings and review edits are never dropped without a discard.
  const start = async (settings: Recordable, more: boolean, onStarted: () => void): Promise<void> => {
    turn.drop();
    try {
      const { app, pin, region, ink, tolerance, drawnOn, scope, examples } = settings;
      await startRecording({ app, region, screenshot: drawnOn, pin, ink, tolerance, scope, examples }, more);
      if (!more) {
        // A new recording replaced the old one, whose readings' pixels go with it.
        setCrops(NO_CROPS);
      }
      setCommandError(null);
      onStarted();
      setView(await recordingState());
    } catch (error: unknown) {
      setCommandError(errorText(error));
    }
  };

  const stop = async (): Promise<boolean> => {
    turn.drop();
    try {
      await stopRecording();
      setView(await recordingState());
      setCommandError(null);
      return true;
    } catch (error: unknown) {
      setCommandError(errorText(error));
      return false;
    }
  };

  const discard = async (): Promise<boolean> => {
    turn.drop();
    try {
      await discardRecording();
      setView(null);
      setCrops(NO_CROPS);
      setCommandError(null);
      setPollError(null);
      return true;
    } catch (error: unknown) {
      setCommandError(errorText(error));
      return false;
    }
  };

  return {
    view,
    crops: crops.id === recordingId ? crops.pictures : new Map<number, Picture>(),
    commandError,
    pollError,
    clearCommandError: () => {
      setCommandError(null);
    },
    start,
    stop,
    discard,
  };
}
