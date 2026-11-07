import { useState } from 'react';

import { type AppSummary, deleteValue, errorText } from './api.ts';
import { checkSummary } from './describe.ts';
import { hex } from './pixels.ts';
import { background } from './repeat.ts';

interface AppRowsProps {
  readonly app: AppSummary;
  readonly confirming: string | null;
  readonly onConfirm: (key: string | null) => void;
  readonly onDelete: (value: string) => void;
}

function AppRows({ app, confirming, onConfirm, onDelete }: AppRowsProps) {
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
  if (app.file.values.length === 0) {
    return null;
  }
  return (
    <>
      <tr className="group">
        <td colSpan={5}>
          <span className={running ? 'dot running' : 'dot'} />
          {app.file.name}
          <small>
            {app.file.window.executable} · {running ? 'running' : 'not running'}
          </small>
        </td>
      </tr>
      {app.file.values.map((value) => {
        const summary = checkSummary(value.screen.check);
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
            <td className="note">No automations yet</td>
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
  readonly loadError: string | null;
  readonly onChanged: () => Promise<void>;
  readonly onNew: () => void;
}

export function ValuesPage({ apps, loadError, onChanged, onNew }: Props) {
  const [confirming, setConfirming] = useState<string | null>(null);
  const [deleteError, setDeleteError] = useState<string | null>(null);

  const remove = async (app: string, value: string): Promise<void> => {
    try {
      await deleteValue(app, value);
      setDeleteError(null);
      setConfirming(null);
      await onChanged();
    } catch (error: unknown) {
      setDeleteError(errorText(error));
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
      {loadError === null ? null : <p className="warn">{loadError}</p>}
      {deleteError === null ? null : <p className="warn">{deleteError}</p>}
      {apps !== null && total === 0 && apps.every((app) => app.error === null) ? (
        <div className="empty">
          <h2>No values yet</h2>
          <p>
            A value is something AppyTappy reads from an application&apos;s window, such as a level percent or a health
            bar. Pick a window, take a screenshot and draw around what to read.
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
                confirming={confirming}
                onConfirm={setConfirming}
                onDelete={(value) => {
                  background(remove(app.id, value));
                }}
              />
            ))}
          </tbody>
        </table>
      ) : null}
    </main>
  );
}
