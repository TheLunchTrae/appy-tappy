import { useCallback, useEffect, useState } from 'react';

import { type AppFile, errorText, listApps } from './api.ts';
import { background } from './repeat.ts';

/** A saved application's file, for its references and the values that read with them. */
export function useAppFile(app: string): {
  readonly file: AppFile | null;
  readonly error: string | null;
  /** Reads `app`'s file again, such as after a save added to it. */
  readonly load: (app: string) => Promise<void>;
} {
  const [file, setFile] = useState<AppFile | null>(null);
  const [error, setError] = useState<string | null>(null);
  const load = useCallback(async (app: string): Promise<void> => {
    try {
      const found = (await listApps()).find((summary) => summary.id === app);
      setFile(found?.file ?? null);
      setError(found === undefined || found.file !== null ? null : found.error);
    } catch (error: unknown) {
      setError(errorText(error));
    }
  }, []);
  useEffect(() => {
    background(load(app));
  }, [load, app]);
  return { file, error, load };
}
