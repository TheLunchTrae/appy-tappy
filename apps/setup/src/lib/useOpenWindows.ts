// The open windows the picker of a value or reference resolves its applications against.

import { useCallback, useEffect, useMemo, useState } from 'react';

import { errorText, listWindows, matchWindows } from './api.ts';
import type { Look, PickerApp } from './picker.ts';

const LOOKING: Look = { kind: 'looking' };

/**
 * The open windows and the ones each of `saved` takes, from one look that is made again when an application's
 * match changes and whenever `refresh` is called.
 */
export function useOpenWindows(saved: readonly PickerApp[]): {
  readonly look: Look;
  /** The anti-cheat products the last look found running, kept while the next one is made. */
  readonly antiCheat: readonly string[];
  readonly refresh: () => void;
} {
  const [look, setLook] = useState<Look>(LOOKING);
  const [antiCheat, setAntiCheat] = useState<readonly string[]>([]);
  const [again, setAgain] = useState(0);
  // What the look depends on, as text, so the same applications listed again do not start another look.
  const matches = useMemo(() => JSON.stringify(saved.map((app) => [app.id, app.window])), [saved]);
  useEffect(() => {
    // An answer to a look that another one replaced is dropped.
    let current = true;
    setLook(LOOKING);
    Promise.all([listWindows(), Promise.all(saved.map(async (app) => matchWindows(app.window)))])
      .then(([list, fits]) => {
        if (current) {
          setAntiCheat(list.antiCheat);
          setLook({
            kind: 'found',
            windows: list.windows,
            fits: new Map(saved.map((app, at) => [app.id, fits[at] ?? []] as const)),
          });
        }
      })
      .catch((error: unknown) => {
        if (current) {
          setLook({ kind: 'failed', error: errorText(error) });
        }
      });
    return () => {
      current = false;
    };
    // `saved` is read as of the render that changed `matches`.
  }, [matches, again]);
  const refresh = useCallback((): void => {
    setAgain((count) => count + 1);
  }, []);
  return { look, antiCheat, refresh };
}
