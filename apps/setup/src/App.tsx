import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { type AppSummary, type AutomationSummary, DEFAULT_SETTINGS, type SettingsFile } from './lib/bindings.ts';
import { errorText, listApps, listAutomations, loadSettings, saveSettings, syncOverlays } from './lib/api.ts';
import { Question, useQuestion } from './components/Question.tsx';
import { UpdateNotice } from './components/UpdateNotice.tsx';
import { ApplicationEditor } from './pages/ApplicationEditor.tsx';
import { ApplicationsPage } from './pages/ApplicationsPage.tsx';
import { AutomationEditor } from './pages/AutomationEditor.tsx';
import { AutomationsPage } from './pages/AutomationsPage.tsx';
import { NewValue } from './pages/NewValue.tsx';
import {
  type Flow,
  type Made,
  type Opener,
  afterSave,
  applicationFor,
  heldInAll,
  lostText,
  resume,
  trail,
  valueFor,
  waiting,
} from './lib/flow.ts';
import { background, useRepeat } from './lib/repeat.ts';
import { ReferenceFlow } from './pages/ReferenceFlow.tsx';
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

/** The page that lists what each kind of flow saves. */
const LISTED_ON: Readonly<Record<Flow['kind'], Page>> = {
  automation: 'automations',
  application: 'applications',
  value: 'values',
  reference: 'references',
};

/** How often the saved files are read again, which also refreshes which applications are running. */
const REFRESH_MS = 3000;

export function App() {
  const [page, setPage] = useState<Page>('automations');
  const [flow, setFlow] = useState<Flow | null>(null);
  // What the open flow would lose, as it reports it, and what leaving it would lose with the flows that wait under it.
  const [reported, setReported] = useState<Held>('nothing');
  const held = heldInAll(flow, reported);
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
  // A save closes its own flow, for the flow that opened it or else the page that lists what it saved, and
  // nothing once the user has left it for another.
  const saved = (ended: Flow, made: Made | null = null): void => {
    const next = afterSave(openFlow.current, ended, made);
    switch (next.kind) {
      case 'flow':
        open(next.flow);
        break;
      case 'page':
        goTo(LISTED_ON[ended.kind]);
        break;
      case 'stay':
        break;
    }
  };
  // The save of a value or reference flow. When a flow waits for what it made, the lists are read first, so that
  // flow finds it in them.
  const captured = async (ended: Flow, made: Made): Promise<void> => {
    if (resume(ended, null) !== null) {
      await refresh();
    }
    saved(ended, made);
  };
  // What lets the flow `opener` open the application flow from its picker and show again when that ends.
  const pickerFor = (opener: Opener) => ({
    onAddApplication: (): void => {
      open(applicationFor(opener, null));
    },
    onEditApplication: (id: string): void => {
      const existing = apps?.find((app) => app.id === id);
      if (existing !== undefined) {
        open(applicationFor(opener, existing));
      }
    },
  });
  const path = flow === null ? [] : trail(flow);

  const content = (() => {
    if (flow?.kind === 'value') {
      return (
        <NewValue
          apps={apps ?? []}
          fixing={flow.fixing}
          app={flow.app}
          trail={path}
          onCancel={() => {
            open(resume(flow, null));
          }}
          onSaved={(made) => captured(flow, made)}
          {...pickerFor(flow)}
          onHeld={setReported}
        />
      );
    }
    if (flow?.kind === 'application') {
      return (
        <ApplicationEditor
          existing={flow.existing}
          trail={path}
          onCancel={() => {
            open(resume(flow, null));
          }}
          onSaved={async (id) => {
            // The lists are read first, so the flow that opened this one finds the application in them.
            await refresh();
            saved(flow, { app: id, value: null });
          }}
          onHeld={setReported}
        />
      );
    }
    if (flow?.kind === 'reference') {
      return (
        <ReferenceFlow
          apps={apps ?? []}
          existing={flow.existing}
          app={flow.app}
          trail={path}
          onCancel={() => {
            open(null);
          }}
          onSaved={(made) => captured(flow, made)}
          {...pickerFor(flow)}
          onHeld={setReported}
        />
      );
    }
    if (flow?.kind === 'automation') {
      return (
        <AutomationEditor
          apps={apps ?? []}
          flow={flow}
          onCancel={() => {
            open(null);
          }}
          onSaved={() => {
            saved(flow);
            background(refresh());
          }}
          onAddApplication={(draft, app) => {
            open(applicationFor(waiting(flow, draft, app), null));
          }}
          onNewValue={(draft, app) => {
            open(valueFor(waiting(flow, draft, app)));
          }}
          onHeld={setReported}
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
              open({ kind: 'value', fixing: null, app: null, opener: null });
            }}
            onFix={(fixing) => {
              open({ kind: 'value', fixing, app: null, opener: null });
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
              open({ kind: 'automation', existing: null, draft: null, app: null, value: null });
            }}
            onEdit={(summary) => {
              open({ kind: 'automation', existing: summary.automation, draft: null, app: null, value: null });
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
              open({ kind: 'application', existing: null, opener: null });
            }}
            onEdit={(app) => {
              open({ kind: 'application', existing: app, opener: null });
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
              open({ kind: 'reference', existing: null, app: null });
            }}
            onEdit={(existing) => {
              open({ kind: 'reference', existing, app: null });
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
              {lostText(flow, reported)}
            </Question>
          )}
          <UpdateNotice busy={held !== 'nothing'} hidden={leaving !== null && flow !== null} />
        </nav>
        {content}
      </div>
    </SettingsContext>
  );
}
