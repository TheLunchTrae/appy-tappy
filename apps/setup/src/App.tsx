import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import {
  type AppSummary,
  type AutomationDef,
  type AutomationSummary,
  DEFAULT_SETTINGS,
  type SettingsFile,
} from './lib/bindings.ts';
import { errorText, listApps, listAutomations, loadSettings, saveSettings, syncOverlays } from './lib/api.ts';
import { Question, useQuestion } from './components/Question.tsx';
import { UpdateNotice } from './components/UpdateNotice.tsx';
import { ApplicationEditor } from './pages/ApplicationEditor.tsx';
import { ApplicationsPage } from './pages/ApplicationsPage.tsx';
import { AutomationEditor } from './pages/AutomationEditor.tsx';
import { AutomationsPage } from './pages/AutomationsPage.tsx';
import { type Changing, NewValue } from './pages/NewValue.tsx';
import { background, useRepeat } from './lib/repeat.ts';
import { ReferenceFlow, type SavedRef } from './pages/ReferenceFlow.tsx';
import { ReferencesPage } from './pages/ReferencesPage.tsx';
import { SettingsPage } from './pages/SettingsPage.tsx';
import { ValuesPage } from './pages/ValuesPage.tsx';
import { SettingsContext, type SettingsHandle, type SettingsUpdate, sameSettings } from './lib/settings.ts';
import { type Held, onLeave } from './lib/unsaved.ts';

type Page = 'automations' | 'applications' | 'values' | 'references' | 'settings';

/**
 * The pages, Automations first since running them is what the app is for, then in the order something new is set
 * up: the application, then the values read from it. Settings come last.
 */
const PAGES: readonly { readonly id: Page; readonly name: string }[] = [
  { id: 'automations', name: 'Automations' },
  { id: 'applications', name: 'Applications' },
  { id: 'values', name: 'Values' },
  { id: 'references', name: 'References' },
  { id: 'settings', name: 'Settings' },
];

/** What the window shows in place of a page: a flow that makes or changes something. */
type Flow =
  | { readonly kind: 'value'; readonly changing: Changing | null }
  | { readonly kind: 'automation'; readonly existing: AutomationDef | null }
  | { readonly kind: 'application'; readonly existing: AppSummary | null }
  | { readonly kind: 'reference'; readonly existing: SavedRef | null };

/** What leaving each kind of flow before it is saved loses. */
const LOST: Readonly<Record<Flow['kind'], string>> = {
  value: 'Leave without saving? The screenshots, the region and any recording are lost.',
  reference: 'Leave without saving? The screenshots, the labels and any recording are lost.',
  automation: 'Leave without saving? The changes to this automation are lost.',
  application: 'Leave without saving? The changes to this application are lost.',
};

/** What leaving an edit of a saved value before it is saved loses: the value itself stays as it is saved. */
const EDIT_LOST = 'Leave without saving? The screenshots and the changes to this value are lost.';

/** What leaving `flow` before it is saved loses. */
function lostText(flow: Flow): string {
  return flow.kind === 'value' && flow.changing?.how === 'edit' ? EDIT_LOST : LOST[flow.kind];
}

/** How often the saved files are read again, which also refreshes which applications are running. */
const REFRESH_MS = 3000;

