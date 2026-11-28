import { useEffect, useRef } from 'react';

import { pickerLanding, useLanding } from '../lib/landing.ts';
import { chooserStep, isRunning, known, lookFailedText } from '../lib/picker.ts';
import { useAppWindows } from '../lib/useAppWindows.ts';
import { Trail } from './Trail.tsx';

interface Props {
  /** The page's heading, such as New value. */
  readonly heading: string;
  /** The way to the picker's flow, for its breadcrumb. */
  readonly trail: readonly string[];
  /** Whether the picker shows because another flow opened its flow or ended, so the focus has to land somewhere. */
  readonly continued: boolean;
  readonly onCancel: () => void;
  /** Takes the application chosen, or the only saved one, by its ID. Its capture screen gets its window. */
  readonly onChoose: (app: string) => void;
  /** Leaves to add an application, and comes back with it chosen. */
  readonly onAddApplication: () => void;
}

/** What the picker says before it lists the applications, and whether it is a problem. */
function statusOf(step: ReturnType<typeof chooserStep>): { readonly text: string; readonly warn: boolean } {
  switch (step.kind) {
    case 'looking':
    case 'only':
      return { text: 'Reading the saved applications…', warn: false };
    case 'failed':
      return { text: lookFailedText(step.error), warn: true };
    case 'no-applications':
      return {
        text: 'Values and references belong to an application, and none is saved yet. Add the application to carry on.',
        warn: false,
      };
    case 'choosing':
      return { text: 'Choose the application, or add it if it is not in the list.', warn: false };
  }
}

/**
 * Chooses the application a new value or reference belongs to. With one saved application there is nothing to
 * choose, so it is taken at once. The capture screen then finds and captures that application's window itself.
 */
export function Picker({ heading, trail, continued, onCancel, onChoose, onAddApplication }: Props) {
  const { look, refresh } = useAppWindows();
  const step = chooserStep(look);
  const status = statusOf(step);
  const page = useRef<HTMLElement>(null);
  useLanding(continued, pickerLanding(step), page);
  const only = step.kind === 'only' ? step.app.id : null;
  useEffect(() => {
    if (only !== null) {
      onChoose(only);
    }
  }, [only, onChoose]);

  return (
    <main className="page" ref={page}>
      <Trail trail={trail} here={heading} />
      <div className="page-head">
        <h1>{heading}</h1>
        <span className="lead">Choose the application it belongs to.</span>
        <div className="end">
          <button type="button" className="btn" data-landing="refresh" onClick={refresh}>
            Refresh
          </button>
        </div>
      </div>
      <p className={status.warn ? 'warn' : 'note'} role="status">
        {status.text}
      </p>
      <section className="windows" aria-label="Applications">
        <ul className="window-list">
          {(known(look)?.apps ?? []).map((app) => {
            const running = isRunning(look, app);
            return (
              <li key={app.id}>
                <button
                  type="button"
                  onClick={() => {
                    onChoose(app.id);
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
            <button type="button" className="add" data-landing="add-application" onClick={onAddApplication}>
              <span className="title">Add application</span>
              <small />
              <small>For a program that is not in this list yet. You come back with it chosen.</small>
            </button>
          </li>
        </ul>
      </section>
      <div className="actions">
        <button type="button" className="btn" onClick={onCancel}>
          Cancel
        </button>
      </div>
    </main>
  );
}
