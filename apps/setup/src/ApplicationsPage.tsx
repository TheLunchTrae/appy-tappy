import type { AppSummary } from './api.ts';
import { ANTI_CHEAT_NAMES, PRIORITY_NAMES } from './describe.ts';

interface Props {
  readonly apps: readonly AppSummary[] | null;
  readonly loadError: string | null;
}

export function ApplicationsPage({ apps, loadError }: Props) {
  return (
    <main className="page">
      <div className="page-head">
        <h1>Applications</h1>
        <span className="lead">How AppyTappy recognizes each application&apos;s window.</span>
      </div>
      {loadError === null ? null : <p className="warn">{loadError}</p>}
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
                  <td className="note">{ANTI_CHEAT_NAMES[app.file.antiCheat]}</td>
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