export function App() {
  const [page, setPage] = useState<Page>('automations');
  const [flow, setFlow] = useState<Flow | null>(null);
  // What the open flow would lose, as it reports it.
  const [held, setHeld] = useState<Held>('nothing');
  // The page a press asked to leave the flow for, while the question asks whether to; and the last one asked for,
  // whose tab takes focus back after the answer.
  const [leaving, setLeaving] = useState<Page | null>(null);
  const [askedFor, setAskedFor] = useState<Page>('automations');
  const { control, refs } = useQuestion<HTMLButtonElement>(leaving !== null);
  // The flow open now, for a save that finishes after the user moved on.
  const openFlow = useRef(flow);
  openFlow.current = flow;
  const [apps, setApps] = useState<readonly AppSummary[] | null>(null);
  const [automations, setAutomations] = useState<readonly AutomationSummary[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [automationError, setAutomationError] = useState<string | null>(null);
  const [settings, setSettings] = useState<SettingsFile>(DEFAULT_SETTINGS);
  const [settingsProblem, setSettingsProblem] = useState<string | null>(null);
  // The saved settings as the last save left them, and the changes still to run, one after another, after the load.
  const savedSettings = useRef<SettingsFile>(DEFAULT_SETTINGS);
  const [settingsLoad] = useState(() => Promise.withResolvers<undefined>());
  const settingsQueue = useRef<Promise<unknown>>(settingsLoad.promise);
  useEffect(() => {
    loadSettings()
      .then(
        (view) => {
          savedSettings.current = view.settings;
          setSettings(view.settings);
          setSettingsProblem(view.problem);
        },
        (error: unknown) => {
          setSettingsProblem(errorText(error));
        },
      )
      .finally(() => {
        settingsLoad.resolve(undefined);
      });
  }, [settingsLoad]);
  const settingsHandle = useMemo<SettingsHandle>(
    () => ({
      settings,
      change: async (update: SettingsUpdate): Promise<string | null> => {
        const run = async (): Promise<string | null> => {
          const next = update(savedSettings.current);
          if ('problem' in next) {
            return next.problem;
          }
          // A second ask for the same change, such as Enter and then leaving the field, saves nothing.
          if (!sameSettings(next, savedSettings.current)) {
            await saveSettings(next);
            savedSettings.current = next;
            setSettings(next);
          }
          setSettingsProblem(null);
          return null;
        };
        const result = settingsQueue.current.then(run);
        settingsQueue.current = result.catch(() => {
          // The caller hears of the failure; the next change runs either way.
        });
        return result;
      },
    }),
    [settings],
  );

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
    settings: null,
  };

  const open = (next: Flow | null): void => {
    setLeaving(null);
    setFlow(next);
  };
  const goTo = (target: Page): void => {
    setLeaving(null);
    setFlow(null);
    setPage(target);
  };
  // A tab leaves an open flow only once the user agrees to lose its work, and never while it saves.
  const pressTab = (target: Page): void => {
    switch (onLeave(held)) {
      case 'leave':
        goTo(target);
        break;
      case 'ask':
        setLeaving(target);
        setAskedFor(target);
        break;
      case 'wait':
        break;
    }
  };
  // A save closes its own flow, and nothing once the user has left it for another.
  const saved = (opened: Flow, target: Page): void => {
    if (openFlow.current === opened) {
      goTo(target);
    }
  };

  const content = (() => {
    if (flow?.kind === 'value') {
      return (
        <NewValue
          apps={apps ?? []}
          changing={flow.changing}
          onCancel={() => {
            open(null);
          }}
          onSaved={() => {
            saved(flow, 'values');
          }}
          onAddApplication={() => {
            open({ kind: 'application', existing: null });
          }}
          onHeld={setHeld}
        />
      );
    }
    if (flow?.kind === 'application') {
      return (
        <ApplicationEditor
          existing={flow.existing}
          onCancel={() => {
            open(null);
          }}
          onSaved={() => {
            saved(flow, 'applications');
            background(refresh());
          }}
          onHeld={setHeld}
        />
      );
    }
    if (flow?.kind === 'reference') {
      return (
        <ReferenceFlow
          apps={apps ?? []}
          existing={flow.existing}
          onCancel={() => {
            open(null);
          }}
          onSaved={() => {
            saved(flow, 'references');
          }}
          onAddApplication={() => {
            open({ kind: 'application', existing: null });
          }}
          onHeld={setHeld}
        />
      );
    }
    if (flow?.kind === 'automation') {
      return (
        <AutomationEditor
          apps={apps ?? []}
          existing={flow.existing}
          onCancel={() => {
            open(null);
          }}
          onSaved={() => {
            saved(flow, 'automations');
            background(refresh());
          }}
          onHeld={setHeld}
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
              open({ kind: 'value', changing: null });
            }}
            onChange={(changing) => {
              open({ kind: 'value', changing });
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
              open({ kind: 'automation', existing: null });
            }}
            onEdit={(summary) => {
              open({ kind: 'automation', existing: summary.automation });
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
              open({ kind: 'application', existing: null });
            }}
            onEdit={(app) => {
              open({ kind: 'application', existing: app });
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
              open({ kind: 'reference', existing: null });
            }}
            onEdit={(existing) => {
              open({ kind: 'reference', existing });
            }}
          />
        );
      case 'settings':
        return <SettingsPage problem={settingsProblem} />;
    }
  })();

  return (
    <SettingsContext value={settingsHandle}>
      <div className="shell">
        <nav className="nav" aria-label="Pages">
          {PAGES.map(({ id, name }) => {
            const count = counts[id];
            return (
              <button
                key={id}
                type="button"
                className="tab"
                ref={id === askedFor ? control : undefined}
                aria-current={page === id && flow === null ? 'page' : undefined}
                disabled={held === 'saving'}
                onClick={() => {
                  pressTab(id);
                }}
              >
                {name}
                {count === null ? null : <span className="count">{count}</span>}
              </button>
            );
          })}
          {leaving === null || flow === null ? null : (
            <Question
              refs={refs}
              label="Leave without saving?"
              keepLabel="Stay"
              actLabel="Leave"
              busy={held === 'saving'}
              onKeep={() => {
                setLeaving(null);
              }}
              onAct={() => {
                goTo(leaving);
              }}
            >
              {lostText(flow)}
            </Question>
          )}
          <UpdateNotice busy={held !== 'nothing'} hidden={leaving !== null && flow !== null} />
        </nav>
        {content}
      </div>
    </SettingsContext>
  );
}
