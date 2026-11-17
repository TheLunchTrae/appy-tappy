import { useEffect, useMemo, useState } from 'react';

import type { AppSummary, OverlayView } from './bindings.ts';
import { errorText, liveValues } from './api.ts';
import { background } from './repeat.ts';
import { ask } from './request.ts';

/** How often the Values page asks what each running application's values read. */
const LIVE_MS = 1000;

/** What a running application's values read now, or why they cannot be read. */
export type LiveApp =
  | { readonly kind: 'view'; readonly view: OverlayView }
  | { readonly kind: 'error'; readonly message: string };

const NO_LIVE: ReadonlyMap<string, LiveApp> = new Map();

// A final empty poll must reach the owner after the page's reads, and before
// any reads from a reopened page. Native commands can otherwise overtake it.
let reading: Promise<unknown> = Promise.resolve();

function pollLiveValues(applications: readonly string[], current = (): boolean => true): ReturnType<typeof liveValues> {
  const requested = reading.then(async () => (current() ? liveValues(applications) : []));
  reading = requested.catch(() => undefined);
  return requested;
}

/**
 * What the running applications' values read, by application ID. Each poll
 * sends the whole list to the owner, which releases captures it leaves out.
 */
export function useLiveValues(apps: readonly AppSummary[] | null): ReadonlyMap<string, LiveApp> {
  const running = (apps ?? [])
    .filter((app) => app.windows.length > 0 && (app.file?.values.length ?? 0) > 0)
    .map((app) => app.id)
    .sort()
    .join('\n');
  const wanted = useMemo(() => (running === '' ? [] : running.split('\n')), [running]);
  const [live, setLive] = useState<{
    readonly apps: readonly string[];
    readonly views: ReadonlyMap<string, LiveApp>;
  } | null>(null);

  useEffect(() => {
    let active = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let cancel: () => void;
    const poll = (): void => {
      const show = (views: ReadonlyMap<string, LiveApp>): void => {
        setLive({ apps: wanted, views });
        if (wanted.length > 0) {
          timer = setTimeout(poll, LIVE_MS);
        }
      };
      cancel = ask(
        async () => pollLiveValues(wanted, () => active),
        (results) => {
          show(
            new Map(
              results.map(([app, result]): [string, LiveApp] => [
                app,
                'Ok' in result ? { kind: 'view', view: result.Ok } : { kind: 'error', message: result.Err },
              ]),
            ),
          );
        },
        (error) => {
          const failed: LiveApp = { kind: 'error', message: errorText(error) };
          show(new Map(wanted.map((app) => [app, failed])));
        },
      );
    };
    poll();
    return () => {
      active = false;
      cancel();
      clearTimeout(timer);
    };
  }, [wanted]);
  useEffect(
    () => () => {
      background(pollLiveValues([]));
    },
    [],
  );
  return live?.apps === wanted ? live.views : NO_LIVE;
}
