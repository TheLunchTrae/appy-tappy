// The saved applications and the open windows the picker of a value or reference resolves against.

import { useCallback, useEffect, useState } from 'react';

import { errorText, listAppWindows } from './api.ts';
import { type Look, known, seen } from './picker.ts';

/**
 * The newest look at the saved applications and the open windows. One is made when the picker opens and whenever
 * `refresh` is called, and each asks once, so its applications, windows and fits agree.
 */
export function useAppWindows(): { readonly look: Look; readonly refresh: () => void } {
  const [look, setLook] = useState<Look>({ kind: 'looking', before: null });
  const [again, setAgain] = useState(0);
  useEffect(() => {
    // An answer to a look that another one replaced is dropped.
    let current = true;
    setLook((last) => ({ kind: 'looking', before: known(last) }));
    listAppWindows()
      .then((answer) => {
        if (current) {
          setLook({ kind: 'seen', seen: seen(answer) });
        }
      })
      .catch((error: unknown) => {
        if (current) {
          setLook((last) => ({ kind: 'failed', error: errorText(error), before: known(last) }));
        }
      });
    return () => {
      current = false;
    };
  }, [again]);
  const refresh = useCallback((): void => {
    setAgain((count) => count + 1);
  }, []);
  return { look, refresh };
}
