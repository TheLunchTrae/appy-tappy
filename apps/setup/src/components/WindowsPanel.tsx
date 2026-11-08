import { useEffect, useMemo, useState } from 'react';

import type { WindowChoice, WindowDef, WindowFit } from '../lib/bindings.ts';
import { errorText, matchWindows } from '../lib/api.ts';
import { fitLabel } from '../lib/matching.ts';

/** How long after a change the match waits before it looks for windows again. */
const MATCH_DELAY_MS = 200;

/** The open windows a match takes, the best first, or why they could not be found. */
export interface WindowFits {
  /** Null while looking, without a match, or after an error. */
  readonly fits: readonly WindowFit[] | null;
  readonly error: string | null;
}

/**
 * The open windows `window` takes, looked for again a moment after it changes and whenever `again` changes.
 */
export function useWindowFits(window: WindowDef | null, again: unknown): WindowFits {
  const [found, setFound] = useState<WindowFits>({ fits: null, error: null });
  useEffect(() => {
    setFound({ fits: null, error: null });
    if (window === null) {
      return undefined;
    }
    let current = true;
    const timer = setTimeout(() => {
      matchWindows(window)
        .then((fits) => {
          if (current) {
            setFound({ fits, error: null });
          }
        })
        .catch((error: unknown) => {
          if (current) {
            setFound({ fits: null, error: errorText(error) });
          }
        });
    }, MATCH_DELAY_MS);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [window, again]);
  return found;
}

interface Props {
  /** The open windows, or null while they are listed. */
  readonly windows: readonly WindowChoice[] | null;
  readonly listError: string | null;
  readonly selected: number | null;
  /** The windows an application's match takes, the best first, or null while they are counted. */
  readonly fits: readonly WindowFit[] | null;
  readonly onChoose: (window: WindowChoice) => void;
  /** Says why the list is empty when no window is open. */
  readonly emptyText?: string;
}

/** The open windows to pick from, filtered by title or executable, marking those an application's match takes. */
export function WindowsPanel({ windows, listError, selected, fits, onChoose, emptyText }: Props) {
  const [filter, setFilter] = useState('');
  const shown = useMemo(() => {
    const needle = filter.trim().toLowerCase();
    const all = windows ?? [];
    return needle.length === 0
      ? all
      : all.filter(
          (window) => window.title.toLowerCase().includes(needle) || window.executable.toLowerCase().includes(needle),
        );
  }, [filter, windows]);
  return (
    <section className="windows" aria-label="Windows">
      <input
        className="input"
        type="search"
        placeholder="Filter by title or executable"
        aria-label="Filter windows"
        value={filter}
        onChange={(event) => {
          setFilter(event.target.value);
        }}
      />
      {listError === null ? null : <p className="warn">{listError}</p>}
      <ul className="window-list">
        {shown.map((window) => {
          const label = fitLabel(fits, window.handle);
          return (
            <li key={window.handle}>
              <button
                type="button"
                aria-pressed={selected === window.handle}
                onClick={() => {
                  onChoose(window);
                }}
              >
                <span className="title">{window.title}</span>
                <small>
                  {window.width} × {window.height}
                </small>
                <small>{window.executable}</small>
                {label === null ? <small /> : <small className={label.className}>{label.text}</small>}
              </button>
            </li>
          );
        })}
        {windows !== null && shown.length === 0 ? (
          <li className="note list-note">
            {windows.length === 0 ? (emptyText ?? 'No windows are open.') : 'No window fits the filter.'}
          </li>
        ) : null}
      </ul>
    </section>
  );
}
