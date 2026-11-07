import { useCallback, useEffect, useMemo, useState } from 'react';

import {
  type AppSummary,
  type MatchPriority,
  type WindowChoice,
  type WindowDef,
  type WindowList,
  errorText,
  listWindows,
  matchWindows,
  pickWindow,
} from './api.ts';
import { PRIORITY_NAMES } from './describe.ts';
import { background } from './repeat.ts';

/** The window to capture and the application its values belong to. */
export interface Choice {
  readonly window: WindowChoice;
  /** A saved application's ID, or null to save a new one. */
  readonly app: string | null;
  /** The application's name. */
  readonly name: string;
  /** How the application's window is recognized. */
  readonly matcher: WindowDef;
}

interface Props {
  readonly apps: readonly AppSummary[];
  readonly initial: Choice | null;
  readonly onCancel: () => void;
  readonly onPick: (choice: Choice) => void;
}

const NEW = 'new';

/** How long the matcher waits after typing before it counts matching windows. */
const MATCH_DELAY_MS = 200;

function exact(window: WindowChoice): WindowDef {
  return { title: window.title, class: window.class, executable: window.executable, priority: 'titleElseExecutable' };
}

function matchCount(matches: readonly number[] | null): string {
  if (matches === null) {
    return 'Counting matching windows…';
  }
  if (matches.length === 0) {
    return 'No running window matches.';
  }
  return matches.length === 1 ? '1 running window matches.' : `${matches.length} running windows match.`;
}

interface MatcherProps {
  readonly name: string;
  readonly matcher: WindowDef;
  readonly onName: (name: string) => void;
  readonly onPattern: (part: 'title' | 'class' | 'executable', text: string) => void;
  readonly onPriority: (priority: MatchPriority) => void;
}

const PRIORITIES: readonly MatchPriority[] = ['title', 'titleElseClass', 'titleElseExecutable'];

const PATTERN_NAMES = { title: 'Window title', class: 'Window class', executable: 'Executable' } as const;

function MatcherFields({ name, matcher, onName, onPattern, onPriority }: MatcherProps) {
  return (
    <>
      <div className="field">
        <label htmlFor="app-name">Name</label>
        <input
          id="app-name"
          className="input"
          value={name}
          onChange={(event) => {
            onName(event.target.value);
          }}
        />
      </div>
      <p className="note">
        AppyTappy finds this application by its window. Use * for any run of characters and ? for one, such as Example
        App* for a title that changes. Case is ignored.
      </p>
      {(['title', 'class', 'executable'] as const).map((part) => (
        <div className="field" key={part}>
          <label htmlFor={`match-${part}`}>{PATTERN_NAMES[part]}</label>
          <input
            id={`match-${part}`}
            className="input"
            value={matcher[part]}
            onChange={(event) => {
              onPattern(part, event.target.value);
            }}
          />
        </div>
      ))}
      <div className="field">
        <label htmlFor="match-priority">When no window matches all three</label>
        <select
          id="match-priority"
          className="input"
          value={matcher.priority}
          onChange={(event) => {
            const priority = PRIORITIES.find((option) => option === event.target.value);
            if (priority !== undefined) {
              onPriority(priority);
            }
          }}
        >
          {PRIORITIES.map((priority) => (
            <option key={priority} value={priority}>
              {PRIORITY_NAMES[priority]}
            </option>
          ))}
        </select>
      </div>
    </>
  );
}

