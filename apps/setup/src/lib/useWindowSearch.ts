// The look for the window of the application a saved value or reference belongs to, and the capture it starts.

import { useCallback, useEffect, useMemo, useReducer } from 'react';

import { errorText } from './api.ts';
import type { WindowChoice } from './bindings.ts';
import { type PickerApp, known, pickerStep } from './picker.ts';
import { ask } from './request.ts';
import { useAppWindows } from './useAppWindows.ts';
import { type WindowAbsence, NO_CAPTURE, searchStep, windowAbsence } from './windowSearch.ts';

/**
 * Looks for the window of the saved application `app` when the flow opens and on `refresh`, and captures it with
 * `pick` once it is found: the only window that fits at once, and one of several when the user chooses it.
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
  const choose = useCallback((window: WindowChoice): void => {
    send({ kind: 'chose', window });
  }, []);
  const restart = useCallback((): void => {
    send({ kind: 'stopped' });
    lookAgain();
  }, [lookAgain]);
  return {
    absence: windowAbsence(step, capture, found?.name ?? null),
    found,
    antiCheat: seen?.antiCheat ?? [],
    refresh,
    choose,
    restart,
  };
}
