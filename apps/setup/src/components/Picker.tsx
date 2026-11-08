import { useCallback, useEffect, useMemo, useState } from 'react';

import { type AppSummary, type WindowChoice, type WindowList, errorText, listWindows, pickWindow } from '../lib/api.ts';
import { pickedStatus } from '../lib/matching.ts';
import { background } from '../lib/repeat.ts';
import { WindowsPanel, useWindowFits } from './WindowsPanel.tsx';

/** The window to capture and the application its values belong to. */
export interface Choice {
  readonly window: WindowChoice;
  /** The saved application's ID. */
  readonly app: string;
  /** The application's name. */
  readonly name: string;
}

interface Props {
  readonly apps: readonly AppSummary[];
  /** The page's heading, such as New value. */
  readonly heading: string;
  /** A saved application's ID to keep the choice to, such as a reference's own, or null to let the user choose. */
  readonly lockedApp: string | null;
  readonly initial: Choice | null;
  readonly onCancel: () => void;
  readonly onPick: (choice: Choice) => void;
  /** Leaves to add an application, for when the one wanted is not saved yet. */
  readonly onAddApplication: () => void;
}

export function Picker({ apps, heading, lockedApp, initial, onCancel, onPick, onAddApplication }: Props) {
  const saved = useMemo(() => apps.filter((app) => app.file !== null), [apps]);
  const [list, setList] = useState<WindowList | null>(null);
  const [listError, setListError] = useState<string | null>(null);
  const [listed, setListed] = useState(0);
  const [selected, setSelected] = useState<WindowChoice | null>(initial?.window ?? null);
  const [target, setTarget] = useState<string | null>(initial?.app ?? lockedApp ?? saved[0]?.id ?? null);
  const [pickError, setPickError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const app = saved.find((each) => each.id === target)?.file ?? null;
  const { fits, error: fitError } = useWindowFits(app?.window ?? null, listed);

  const load = useCallback(async (): Promise<void> => {
    try {
      setList(await listWindows());
      setListError(null);
      setListed((count) => count + 1);
    } catch (error: unknown) {
      setListError(errorText(error));
    }
  }, []);
  useEffect(() => {
    background(load());
  }, [load]);

  const choose = (window: WindowChoice): void => {
    setSelected(window);
    setPickError(null);
    // A window a saved application already finds picks that application.
    const owner = saved.find((each) => each.windows.includes(window.handle));
    if (owner !== undefined && lockedApp === null) {
      setTarget(owner.id);
    }
  };

  const ready = selected !== null && app !== null && target !== null && !busy;
  const status =
    selected === null || app === null
      ? null
      : pickedStatus(app.name, selected.handle, fits, fitError, (handle) => {
          return list?.windows.find((each) => each.handle === handle)?.title ?? null;
        });

  const capture = async (): Promise<void> => {
    if (selected === null || app === null || target === null) {
      return;
    }
    setBusy(true);
    try {
      await pickWindow(selected.handle);
      onPick({ window: selected, app: target, name: app.name });
    } catch (error: unknown) {
      setPickError(errorText(error));
    } finally {
      setBusy(false);
    }
  };

  return (
    <main className="page">
      <div className="page-head">
        <h1>{heading}</h1>
        <span className="lead">Pick the application, then the window that shows what to read.</span>
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
        <WindowsPanel
          windows={list?.windows ?? null}
          listError={listError}
          selected={selected?.handle ?? null}
          fits={fits}
          onChoose={choose}
        />
        <aside className="side" aria-label="Application">
          <h2>Application</h2>
          {saved.length === 0 ? (
            <p className="note">
              Values and references belong to an application. Add the application first, then come back here.
            </p>
          ) : (
            <div className="field">
              <label htmlFor="target">Save to</label>
              <select
                id="target"
                className="input"
                disabled={lockedApp !== null}
                value={target ?? ''}
                onChange={(event) => {
                  setTarget(event.target.value);
                }}
              >
                {saved.map((each) => (
                  <option key={each.id} value={each.id}>
                    {each.file?.name ?? each.id}
                  </option>
                ))}
              </select>
            </div>
          )}
          {lockedApp === null ? (
            <button type="button" className="btn" onClick={onAddApplication}>
              Add application
            </button>
          ) : null}
          {app === null ? null : selected === null ? (
            <p className="note">Pick the window that shows what to read.</p>
          ) : (
            <p className={status === null || status.ok ? 'note' : 'warn'} role="status">
              {status?.text}
            </p>
          )}
          {pickError === null ? null : <p className="warn">{pickError}</p>}
          <div className="actions">
            <button type="button" className="btn" onClick={onCancel}>
              Cancel
            </button>
            <button
              type="button"
              className="btn primary"
              disabled={!ready || status?.ok !== true}
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