export function Picker({ apps, initial, onCancel, onPick }: Props) {
  const [list, setList] = useState<WindowList | null>(null);
  const [listError, setListError] = useState<string | null>(null);
  const [filter, setFilter] = useState('');
  const [selected, setSelected] = useState<WindowChoice | null>(initial?.window ?? null);
  const [target, setTarget] = useState<string>(initial?.app ?? NEW);
  const [name, setName] = useState(initial?.name ?? '');
  const [matcher, setMatcher] = useState<WindowDef | null>(initial?.matcher ?? null);
  const [matches, setMatches] = useState<readonly number[] | null>(null);
  const [pickError, setPickError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const saved = useMemo(() => apps.filter((app) => app.file !== null), [apps]);
  const existing = saved.find((app) => app.id === target)?.file ?? null;
  const effective = existing?.window ?? matcher;

  const load = useCallback(async (): Promise<void> => {
    try {
      setList(await listWindows());
      setListError(null);
    } catch (error: unknown) {
      setListError(errorText(error));
    }
  }, []);
  useEffect(() => {
    background(load());
  }, [load]);

  useEffect(() => {
    if (effective === null) {
      setMatches(null);
      return undefined;
    }
    let current = true;
    const timer = setTimeout(() => {
      matchWindows(effective)
        .then((found) => {
          if (current) {
            setMatches(found);
          }
        })
        .catch(() => {
          if (current) {
            setMatches(null);
          }
        });
    }, MATCH_DELAY_MS);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [effective]);

  const choose = (window: WindowChoice): void => {
    setSelected(window);
    setPickError(null);
    const owner = saved.find((app) => app.windows.includes(window.handle));
    if (owner === undefined) {
      setTarget(NEW);
      setMatcher(exact(window));
      setName(window.title);
    } else {
      setTarget(owner.id);
    }
  };

  const setPattern = (part: 'title' | 'class' | 'executable', text: string): void => {
    setMatcher((current) => (current === null ? null : { ...current, [part]: text }));
  };

  const shown = useMemo(() => {
    const needle = filter.trim().toLowerCase();
    const windows = list?.windows ?? [];
    return needle.length === 0
      ? windows
      : windows.filter(
          (window) => window.title.toLowerCase().includes(needle) || window.executable.toLowerCase().includes(needle),
        );
  }, [filter, list]);

  const finalName = existing?.name ?? name.trim();
  const ready = selected !== null && effective !== null && finalName.length > 0 && !busy;
  const missesSelected = selected !== null && matches !== null && !matches.includes(selected.handle);

  const capture = async (): Promise<void> => {
    if (selected === null || effective === null) {
      return;
    }
    setBusy(true);
    try {
      await pickWindow(selected.handle);
      onPick({ window: selected, app: existing === null ? null : target, name: finalName, matcher: effective });
    } catch (error: unknown) {
      setPickError(errorText(error));
    } finally {
      setBusy(false);
    }
  };

  return (
    <main className="page">
      <div className="page-head">
        <h1>New value</h1>
        <span className="lead">Pick the window that shows what to read.</span>
        <div className="end">
          <button
            type="button"
            className="btn"
            onClick={() => {
              background(load());
            }}
          >
            Refresh windows
          </button>
        </div>
      </div>
      {list !== null && list.antiCheat.length > 0 ? (
        <p className="warn">
          Anti-cheat running now: {list.antiCheat.join(', ')}. Reading the screen is fine, but set the
          application&apos;s anti-cheat before giving it key press actions.
        </p>
      ) : null}
      <div className="picker">
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
            {shown.map((window) => (
              <li key={window.handle}>
                <button
                  type="button"
                  aria-pressed={selected?.handle === window.handle}
                  onClick={() => {
                    choose(window);
                  }}
                >
                  <span className="title">{window.title}</span>
                  <small>
                    {window.width} × {window.height}
                  </small>
                  <small>{window.executable}</small>
                  {matches?.includes(window.handle) === true ? <small className="ok">matches</small> : <small />}
                </button>
              </li>
            ))}
            {list !== null && shown.length === 0 ? (
              <li className="note list-note">
                {list.windows.length === 0 ? 'No windows are open.' : 'No window fits the filter.'}
              </li>
            ) : null}
          </ul>
        </section>
        <aside className="side" aria-label="Application">
          <h2>Application</h2>
          {selected === null ? (
            <p className="note">Pick a window to set up its application.</p>
          ) : (
            <>
              <div className="field">
                <label htmlFor="target">Save values to</label>
                <select
                  id="target"
                  className="input"
                  value={target}
                  onChange={(event) => {
                    setTarget(event.target.value);
                    if (event.target.value === NEW && matcher === null) {
                      setMatcher(exact(selected));
                      setName(selected.title);
                    }
                  }}
                >
                  <option value={NEW}>A new application</option>
                  {saved.map((app) => (
                    <option key={app.id} value={app.id}>
                      {app.file?.name ?? app.id}
                    </option>
                  ))}
                </select>
              </div>
              {existing === null && matcher !== null ? (
                <MatcherFields
                  name={name}
                  matcher={matcher}
                  onName={setName}
                  onPattern={setPattern}
                  onPriority={(priority) => {
                    setMatcher({ ...matcher, priority });
                  }}
                />
              ) : (
                <p className="note">
                  {existing?.name} is recognized by {existing?.window.executable}. Its matcher is kept as it is.
                </p>
              )}
              <p className={missesSelected ? 'warn' : 'note'}>
                {matchCount(matches)}
                {missesSelected ? ' The picked window is not one of them, so AppyTappy would not find it later.' : ''}
              </p>
            </>
          )}
          {pickError === null ? null : <p className="warn">{pickError}</p>}
          <div className="actions">
            <button type="button" className="btn" onClick={onCancel}>
              Cancel
            </button>
            <button
              type="button"
              className="btn primary"
              disabled={!ready}
              onClick={() => {
                background(capture());
              }}
            >
              Capture this window
            </button>
          </div>
        </aside>
      </div>
    </main>
  );
}
