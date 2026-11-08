import { useCallback, useEffect, useState } from 'react';

import type { AppSummary, OverlayView } from './bindings.ts';
import { errorText, liveValues, stopLiveValues } from './api.ts';
import { background, useRepeat } from './repeat.ts';

/** How often the Values page asks what each running application's values read. */
const LIVE_MS = 1000;

/** What a running application's values read now, or why they cannot be read. */
export type LiveApp =
  | { readonly kind: 'view'; readonly view: OverlayView }
  | { readonly kind: 'error'; readonly message: string };

/**
 * What the values of each running application of `apps` read now, by application ID, while the page shows. Leaving
 * the page stops reading their windows.
 */
export function useLiveValues(apps: readonly AppSummary[] | null): ReadonlyMap<string, LiveApp> {
  const [live, setLive] = useState<ReadonlyMap<string, LiveApp>>(new Map());
  const running = (apps ?? [])
    .filter((app) => app.windows.length > 0 && (app.file?.values.length ?? 0) > 0)
    .map((app) => app.id)
    .join('\n');
  const poll = useCallback(async (): Promise<void> => {
    const ids = running === '' ? [] : running.split('\n');
    const read = await Promise.all(
      ids.map(async (id): Promise<readonly [string, LiveApp]> => {
        try {
          return [id, { kind: 'view', view: await liveValues(id) }];
        } catch (error: unknown) {
          return [id, { kind: 'error', message: errorText(error) }];
        }
      }),
    );
    setLive(new Map(read));
  }, [running]);
  useRepeat(poll, LIVE_MS, running !== '');
  useEffect(
    () => () => {
      background(stopLiveValues());
    },
    [],
  );
  return live;
}
