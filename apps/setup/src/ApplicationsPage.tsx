import { useState } from 'react';

import { type AntiCheat, type AppSummary, errorText, setAntiCheat } from './api.ts';
import { ANTI_CHEAT_NAMES, PRIORITY_NAMES } from './describe.ts';
import { background } from './repeat.ts';

/** The anti-cheat menu's choices, with not set first since every application starts there. */
const ANTI_CHEAT_CHOICES: readonly AntiCheat[] = ['unknown', 'none', 'userMode', 'kernelLevel'];

interface AntiCheatCellProps {
  readonly name: string;
  readonly antiCheat: AntiCheat;
  readonly confirming: boolean;
  readonly onConfirm: (confirming: boolean) => void;
  readonly onSet: (antiCheat: AntiCheat) => void;
}

/**
 * An application's anti-cheat, picked from a menu. None lets automations
 * press keys into it, so picking None asks first.
 */
function AntiCheatCell({ name, antiCheat, confirming, onConfirm, onSet }: AntiCheatCellProps) {
  if (confirming) {
    return (
      <span className="confirm-none">
        <span className="warn">
          Only pick None if {name} has no anti-cheat. Pressing keys into an application with anti-cheat can get your
          account banned.
        </span>
        <span className="confirm">
          <button
            type="button"
            className="btn small"
            onClick={() => {
              onConfirm(false);
            }}
          >
            Keep {ANTI_CHEAT_NAMES[antiCheat]}
          </button>
          <button
            type="button"
            className="btn small danger"
            onClick={() => {
              onSet('none');
            }}
          >
            Set to None
          </button>
        </span>
      </span>
    );
  }
  return (
    <select
      className="input"
      aria-label={`${name}: anti-cheat`}
      value={antiCheat}
      onChange={(event) => {
        const picked = ANTI_CHEAT_CHOICES.find((choice) => choice === event.target.value);
        if (picked === 'none') {
          onConfirm(true);
        } else if (picked !== undefined) {
          onSet(picked);
        }
      }}
    >
      {ANTI_CHEAT_CHOICES.map((choice) => (
        <option key={choice} value={choice}>
          {ANTI_CHEAT_NAMES[choice]}
        </option>
      ))}
    </select>
  );
}

interface Props {
  readonly apps: readonly AppSummary[] | null;
  readonly loadError: string | null;
  readonly onChanged: () => Promise<void>;
}

export function ApplicationsPage({ apps, loadError, onChanged }: Props) {
  const [confirming, setConfirming] = useState<string | null>(null);
  const [setError, setSetError] = useState<string | null>(null);

  const set = async (app: string, antiCheat: AntiCheat): Promise<void> => {
    try {
      await setAntiCheat(app, antiCheat);
      setSetError(null);
      setConfirming(null);
      await onChanged();
    } catch (error: unknown) {
      setSetError(errorText(error));
    }
  };

  return (
    <main className="page">
      <div className="page-head">
        <h1>Applications</h1>
        <span className="lead">
          How AppyTappy recognizes each application&apos;s window, and whether automations may press keys into it.
        </span>
      </div>
      {loadError === null ? null : <p className="warn">{loadError}</p>}
      {setError === null ? null : (
        <p className="warn" role="alert">
          {setError}
        </p>
      )}
      {apps?.length === 0 ? (
        <div className="empty">
          <h2>No applications yet</h2>
          <p>An application is saved with its first value. Add a value on the Values page to start.</p>
        </div>
      ) : null}
      {apps !== null && apps.length > 0 ? (
        <table className="table">
          <thead>
            <tr>
              <th scope="col">Application</th>
              <th scope="col">Window title</th>
              <th scope="col">Class</th>
              <th scope="col">Executable</th>
              <th scope="col">Fallback</th>
              <th scope="col">Anti-cheat</th>
              <th scope="col">Running</th>
            </tr>
          </thead>
          <tbody>
            {apps.map((app) =>
              app.file === null ? (
                <tr key={app.id}>
                  <td>{app.id}</td>
                  <td colSpan={6} className="warn">
                    {app.error ?? 'This application could not be read.'}
                  </td>
                </tr>
              ) : (
                <tr key={app.id}>
                  <td>{app.file.name}</td>
                  <td>{app.file.window.title}</td>
                  <td>{app.file.window.class}</td>
                  <td>{app.file.window.executable}</td>
                  <td className="note">{PRIORITY_NAMES[app.file.window.priority]}</td>
                  <td>
                    <AntiCheatCell
                      name={app.file.name}
                      antiCheat={app.file.antiCheat}
                      confirming={confirming === app.id}
                      onConfirm={(now) => {
                        setConfirming(now ? app.id : null);
                      }}
                      onSet={(antiCheat) => {
                        background(set(app.id, antiCheat));
                      }}
                    />
                  </td>
                  <td>
                    {app.windows.length > 0 ? (
                      <span className="ok">
                        {app.windows.length === 1 ? '1 window' : `${app.windows.length} windows`}
                      </span>
                    ) : (
                      <span className="note">No</span>
                    )}
                  </td>
                </tr>
              ),
            )}
          </tbody>
        </table>
      ) : null}
    </main>
  );
}
