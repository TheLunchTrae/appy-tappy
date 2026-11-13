// What the capture screen's two recordings share: the recording's state kept current, its errors, and its commands.

import { useCallback, useEffect, useReducer, useState } from 'react';

import { errorText, stopRecording } from './api.ts';
import { background, useRepeat } from './repeat.ts';
import { useNewest } from './request.ts';
import { ownTicket } from './ticket.ts';

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
  /**
   * Reads the recording's state now, as a start does after it answered. A failure is a failed read, as a poll's is,
   * and never the command's.
   */
  readonly readState: () => Promise<void>;
  /** Shows no recording and no failed read, once a discard forgot it. */
  readonly forget: () => void;
  /**
   * Runs a start, stop, or discard, and answers whether it worked. It makes the state reads sent before it stale,
   * and its failure is the command error, which its success clears.
   */
  readonly command: (work: () => Promise<void>) => Promise<boolean>;
  /**
   * Hears that a start answered `recording`, the recording's number, which the component then stops by. Its state
   * is read until a read answers.
   */
  readonly started: (recording: number) => void;
  /** Stops the recording the component started, if it started one. */
  readonly stop: () => Promise<void>;
}

/**
 * What the page knows of its recording: the view the newest read answered, whether a start answered with no read
 * since, which leaves a recording the owner runs with no view yet, and why the newest read failed, until one
 * answers.
 */
interface Known<View> {
  readonly view: View | null;
  readonly unread: boolean;
  readonly readError: string | null;
}

/** What changes what the page knows of its recording. */
type Learned<View> =
  | { readonly kind: 'started' }
  | { readonly kind: 'read'; readonly view: View | null }
  | { readonly kind: 'readFailed'; readonly error: string }
  | { readonly kind: 'forgot' };

/** Nothing known of a recording. */
const NOTHING_KNOWN: Known<never> = { view: null, unread: false, readError: null };

/**
 * What the page knows of its recording after `learned`. A read that fails keeps the view and the wait for a read: a
 * start that answered worked, and the read after it is a read like any poll, whose failure the next read that
 * answers clears.
 */
export function learn<View>(known: Known<View>, learned: Learned<View>): Known<View> {
  switch (learned.kind) {
    case 'started':
      return { ...known, unread: true };
    case 'read':
      return { view: learned.view, unread: false, readError: null };
    case 'readFailed':
      return { ...known, readError: learned.error };
    case 'forgot':
      return NOTHING_KNOWN;
  }
}

/**
 * Whether the recording's state is read again and again: until a read answers after a start, so a recording whose
 * first read failed still shows, and while `live` says its view still changes.
 */
export function keepsReading<View>(known: Known<View>, live: (view: View) => boolean): boolean {
  return known.unread || (known.view !== null && live(known.view));
}

/**
 * A recording whose state `read` answers, kept current while `live` says its view still changes. Leaving the
 * component that uses it stops the recording it started, since it samples the window until it stops, by the number
 * its start answered (`ownTicket`), so the stop never stops a recording that took its place. Both must stay the same
 * functions.
 */
export function useRecorder<View>(read: () => Promise<View | null>, live: (view: View) => boolean): Recorder<View> {
  const [known, tell] = useReducer(learn<View>, NOTHING_KNOWN);
  const { view } = known;
  // Each error clears only when what set it next succeeds: a start, stop or discard, or reading the recording's
  // state.
  const [commandError, setCommandError] = useState<string | null>(null);
  // Each start, stop, and discard makes the polls sent before it stale.
  const turn = useNewest();
  const [own] = useState(() =>
    ownTicket((recording) => {
      background(stopRecording(recording));
    }),
  );

  useEffect(() => {
    own.enter();
    return () => {
      own.leave();
    };
  }, [own]);

  const poll = useCallback(async (): Promise<void> => {
    const current = turn.join();
    try {
      const state = await read();
      if (current()) {
        tell({ kind: 'read', view: state });
      }
    } catch (error: unknown) {
      tell({ kind: 'readFailed', error: errorText(error) });
    }
  }, [turn, read]);
  const readState = useCallback(async (): Promise<void> => {
    try {
      tell({ kind: 'read', view: await read() });
    } catch (error: unknown) {
      tell({ kind: 'readFailed', error: errorText(error) });
    }
  }, [read]);
  useRepeat(poll, RECORDING_PAUSE_MS, keepsReading(known, live));

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
  const show = useCallback((shown: View | null) => {
    tell({ kind: 'read', view: shown });
  }, []);
  const forget = useCallback(() => {
    tell({ kind: 'forgot' });
  }, []);
  const started = useCallback(
    (recording: number) => {
      own.started(recording);
      tell({ kind: 'started' });
    },
    [own],
  );
  const stop = useCallback(async (): Promise<void> => {
    const recording = own.held();
    if (recording !== null) {
      await stopRecording(recording);
    }
  }, [own]);
  return {
    view,
    commandError,
    pollError: known.readError,
    clearCommandError,
    show,
    readState,
    forget,
    command,
    started,
    stop,
  };
}
