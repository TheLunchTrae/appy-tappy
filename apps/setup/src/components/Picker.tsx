import { useState } from 'react';

import type { AppSummary, WindowChoice } from '../lib/bindings.ts';
import { errorText, pickWindow } from '../lib/api.ts';
import { captureWindow, isRunning, known, offersMatchEdit, pickerStep, stepStatus } from '../lib/picker.ts';
import { background } from '../lib/repeat.ts';
import { useAppWindows } from '../lib/useAppWindows.ts';
import { Trail } from './Trail.tsx';

/** The window to capture and the application its values belong to. */
export interface Choice {
  readonly window: WindowChoice;
  /** The saved application's ID. */
  readonly app: string;
  /** The application's name. */
  readonly name: string;
}

/** The capture of the chosen window: not asked for, starting, or refused with the reason. */
type Start = 'idle' | 'starting' | { readonly error: string };

interface Props {
  /** The page's heading, such as New value. */
  readonly heading: string;
  /** The way to the picker's flow, for its breadcrumb. */
  readonly trail: readonly string[];
  /** The saved application the flow belongs to already, such as a reference's own, or null to let the user choose. */
  readonly lockedApp: string | null;
  /** The application chosen at first: the last choice's, or one handed over to this flow. */
  readonly startApp: string | null;
  /** The window of the last choice, picked again when several fit. */
  readonly startWindow: number | null;
  readonly onCancel: () => void;
  readonly onPick: (choice: Choice) => void;
  /** Leaves to add an application, and comes back with it chosen, or with `chosen` when that is canceled. */
  readonly onAddApplication: (chosen: string | null) => void;
  /** Leaves to change how `app` is found, and comes back to it. */
  readonly onEditApplication: (app: AppSummary) => void;
}

/** Chooses the application a value or reference belongs to, then the window that application reads. */
export function Picker({
  heading,
  trail,
  lockedApp,
  startApp,
  startWindow,
  onCancel,
  onPick,
  onAddApplication,
  onEditApplication,
}: Props) {
  const [chosen, setChosen] = useState(lockedApp ?? startApp);
  const [picked, setPicked] = useState(startWindow);
  const [start, setStart] = useState<Start>('idle');
  const { look, refresh } = useAppWindows();

  const found = known(look);
  const step = pickerStep({ locked: lockedApp !== null, chosen, look, picked });
  const status = stepStatus(step);
  const target = captureWindow(step);
  const busy = start === 'starting';

  const choose = (app: string): void => {
    setChosen(app);
    setPicked(null);
    setStart('idle');
  };

  const capture = async (): Promise<void> => {
    if (target === null || !('app' in step)) {
      return;
    }
    setStart('starting');
    try {
      await pickWindow(target.handle);
      setStart('idle');
      onPick({ window: target, app: step.app.id, name: step.app.name });
    } catch (error: unknown) {
      setStart({ error: errorText(error) });
    }
  };

  return (
    <main className="page">
      <Trail trail={trail} here={heading} />
      <div className="page-head">
        <h1>{heading}</h1>
        <span className="lead">
          {lockedApp === null
            ? 'Choose the application, then capture its window.'
            : "Capture the application's window."}
        </span>
        <div className="end">
          <button
            type="button"
            className="btn"
            disabled={busy}
            onClick={() => {
              setStart('idle');
              refresh();
            }}
          >
            Refresh
          </button>
        </div>
      </div>
      {found !== null && found.antiCheat.length > 0 ? (
        <p className="warn">
          Anti-cheat running now: {found.antiCheat.join(', ')}. Reading the screen is fine, but set the
          application&apos;s anti-cheat before giving it key press actions.
        </p>
      ) : null}
      <div className={lockedApp === null ? 'picker' : 'picker alone'}>
        {lockedApp === null ? (
          <section className="windows" aria-label="Applications">
            <ul className="window-list">
              {(found?.apps ?? []).map((app) => {
                const running = isRunning(look, app);
                return (
                  <li key={app.id}>
                    <button
                      type="button"
                      aria-pressed={'app' in step && step.app.id === app.id}
                      disabled={busy}
                      onClick={() => {
                        choose(app.id);
                      }}
                    >
                      <span className="title">{app.name}</span>
                      {running === null ? (
                        <small />
                      ) : (
                        <small className={running ? 'ok' : undefined}>{running ? 'Running' : 'Not running'}</small>
                      )}
                      <small>{app.executable}</small>
                    </button>
                  </li>
                );
              })}
              <li>
                <button
                  type="button"
                  className="add"
                  disabled={busy}
                  onClick={() => {
                    onAddApplication('app' in step ? step.app.id : null);
                  }}
                >
                  <span className="title">Add application</span>
                  <small />
                  <small>For a program that is not in this list yet. You come back here with it chosen.</small>
                </button>
              </li>
            </ul>
          </section>
        ) : null}
        <aside className="side" aria-label="Window">
          <h2>{'app' in step ? step.app.name : 'Application'}</h2>
          <p className={status.warn ? 'warn' : 'note'} role="status">
            {status.text}
          </p>
          {step.kind === 'one' ? (
            <p className="note">
              {step.window.width} × {step.window.height} · {step.window.class} · {step.window.executable}
            </p>
          ) : null}
          {step.kind === 'several' ? (
            <ul className="window-list" aria-label="Windows that fit">
              {step.windows.map((each) => (
                <li key={each.handle}>
                  <button
                    type="button"
                    aria-pressed={step.picked?.handle === each.handle}
                    disabled={busy}
                    onClick={() => {
                      setPicked(each.handle);
                      setStart('idle');
                    }}
                  >
                    <span className="title">{each.title}</span>
                    <small>
                      {each.width} × {each.height}
                    </small>
                    <small>
                      {each.class} · {each.executable}
                    </small>
                    {step.reads === each.handle ? <small className="ok">Read if it starts now</small> : <small />}
                  </button>
                </li>
              ))}
            </ul>
          ) : null}
          {'app' in step && offersMatchEdit(step) ? (
            <button
              type="button"
              className="btn"
              disabled={busy}
              onClick={() => {
                onEditApplication(step.app.summary);
              }}
            >
              Change how it&apos;s found
            </button>
          ) : null}
          {typeof start === 'object' ? (
            <p className="warn" role="alert">
              {start.error}
            </p>
          ) : null}
          <div className="actions">
            <button type="button" className="btn" onClick={onCancel}>
              Cancel
            </button>
            <button
              type="button"
              className="btn primary"
              disabled={target === null || busy}
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
