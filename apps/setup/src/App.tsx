import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import {
  type AppSummary,
  type AutomationDef,
  type AutomationSummary,
  DEFAULT_SETTINGS,
  type DraftSummary,
  type SettingsFile,
} from './lib/bindings.ts';
import {
  deleteDraft,
  errorText,
  listApps,
  listAutomations,
  listDrafts,
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
import { type Fixing, NewValue } from './pages/NewValue.tsx';
import { background, useRepeat } from './lib/repeat.ts';
import { ReferenceFlow, type SavedRef } from './pages/ReferenceFlow.tsx';
import { ReferencesPage } from './pages/ReferencesPage.tsx';
import { SettingsPage } from './pages/SettingsPage.tsx';
import { ValuesPage } from './pages/ValuesPage.tsx';
import { SettingsContext, type SettingsHandle, type SettingsUpdate, sameSettings } from './lib/settings.ts';
import { type Held, onLeave } from './lib/unsaved.ts';
import type { DraftSaver, Resumed } from './lib/drafts.ts';

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

/**
 * What the window shows in place of a page: a flow that makes or changes something, and the draft it finishes, or
 * null.
 */
type Flow = (
  | { readonly kind: 'value'; readonly fixing: Fixing | null }
  | { readonly kind: 'automation'; readonly existing: AutomationDef | null }
  | { readonly kind: 'application'; readonly existing: AppSummary | null }
  | { readonly kind: 'reference'; readonly existing: SavedRef | null }
) & { readonly draft: Resumed | null };

/** What leaving each kind of flow before it is saved loses, and that a draft can keep it. */
const LOST: Readonly<Record<Flow['kind'], string>> = {
  value:
    'Leave this value? Save it as a draft to finish later, or leave and lose the screenshots and the region. A draft does not keep a recording.',
  reference:
    'Leave this reference? Save it as a draft to finish later, or leave and lose the screenshots and labels. A draft does not keep a recording.',
  automation: 'Leave this automation? Save it as a draft to finish later, or leave and lose the changes.',
  application: 'Leave this application? Save it as a draft to finish later, or leave and lose the changes.',
};

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
  const [drafts, setDrafts] = useState<readonly DraftSummary[] | null>(null);
  // How the open flow saves its draft, which the leave question offers, and why its last try failed.
  const draftSaver = useRef<DraftSaver | null>(null);
  const [canDraft, setCanDraft] = useState(false);
  const [leaveError, setLeaveError] = useState<string | null>(null);
  const reportDraft = useCallback((save: DraftSaver | null): void => {
    draftSaver.current = save;
    setCanDraft(save !== null);
  }, []);
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
    // Each is read apart, so a broken automations file still lists the values.
    const [appsRead, automationsRead, draftsRead] = await Promise.allSettled([
      listApps(),
      listAutomations(),
      listDrafts(),
    ]);
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
    // Drafts that cannot be listed this time keep the last list, so their rows do not flicker away; none before
    // the first list shows as none, since the saved items are what matters.
    if (draftsRead.status === 'fulfilled') {
      setDrafts(draftsRead.value);
    } else {
      setDrafts((last) => last ?? []);
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
    setLeaveError(null);
    setFlow(next);
  };
  const goTo = (target: Page): void => {
    setLeaving(null);
    setLeaveError(null);
    setFlow(null);
    setPage(target);
  };
  // Saves the open flow's draft, then leaves for the page the press asked for.
  const leaveWithDraft = async (target: Page): Promise<void> => {
    const save = draftSaver.current;
    const opened = openFlow.current;
    if (save === null) {
      return;
    }
    const failed = await save();
    if (openFlow.current !== opened) {
      // The flow closed while its draft saved; the lists show the draft at the next read.
      return;
    }
    if (failed === null) {
      goTo(target);
      await refresh();
    } else {
      setLeaveError(`The draft could not be saved. ${failed}`);
    }
  };
  // Opens the flow that finishes `resumed`, on the saved item it changes when that is still there.
  const finish = (resumed: Resumed): void => {
    const { def } = resumed;
    const changed = def.changes;
    switch (def.kind) {
      case 'automation':
        open({
          kind: 'automation',
          existing: automations?.find((summary) => summary.automation.id === changed)?.automation ?? null,
          draft: resumed,
        });
        break;
      case 'application':
        open({ kind: 'application', existing: apps?.find((app) => app.id === changed) ?? null, draft: resumed });
        break;
      case 'value':
        open({ kind: 'value', fixing: null, draft: resumed });
        break;
      case 'fix': {
        const value = apps?.find((app) => app.id === def.app)?.file?.values.find((each) => each.id === changed);
        if (def.app !== null && value !== undefined) {
          open({ kind: 'value', fixing: { app: def.app, value }, draft: resumed });
        }
        break;
      }
      case 'reference': {
        const reference = apps?.find((app) => app.id === def.app)?.file?.references.find((each) => each.id === changed);
        open({
          kind: 'reference',
          existing: def.app === null || reference === undefined ? null : { app: def.app, reference },
          draft: resumed,
        });
        break;
      }
    }
  };
  const removeDraft = async (id: string): Promise<void> => {
    await deleteDraft(id);
    await refresh();
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
  // A save closes its own flow, and nothing once the user has left it for another. It may have saved or removed a
  // draft, so the lists are read again.
  const saved = (opened: Flow, target: Page): void => {
    if (openFlow.current === opened) {
      goTo(target);
    }
    background(refresh());
  };

  const content = (() => {
    if (flow?.kind === 'value') {
      return (
        <NewValue
          apps={apps ?? []}
          fixing={flow.fixing}
          draft={flow.draft}
          onCancel={() => {
            open(null);
          }}
          onSaved={() => {
            saved(flow, 'values');
          }}
          onAddApplication={() => {
            open({ kind: 'application', existing: null, draft: null });
          }}
          onHeld={setHeld}
          onDraft={reportDraft}
        />
      );
    }
    if (flow?.kind === 'application') {
      return (
        <ApplicationEditor
          existing={flow.existing}
          draft={flow.draft}
          onCancel={() => {
            open(null);
          }}
          onSaved={() => {
            saved(flow, 'applications');
          }}
          onHeld={setHeld}
          onDraft={reportDraft}
        />
      );
    }
    if (flow?.kind === 'reference') {
      return (
        <ReferenceFlow
          apps={apps ?? []}
          existing={flow.existing}
          draft={flow.draft}
          onCancel={() => {
            open(null);
          }}
          onSaved={() => {
            saved(flow, 'references');
          }}
          onAddApplication={() => {
            open({ kind: 'application', existing: null, draft: null });
          }}
          onHeld={setHeld}
          onDraft={reportDraft}
        />
      );
    }
    if (flow?.kind === 'automation') {
      return (
        <AutomationEditor
          apps={apps ?? []}
          existing={flow.existing}
          draft={flow.draft}
          onCancel={() => {
            open(null);
          }}
          onSaved={() => {
            saved(flow, 'automations');
          }}
          onHeld={setHeld}
          onDraft={reportDraft}
        />
      );
    }
    switch (page) {
      case 'values':
        return (
          <ValuesPage
            apps={apps}
            automations={automations}
            drafts={drafts}
            loadError={loadError}
            onChanged={refresh}
            onNew={() => {
              open({ kind: 'value', fixing: null, draft: null });
            }}
            onFix={(fixing) => {
              open({ kind: 'value', fixing, draft: null });
            }}
            onFinish={finish}
            onDeleteDraft={removeDraft}
          />
        );
      case 'automations':
        return (
          <AutomationsPage
            apps={apps}
            automations={automations}
            drafts={drafts}
            loadError={automationError ?? loadError}
            onChanged={refresh}
            onNew={() => {
              open({ kind: 'automation', existing: null, draft: null });
            }}
            onEdit={(summary) => {
              open({ kind: 'automation', existing: summary.automation, draft: null });
            }}
            onFinish={finish}
            onDeleteDraft={removeDraft}
          />
        );
      case 'applications':
        return (
          <ApplicationsPage
            apps={apps}
            drafts={drafts}
            loadError={loadError}
            onChanged={refresh}
            onAdd={() => {
              open({ kind: 'application', existing: null, draft: null });
            }}
            onEdit={(app) => {
              open({ kind: 'application', existing: app, draft: null });
            }}
            onFinish={finish}
            onDeleteDraft={removeDraft}
          />
        );
      case 'references':
        return (
          <ReferencesPage
            apps={apps}
            drafts={drafts}
            loadError={loadError}
            onChanged={refresh}
            onNew={() => {
              open({ kind: 'reference', existing: null, draft: null });
            }}
            onEdit={(existing) => {
              open({ kind: 'reference', existing, draft: null });
            }}
            onFinish={finish}
            onDeleteDraft={removeDraft}
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
              label={canDraft ? `Leave this ${flow.kind}?` : 'Leave without saving?'}
              keepLabel="Stay"
              actLabel="Leave"
              busy={held === 'saving'}
              middle={
                canDraft
                  ? {
                      label: 'Save as draft',
                      onPress: () => {
                        background(leaveWithDraft(leaving));
                      },
                    }
                  : undefined
              }
              onKeep={() => {
                setLeaving(null);
                setLeaveError(null);
              }}
              onAct={() => {
                goTo(leaving);
              }}
            >
              {leaveError ?? (canDraft ? LOST[flow.kind] : 'Leave without saving? What it has set up is lost.')}
            </Question>
          )}
          <UpdateNotice busy={held !== 'nothing'} hidden={leaving !== null && flow !== null} />
        </nav>
        {content}
      </div>
    </SettingsContext>
  );
}
