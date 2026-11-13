// The saved applications and the open windows the picker of a value or reference resolves against.

import { useCallback, useEffect, useState } from 'react';

import { errorText, listAppWindows } from './api.ts';
import { type Look, known, seen } from './picker.ts';
import { ask } from './request.ts';

/**
 * The newest look at the saved applications and the open windows. One is made when the picker opens and whenever
 * `refresh` is called, and each asks once, so its applications, windows and fits agree.
 */
export function useAppWindows(): { readonly look: Look; readonly refresh: () => void } {
  const [look, setLook] = useState<Look>({ kind: 'looking', before: null });
  const [again, setAgain] = useState(0);
  useEffect(() => {
    setLook((last) => ({ kind: 'looking', before: known(last) }));
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
    setAgain((count) => count + 1);
  }, []);
  return { look, refresh };
}
