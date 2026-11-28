// The saved applications and the open windows the picker of a value or reference resolves against.

import { useCallback, useEffect, useState } from 'react';

import { errorText, listAppWindows } from './api.ts';
import { type Look, known, seen } from './picker.ts';
import { ask } from './request.ts';

/** How a look shows while it is under way: as one, or not at all, keeping what the last one found. */
type LookShows = 'looking' | 'quietly';

/**
 * The newest look at the saved applications and the open windows. One is made when the picker opens and whenever
 * `refresh` or `refreshQuietly` is called, and each asks once, so its applications, windows and fits agree. A quiet
 * look keeps the last answer on show until it answers, so what shows does not change while nothing changed.
 */
export function useAppWindows(): {
  readonly look: Look;
  readonly refresh: () => void;
  readonly refreshQuietly: () => void;
} {
  const [look, setLook] = useState<Look>({ kind: 'looking', before: null });
  const [again, setAgain] = useState<{ readonly count: number; readonly shows: LookShows }>({
    count: 0,
    shows: 'looking',
  });
  useEffect(() => {
    if (again.shows === 'looking') {
      setLook((last) => ({ kind: 'looking', before: known(last) }));
    }
    // An answer to a look that another one replaced is dropped.
    return ask(
      listAppWindows,
      (answer) => {
        setLook({ kind: 'seen', seen: seen(answer) });
      },
      (error) => {
        setLook((last) => ({ kind: 'failed', error: errorText(error), before: known(last) }));
      },
    );
  }, [again]);
  const refresh = useCallback((): void => {
    setAgain(({ count }) => ({ count: count + 1, shows: 'looking' }));
  }, []);
  const refreshQuietly = useCallback((): void => {
    setAgain(({ count }) => ({ count: count + 1, shows: 'quietly' }));
  }, []);
  return { look, refresh, refreshQuietly };
}
