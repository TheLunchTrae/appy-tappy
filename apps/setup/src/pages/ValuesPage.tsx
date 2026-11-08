import { useState } from 'react';

import { type AppSummary, type AutomationSummary, deleteValue, errorText, markChecked } from '../lib/api.ts';
import { usedBy } from '../lib/automation.ts';
import { buildState, checkedText } from '../lib/build.ts';
import { checkSummary } from '../lib/describe.ts';
import { hex } from '../lib/pixels.ts';
import { background } from '../lib/repeat.ts';

interface AppRowsProps {
  readonly app: AppSummary;
  readonly automations: readonly AutomationSummary[] | null;
  readonly confirming: string | null;
  readonly onConfirm: (key: string | null) => void;
  readonly onDelete: (value: string) => void;
  readonly onMarkChecked: () => void;
}

/** The automations that use a value, in a few words. */
function usedByText(names: readonly string[] | null): string {
  if (names === null) {
    return '';
  }
  return names.length === 0 ? 'No automations yet' : names.join(', ');
}

function AppRows({ app, automations, confirming, onConfirm, onDelete, onMarkChecked }: AppRowsProps) {
  const running = app.windows.length > 0;
  if (app.file === null) {
    return (
      <tr className="group">
        <td colSpan={5}>
          {app.id}
          <small className="warn">{app.error ?? 'This application could not be read.'}</small>
        </td>
      </tr>
    );
  }
  const { file } = app;
  if (file.values.length === 0) {
    return null;
  }
  const build = buildState(file, app.running);
  const checked = checkedText(file, (at) => at.toLocaleDateString(undefined, { dateStyle: 'medium' }));
  return (
    <>
      <tr className="group">
        <td colSpan={5}>
          <span className={running ? 'dot running' : 'dot'} />
          {app.file.name}
          <small>
            {app.file.window.executable} · {running ? 'running' : 'not running'}
            {checked === null ? null : ` · ${checked}`}
          </small>
          {build === null ? null : (
            <div className="changed">
              <span className={build.kind === 'changed' ? 'warn' : 'note'}>
                {build.kind === 'changed'
                  ? `Changed since its values were checked (${build.change}). Check that each value still reads right.`
                  : 'Which build its values were checked against is not recorded yet.'}
              </span>
              <button type="button" className="btn small" onClick={onMarkChecked}>
                Mark as checked
              </button>
            </div>
          )}
        </td>
      </tr>
      {app.file.values.map((value) => {
        const summary = checkSummary(value.screen.check, file.references);
        const key = `${app.id}/${value.id}`;
        const { region, windowSize } = value.screen;
        return (
          <tr key={value.id}>
            <td>{value.name}</td>
            <td>
              {summary.colors.map((color) => (
                <span key={hex(color)} className="swatch" style={{ background: hex(color) }} />
              ))}
              {summary.name}
            </td>
            <td className="note">
              {region.width} × {region.height} at {region.x}, {region.y} of {windowSize.width} × {windowSize.height}
            </td>
            <td className="note">{usedByText(automations === null ? null : usedBy(automations, app.id, value.id))}</td>
            <td className="end">
              {confirming === key ? (
                <span className="confirm">
                  <button
                    type="button"
                    className="btn small"
                    onClick={() => {
                      onConfirm(null);
                    }}
                  >
                    Keep
                  </button>
                  <button
                    type="button"
                    className="btn small danger"
                    onClick={() => {
                      onDelete(value.id);
                    }}
                  >
                    Delete {value.name}
                  </button>
                </span>
              ) : (
                <button
                  type="button"
                  className="btn small"
                  onClick={() => {
                    onConfirm(key);
                  }}
                >
                  Delete
                </button>
              )}
            </td>
          </tr>
        );
      })}
    </>
  );
}

interface Props {
  readonly apps: readonly AppSummary[] | null;
  readonly automations: readonly AutomationSummary[] | null;
  readonly loadError: string | null;
  readonly onChanged: () => Promise<void>;
  readonly onNew: () => void;
}

export function ValuesPage({ apps, automations, loadError, onChanged, onNew }: Props) {
  const [confirming, setConfirming] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const remove = async (app: string, value: string): Promise<void> => {
    try {
      await deleteValue(app, value);
      setActionError(null);
      setConfirming(null);
      await onChanged();
    } catch (error: unknown) {
      setActionError(errorText(error));
    }
  };

  const markAsChecked = async (app: string): Promise<void> => {
    try {
      await markChecked(app);
      setActionError(null);
      await onChanged();
    } catch (error: unknown) {
      setActionError(errorText(error));
    }
  };

  const total = apps?.reduce((sum, app) => sum + (app.file?.values.length ?? 0), 0) ?? 0;

  return (
    <main className="page">
      <div className="page-head">
        <h1>Values</h1>
        <span className="lead">What AppyTappy reads from each application&apos;s window.</span>
        <div className="end">
          <button type="button" className="btn primary" onClick={onNew}>
            New value
          </button>
        </div>
      </div>
      {loadError === null ? null : (
        <p className="warn" role="alert">
          {loadError}
        </p>
      )}
      {actionError === null ? null : (
        <p className="warn" role="alert">
          {actionError}
        </p>
      )}
      {apps !== null && total === 0 && apps.every((app) => app.error === null) ? (
        <div className="empty">
          <h2>No values yet</h2>
          <p>
            A value is something AppyTappy reads from an application&apos;s window, such as a number in a certain place
            on screen, whether an icon is showing or how full a bar is. Pick a window, take a screenshot and draw around
            what to read.
          </p>
          <button type="button" className="btn primary" onClick={onNew}>
            New value
          </button>
        </div>
      ) : null}
      {apps !== null && (total > 0 || apps.some((app) => app.error !== null)) ? (
        <table className="table">
          <thead>
            <tr>
              <th scope="col">Value</th>
              <th scope="col">Check</th>
              <th scope="col">Region</th>
              <th scope="col">Used by</th>
              <th scope="col">
                <span className="note">Actions</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {apps.map((app) => (
              <AppRows
                key={app.id}
                app={app}
                automations={automations}
                confirming={confirming}
                onConfirm={setConfirming}
                onDelete={(value) => {
                  background(remove(app.id, value));
                }}
                onMarkChecked={() => {
                  background(markAsChecked(app.id));
                }}
              />
            ))}
          </tbody>
        </table>
      ) : null}
    </main>
  );
}
