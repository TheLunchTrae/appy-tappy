// The saved application file the capture screen's references and values read with.

import { useCallback, useEffect, useState } from 'react';

import type { AppFile } from './bindings.ts';
import { errorText, listApps } from './api.ts';
import { background } from './repeat.ts';

/** A saved application's file, for its references and the values that read with them. */
export function useAppFile(app: string): {
  readonly file: AppFile | null;
  readonly error: string | null;
  /** Reads the file again, such as after a save added to it. */
  readonly reload: () => Promise<void>;
} {
  const [file, setFile] = useState<AppFile | null>(null);
  const [error, setError] = useState<string | null>(null);
  const reload = useCallback(async (): Promise<void> => {
    try {
      const found = (await listApps()).find((summary) => summary.id === app);
      setFile(found?.file ?? null);
      setError(found === undefined || found.file !== null ? null : found.error);
    } catch (error: unknown) {
      setError(errorText(error));
    }
  }, [app]);
  useEffect(() => {
    background(reload());
  }, [reload]);
  return { file, error, reload };
}
