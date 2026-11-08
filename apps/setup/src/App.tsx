import { useCallback, useEffect, useState } from 'react';

import {
  type AppSummary,
  type AutomationDef,
  type AutomationSummary,
  errorText,
  listApps,
  listAutomations,
  syncOverlays,
} from './lib/api.ts';
import { ApplicationEditor } from './pages/ApplicationEditor.tsx';
import { ApplicationsPage } from './pages/ApplicationsPage.tsx';
import { AutomationEditor } from './pages/AutomationEditor.tsx';
import { AutomationsPage } from './pages/AutomationsPage.tsx';
import { NewValue } from './pages/NewValue.tsx';
import { background, useRepeat } from './lib/repeat.ts';
import { ReferenceFlow, type SavedRef } from './pages/ReferenceFlow.tsx';
import { ReferencesPage } from './pages/ReferencesPage.tsx';
import { ValuesPage } from './pages/ValuesPage.tsx';

type Page = 'automations' | 'applications' | 'values' | 'references';

/**
 * The pages, Automations first since running them is what the app is for, then in the order something new is set
 * up: the application, then the values read from it.
 */
const PAGES: readonly { readonly id: Page; readonly name: string }[] = [
  { id: 'automations', name: 'Automations' },
  { id: 'applications', name: 'Applications' },
  { id: 'values', name: 'Values' },
  { id: 'references', name: 'References' },
];

/** What the window shows in place of a page: a flow that makes or changes something. */
type Flow =
  | { readonly kind: 'value' }
  | { readonly kind: 'automation'; readonly existing: AutomationDef | null }
  | { readonly kind: 'application'; readonly existing: AppSummary | null }
  | { readonly kind: 'reference'; readonly existing: SavedRef | null };

/** How often the saved files are read again, which also refreshes which applications are running. */
const REFRESH_MS = 3000;

export function App() {
  const [page, setPage] = useState<Page>('automations');
  const [flow, setFlow] = useState<Flow | null>(null);
  const [apps, setApps] = useState<readonly AppSummary[] | null>(null);
  const [automations, setAutomations] = useState<readonly AutomationSummary[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [automationError, setAutomationError] = useState<string | null>(null);

  const refresh = useCallback(async (): Promise<void> => {
    // The two files are read apart, so a broken automations file still lists the values.
    const [appsRead, automationsRead] = await Promise.allSettled([listApps(), listAutomations()]);
    if (appsRead.status === 'fulfilled') {
      setApps(appsRead.value);
      setLoadError(null);
    } else {
      setLoadError(errorText(appsRead.reason));
    }
    if (automationsRead.status === 'fulfilled') {
      setAutomations(automationsRead.value);
      setAutomationError(null);
    } else {
      setAutomationError(errorText(automationsRead.reason));
    }
  }, []);
  // The overlays that are on open with the app.
  useEffect(() => {
    background(syncOverlays());
  }, []);
  // A flow that makes a value or a reference captures a window, so the lists wait until it ends.
  useRepeat(refresh, REFRESH_MS, flow?.kind !== 'value' && flow?.kind !== 'reference');

  const counts: Readonly<Record<Page, number | null>> = {
    values: apps === null ? null : apps.reduce((total, app) => total + (app.file?.values.length ?? 0), 0),
    automations: automations === null ? null : automations.length,
    applications: apps === null ? null : apps.length,
    references: apps === null ? null : apps.reduce((total, app) => total + (app.file?.references.length ?? 0), 0),
  };

  const content = (() => {
    if (flow?.kind === 'value') {
      return (
        <NewValue
          apps={apps ?? []}
          onCancel={() => {
            setFlow(null);
          }}
          onSaved={() => {
            setFlow(null);
            setPage('values');
          }}
          onAddApplication={() => {
            setFlow({ kind: 'application', existing: null });
          }}
        />
      );
    }
    if (flow?.kind === 'application') {
      return (
        <ApplicationEditor
          existing={flow.existing}
          onCancel={() => {
            setFlow(null);
          }}
          onSaved={() => {
            setFlow(null);
            setPage('applications');
            background(refresh());
          }}
        />
      );
    }
    if (flow?.kind === 'reference') {
      return (
        <ReferenceFlow
          apps={apps ?? []}
          existing={flow.existing}
          onCancel={() => {
            setFlow(null);
          }}
          onSaved={() => {
            setFlow(null);
            setPage('references');
          }}
          onAddApplication={() => {
            setFlow({ kind: 'application', existing: null });
          }}
        />
      );
    }
    if (flow?.kind === 'automation') {
      return (
        <AutomationEditor
          apps={apps ?? []}
          existing={flow.existing}
          onCancel={() => {
            setFlow(null);
          }}
          onSaved={() => {
            setFlow(null);
            setPage('automations');
            background(refresh());
          }}
        />
      );
    }
    switch (page) {
      case 'values':
        return (
          <ValuesPage
            apps={apps}
            automations={automations}
            loadError={loadError}
            onChanged={refresh}
            onNew={() => {
              setFlow({ kind: 'value' });
            }}
          />
        );
      case 'automations':
        return (
          <AutomationsPage
            apps={apps}
            automations={automations}
            loadError={automationError ?? loadError}
            onChanged={refresh}
            onNew={() => {
              setFlow({ kind: 'automation', existing: null });
            }}
            onEdit={(summary) => {
              setFlow({ kind: 'automation', existing: summary.automation });
            }}
          />
        );
      case 'applications':
        return (
          <ApplicationsPage
            apps={apps}
            loadError={loadError}
            onChanged={refresh}
            onAdd={() => {
              setFlow({ kind: 'application', existing: null });
            }}
            onEdit={(app) => {
              setFlow({ kind: 'application', existing: app });
            }}
          />
        );
      case 'references':
        return (
          <ReferencesPage
            apps={apps}
            loadError={loadError}
            onChanged={refresh}
            onNew={() => {
              setFlow({ kind: 'reference', existing: null });
            }}
            onEdit={(existing) => {
              setFlow({ kind: 'reference', existing });
            }}
          />
        );
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
              aria-current={page === id && flow === null ? 'page' : undefined}
              onClick={() => {
                setFlow(null);
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
