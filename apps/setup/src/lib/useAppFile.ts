// The saved application file the capture screen's references and values read with.

import { useCallback, useEffect, useState } from 'react';

import type { AppFile } from './bindings.ts';
import { appFile, errorText } from './api.ts';
import { background } from './repeat.ts';
import { useNewest } from './request.ts';

/** A saved application's file, for its references and the values that read with them. */
export function useAppFile(app: string): {
  readonly file: AppFile | null;
  readonly error: string | null;
  /** Reads the file again, such as after a save added to it, and answers it, or null when it cannot be read. */
  readonly reload: () => Promise<AppFile | null>;
} {
  const [file, setFile] = useState<AppFile | null>(null);
  const [error, setError] = useState<string | null>(null);
  const { begin } = useNewest();
  const reload = useCallback(async (): Promise<AppFile | null> => {
    // A read that a newer one overtook still answers its caller, and leaves what shows to the newer one.
    const current = begin();
    try {
      const file = await appFile(app);
      if (current()) {
        setFile(file);
        setError(null);
      }
      return file;
    } catch (error: unknown) {
      // The file cannot be read, so the screen has none, and says why.
      if (current()) {
        setFile(null);
        setError(errorText(error));
      }
      return null;
    }
  }, [app, begin]);
  useEffect(() => {
    background(reload());
  }, [reload]);
  return { file, error, reload };
}
