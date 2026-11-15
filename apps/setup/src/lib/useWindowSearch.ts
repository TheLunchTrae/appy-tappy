// The look for the window of the application a saved value or reference belongs to, and the capture it starts.

import { useCallback, useEffect, useMemo, useReducer, useState } from 'react';

import { captureEnded, errorText } from './api.ts';
import type { WindowChoice } from './bindings.ts';
import { type PickerApp, known, pickerStep } from './picker.ts';
import { background } from './repeat.ts';
import { ask } from './request.ts';
import { useAppWindows } from './useAppWindows.ts';
import {
  type WindowAbsence,
  ENDED_ASK_PAUSE_MS,
  NO_CAPTURE,
  endedAsks,
  fallbackWarning,
  searchStep,
  windowAbsence,
} from './windowSearch.ts';

/**
 * Looks for the window of the saved application `app` when the flow opens and on `refresh`, and captures it with
 * `pick` once it is found: the only window that fits at once, and one of several when the user chooses it. A
 * capture whose window closed is looked for again the same way.
 */
export function useWindowSearch(
  app: string,
  pick: (window: number) => Promise<void>,
): {
  /** What shows in place of the window, or null once it is captured. */
  readonly absence: WindowAbsence | null;
  /** The application as the newest look that answered read it, or null before one did or when it is gone. */
  readonly found: PickerApp | null;
  /** The anti-cheat products that ran at the newest look that answered. */
  readonly antiCheat: readonly string[];
  /** What to say of a captured window that was taken only as the fallback, or null. */
  readonly fallback: string | null;
  /**
   * Hears a sign that the captured window may have closed: its live view failed, a screenshot was refused, or a
   * recording stopped.
   */
  readonly windowFailed: () => void;
  readonly refresh: () => void;
  readonly choose: (window: WindowChoice) => void;
  /** Looks again from the start, once the flow stopped its capture. */
  readonly restart: () => void;
} {
  const { look, refresh: lookAgain } = useAppWindows();
  const [capture, send] = useReducer(searchStep, NO_CAPTURE);
  const step = useMemo(() => pickerStep({ locked: true, chosen: app, look, picked: null }), [app, look]);
  useEffect(() => {
    send({ kind: 'looked', step });
  }, [step]);

  const starting = capture.kind === 'starting' ? capture.window.handle : null;
  useEffect(
    () =>
      starting === null
        ? undefined
        : ask(
            async () => pick(starting),
            () => {
              send({ kind: 'started' });
            },
            (error) => {
              send({ kind: 'refused', error: errorText(error) });
            },
          ),
    [starting, pick],
  );

  const seen = known(look);
  const found = seen?.apps.find((each) => each.id === app) ?? null;
  const refresh = useCallback((): void => {
    send({ kind: 'refreshed' });
    lookAgain();
  }, [lookAgain]);
  const severalFallback = step.kind === 'several' && step.fallback;
  const choose = useCallback(
    (window: WindowChoice): void => {
      send({ kind: 'chose', window, fallback: severalFallback });
    },
    [severalFallback],
  );
  const running = capture.kind === 'running' ? capture.window.handle : null;
  const [askEnded] = useState(() => endedAsks(captureEnded, ENDED_ASK_PAUSE_MS, Date.now));
  const windowFailed = useCallback((): void => {
    if (running === null) {
      return;
    }
    background(
      askEnded(() => {
        send({ kind: 'ended', window: running });
        lookAgain();
      }),
    );
  }, [running, askEnded, lookAgain]);
  const restart = useCallback((): void => {
    send({ kind: 'stopped' });
    lookAgain();
  }, [lookAgain]);
  return {
    absence: windowAbsence(step, capture, found?.name ?? null),
    found,
    antiCheat: seen?.antiCheat ?? [],
    fallback: fallbackWarning(capture, found?.name ?? 'The application'),
    windowFailed,
    refresh,
    choose,
    restart,
  };
}
