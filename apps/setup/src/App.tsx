import { useCallback, useState } from 'react';

import { type AppSummary, errorText, listApps } from './api.ts';
import { ApplicationsPage } from './ApplicationsPage.tsx';
import { AutomationsPage } from './AutomationsPage.tsx';
import { NewValue } from './NewValue.tsx';
import { useRepeat } from './repeat.ts';
import { ValuesPage } from './ValuesPage.tsx';

type Page = 'values' | 'automations' | 'applications';

const PAGES: readonly { readonly id: Page; readonly name: string }[] = [
  { id: 'values', name: 'Values' },
  { id: 'automations', name: 'Automations' },
  { id: 'applications', name: 'Applications' },
];

/** How often the saved applications are read again, which also refreshes which ones are running. */
const REFRESH_MS = 3000;

export function App() {
  const [page, setPage] = useState<Page>('values');
  const [creating, setCreating] = useState(false);
  const [apps, setApps] = useState<readonly AppSummary[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);

  const refresh = useCallback(async (): Promise<void> => {
    try {
      setApps(await listApps());
      setLoadError(null);
    } catch (error: unknown) {
      setLoadError(errorText(error));
    }
  }, []);
  useRepeat(refresh, REFRESH_MS, !creating);

  const counts: Readonly<Record<Page, number | null>> = {
    values: apps === null ? null : apps.reduce((total, app) => total + (app.file?.values.length ?? 0), 0),
    automations: null,
    applications: apps === null ? null : apps.length,
  };

  const content = (() => {
    if (creating) {
      return (
        <NewValue
          apps={apps ?? []}
          onCancel={() => {
            setCreating(false);
          }}
          onSaved={() => {
            setCreating(false);
            setPage('values');
          }}
        />
      );
    }
    switch (page) {
      case 'values':
        return (
          <ValuesPage
            apps={apps}
            loadError={loadError}
            onChanged={refresh}
            onNew={() => {
              setCreating(true);
            }}
          />
        );
      case 'automations':
        return <AutomationsPage />;
      case 'applications':
        return <ApplicationsPage apps={apps} loadError={loadError} />;
    }
  })();

  return (
    <div className="shell">
      <nav className="nav" aria-label="Pages">
        {PAGES.map(({ id, name }) => {
          const count = counts[id];
          return (
            <button
              key={id}
              type="button"
              className="tab"
              aria-current={page === id && !creating ? 'page' : undefined}
              onClick={() => {
                setCreating(false);
                setPage(id);
              }}
            >
              {name}
              {count === null ? null : <span className="count">{count}</span>}
            </button>
          );
        })}
      </nav>
      {content}
    </div>
  );
}
