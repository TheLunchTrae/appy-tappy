// The saved application file the capture screen's references and values read with.

import { useCallback, useEffect, useState } from 'react';

import type { AppFile } from './bindings.ts';
import { appFile, errorText } from './api.ts';
import { background } from './repeat.ts';
import { useNewest } from './request.ts';

/** What the screen shows of a saved application's file: the file, or no file and why it cannot be read. */
export interface AppFileRead {
  readonly file: AppFile | null;
  readonly error: string | null;
}

/**
 * Reads the file with `read`, which answers null for an application that is no longer saved. A file that cannot be
 * read shows no file, and says why.
 */
export async function readAppFile(read: () => Promise<AppFile | null>): Promise<AppFileRead> {
  try {
    return { file: await read(), error: null };
  } catch (error: unknown) {
    return { file: null, error: errorText(error) };
  }
}

/** A saved application's file, for its references and the values that read with them. */
export function useAppFile(app: string): AppFileRead & {
  /** Reads the file again, such as after a save added to it, and answers it, or null when it cannot be read. */
  readonly reload: () => Promise<AppFile | null>;
} {
  const [shown, setShown] = useState<AppFileRead>({ file: null, error: null });
  const { begin } = useNewest();
  const reload = useCallback(async (): Promise<AppFile | null> => {
    // A read that a newer one overtook still answers its caller, and leaves what shows to the newer one.
    const current = begin();
    const read = await readAppFile(async () => appFile(app));
    if (current()) {
      setShown(read);
    }
    return read.file;
  }, [app, begin]);
  useEffect(() => {
    background(reload());
  }, [reload]);
  return { ...shown, reload };
}
