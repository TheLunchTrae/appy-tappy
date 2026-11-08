import { useEffect, useRef, useState } from 'react';

import type { OverlayModeDef, OverlayView } from './lib/bindings.ts';
import { errorText, overlayCard, overlayState } from './lib/api.ts';
import { readingText } from './lib/automation.ts';
import { background, useRepeat } from './lib/repeat.ts';

/** How long the overlay waits between two polls; its readings change at most about four times a second. */
const POLL_MS = 150;

interface Props {
  readonly app: string;
  readonly mode: OverlayModeDef;
}

/** An application's live readings, in a window of its own over the application or in a panel kept on top. */
export function Overlay({ app, mode }: Props) {
  const [view, setView] = useState<OverlayView | null>(null);
  const [error, setError] = useState<string | null>(null);

  // The window over the application is transparent, so the page behind the card must be too.
  useEffect(() => {
    document.documentElement.classList.add(mode === 'overlay' ? 'overlay-page' : 'panel-page');
  }, [mode]);

  // The window over the application is only as big as the card, so the setup app is told each size it takes.
  const card = useRef<HTMLElement>(null);
  useEffect(() => {
    const element = card.current;
    if (mode !== 'overlay' || element === null) {
      return undefined;
    }
    const observer = new ResizeObserver(() => {
      const { width, height } = element.getBoundingClientRect();
      background(overlayCard(app, width, height));
    });
    observer.observe(element);
    return () => {
      observer.disconnect();
    };
  }, [app, mode]);

  useRepeat(
    async () => {
      try {
        setView(await overlayState(app));
        setError(null);
      } catch (caught: unknown) {
        setError(errorText(caught));
      }
    },
    POLL_MS,
    true,
  );

  // A message says why there are no readings, so the values are left out under it.
  const message = error ?? view?.message ?? null;
  return (
    <section
      ref={card}
      className={mode === 'overlay' ? 'overlay-card' : 'panel-card'}
      aria-label={view?.name ?? 'Live values'}
    >
      {mode === 'overlay' && view !== null ? <h1 className="overlay-name">{view.name}</h1> : null}
      {message === null ? null : <p className="overlay-message">{message}</p>}
      {message !== null || view === null || view.values.length === 0 ? null : (
        <dl className="overlay-values">
          {view.values.map((value) => (
            // An application's values have names unique to it, since their IDs come from them.
            <div key={value.name} className="overlay-row">
              <dt>{value.name}</dt>
              <dd className={value.problem === null ? undefined : 'overlay-problem'}>
                {value.reading === null ? (value.problem ?? '…') : readingText(value.reading)}
              </dd>
            </div>
          ))}
        </dl>
      )}
    </section>
  );
}
