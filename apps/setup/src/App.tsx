import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { type AppSummary, type AutomationSummary, DEFAULT_SETTINGS, type SettingsFile } from './lib/bindings.ts';
import {
  errorText,
  forgetUnsavedClickPictures,
  listApps,
  listAutomations,
  loadSettings,
  saveSettings,
  syncOverlays,
} from './lib/api.ts';
import { Question, useQuestion } from './components/Question.tsx';
import { UpdateNotice } from './components/UpdateNotice.tsx';
import { ApplicationEditor } from './pages/ApplicationEditor.tsx';
import { ApplicationsPage } from './pages/ApplicationsPage.tsx';
import { AutomationEditor } from './pages/AutomationEditor.tsx';
import { AutomationsPage } from './pages/AutomationsPage.tsx';
import { NewValue } from './pages/NewValue.tsx';
import {
  type Ended,
  type Flow,
  type Made,
  type Opener,
  afterFlow,
  applicationFor,
  heldInAll,
  holdsAutomation,
  lostText,
  trail,
  valueFor,
  waiting,
} from './lib/flow.ts';
import { supersedes } from './lib/reads.ts';
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
  // Whether the open flow shows because another flow opened it or ended, which took the focused control with it.
  const [continued, setContinued] = useState(false);
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
  // The reads of the saved lists, numbered as they start: the newest started, and the newest whose answer was
  // taken. `listed` is the number of the read `apps` is from.
  const reads = useRef({ started: 0, applied: 0 });
  const [listed, setListed] = useState(0);
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
    reads.current.started += 1;
    const read = reads.current.started;
    const [appsRead, automationsRead] = await Promise.allSettled([listApps(), listAutomations()]);
    // Reads overlap, so an answer that arrives after a newer one would put older lists back.
    if (!supersedes(read, reads.current.applied)) {
      return;
    }
    reads.current.applied = read;
    if (appsRead.status === 'fulfilled') {
      setApps(appsRead.value);
      setListed(read);
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
  // A click picture is kept as a file while its target is set up, and the automation's draft names it, also while
  // the editor waits under a value or application flow it opened. Once no automation flow is left, those that the
  // saved automations do not name go, such as after Cancel.
  const automating = holdsAutomation(flow);
  useEffect(() => {
    if (!automating) {
      return undefined;
    }
    return () => {
      background(forgetUnsavedClickPictures());
    };
  }, [automating]);
  // A flow that makes a value or a reference captures a window, so the lists wait until it ends.
  useRepeat(refresh, REFRESH_MS, flow?.kind !== 'value' && flow?.kind !== 'reference');

  const counts: Readonly<Record<Page, number | null>> = {
    values: apps === null ? null : apps.reduce((total, app) => total + (app.file?.values.length ?? 0), 0),
    automations: automations === null ? null : automations.length,
    applications: apps === null ? null : apps.length,
    references: apps === null ? null : apps.reduce((total, app) => total + (app.file?.references.length ?? 0), 0),
    settings: null,
  };

  const open = (next: Flow | null, after: 'page' | 'flow' = 'page'): void => {
    setLeaving(null);
    setFlow(next);
    setContinued(after === 'flow');
  };
  const goTo = (target: Page): void => {
    setLeaving(null);
    setFlow(null);
    setContinued(false);
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
  // Shows what follows the flow `ended` once it saved `made` or, with 'canceled', was canceled. What it saved is
  // read into the lists at once, for the page or the flow that shows next.
  const end = (ended: Flow, made: Made | null | 'canceled'): void => {
    const how: Ended =
      made === 'canceled' ? { kind: 'canceled' } : { kind: 'saved', made, read: reads.current.started };
    const after = afterFlow(ended, how, openFlow.current === ended);
    switch (after.kind) {
      case 'nothing':
        break;
      case 'flow':
        open(after.flow, 'flow');
        break;
      case 'listed':
        goTo(LISTED_ON[ended.kind]);
        break;
      case 'page':
        open(null);
        break;
    }
    if (how.kind === 'saved') {
      background(refresh());
    }
  };
  // What lets the flow `opener` open the application flow from its picker, and show again when that ends as its
  // picker stood.
  const pickerFor = (opener: Opener) => ({
    onAddApplication: (chosen: string | null): void => {
      open(applicationFor(opener, chosen, null), 'flow');
    },
    onEditApplication: (existing: AppSummary): void => {
      open(applicationFor(opener, existing.id, existing), 'flow');
    },
  });
  const path = flow === null ? [] : trail(flow);

  const content = (() => {
    if (flow?.kind === 'value') {
      return (
        <NewValue
          fixing={flow.fixing}
          app={flow.app}
          trail={path}
          continued={continued}
          onCancel={() => {
            end(flow, 'canceled');
          }}
          onSaved={(made) => {
            end(flow, made);
          }}
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
            end(flow, 'canceled');
          }}
          onSaved={(id) => {
            end(flow, { app: id, value: null });
          }}
          onHeld={setReported}
        />
      );
    }
    if (flow?.kind === 'reference') {
      return (
        <ReferenceFlow
          existing={flow.existing}
          app={flow.app}
          trail={path}
          continued={continued}
          onCancel={() => {
            end(flow, 'canceled');
          }}
          onSaved={(made) => {
            end(flow, made);
          }}
          {...pickerFor(flow)}
          onHeld={setReported}
        />
      );
    }
    if (flow?.kind === 'automation') {
      return (
        <AutomationEditor
          apps={apps ?? []}
          listed={listed}
          readError={loadError}
          flow={flow}
          continued={continued}
          onCancel={() => {
            end(flow, 'canceled');
          }}
          onSaved={() => {
            end(flow, null);
          }}
          onAddApplication={(draft, app) => {
            open(applicationFor(waiting(flow, draft), app, null), 'flow');
          }}
          onNewValue={(draft, app) => {
            open(valueFor(waiting(flow, draft), app), 'flow');
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
              open({ kind: 'automation', existing: null, draft: null, app: null, handed: null });
            }}
            onEdit={(summary) => {
              open({ kind: 'automation', existing: summary.automation, draft: null, app: null, handed: null });
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
