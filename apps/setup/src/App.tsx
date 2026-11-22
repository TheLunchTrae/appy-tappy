import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';

import {
  type AppSummary,
  type AutomationSummary,
  DEFAULT_SETTINGS,
  type DraftSummary,
  type SettingsFile,
} from './lib/bindings.ts';
import {
  deleteDraft,
  errorText,
  forgetUnsavedClickPictures,
  listApps,
  listAutomations,
  listDrafts,
  runtimeRuns,
  saveDraft,
  loadSettings,
  saveSettings,
  syncOverlays,
} from './lib/api.ts';
import { Dialog } from './components/Dialog.tsx';
import { UpdateNotice } from './components/UpdateNotice.tsx';
import { ApplicationEditor } from './pages/ApplicationEditor.tsx';
import { ApplicationsPage } from './pages/ApplicationsPage.tsx';
import { AutomationEditor } from './pages/AutomationEditor.tsx';
import { AutomationsPage } from './pages/AutomationsPage.tsx';
import { CaptureFlow } from './pages/CaptureFlow.tsx';
import { DebugPage } from './pages/DebugPage.tsx';
import {
  type Ended,
  type Flow,
  type KeepEvent,
  type Keeping,
  type Made,
  type Opener,
  afterFlow,
  makesReference,
  applicationFor,
  heldInAll,
  holdsAutomation,
  keepStep,
  discardQuestion,
  leaveQuestion,
  resumedFlow,
  trail,
  valueFor,
  waiting,
} from './lib/flow.ts';
import { supersedes } from './lib/reads.ts';
import { runtimeAfter } from './lib/runtime.ts';
import { useCaptureHeld } from './lib/capture.ts';
import { background, useRepeat } from './lib/repeat.ts';
import { ReferencesPage } from './pages/ReferencesPage.tsx';
import { SettingsPage } from './pages/SettingsPage.tsx';
import { ValuesPage } from './pages/ValuesPage.tsx';
import { SettingsContext, type SettingsHandle, type SettingsUpdate, sameSettings } from './lib/settings.ts';
import { type Held, onLeave } from './lib/unsaved.ts';
import { type DraftSaver, type Resumed, draftsOf } from './lib/drafts.ts';

type Page = 'automations' | 'applications' | 'values' | 'references' | 'settings' | 'debug';

/**
 * What leaving an open flow is for: a page its tab opens, **Discard** or **Cancel** in the flow, which ends the open
 * flow alone, or closing the window.
 */
type Leaving = Page | 'discard' | 'close';

/**
 * The pages, Automations first since running them is what the app is for, then in the order something new is set
 * up: the application, then the values read from it. Settings come last, and Debug after them while debug details
 * are on.
 */
const PAGES: readonly { readonly id: Page; readonly name: string }[] = [
  { id: 'automations', name: 'Automations' },
  { id: 'applications', name: 'Applications' },
  { id: 'values', name: 'Values' },
  { id: 'references', name: 'References' },
  { id: 'settings', name: 'Settings' },
  { id: 'debug', name: 'Debug' },
];

/** The page that lists what each kind of flow saves. */
const LISTED_ON: Readonly<Record<Flow['kind'], Page>> = {
  automation: 'automations',
  application: 'applications',
  value: 'values',
  reference: 'references',
};

/**
 * How often the saved files are read again, which also refreshes which applications are running and whether the
 * background part runs.
 */
const REFRESH_MS = 3000;

export function App() {
  const [page, setPage] = useState<Page>('automations');
  const [flow, setFlow] = useState<Flow | null>(null);
  // Whether the open flow shows because another flow opened it or ended, which took the focused control with it.
  const [continued, setContinued] = useState(false);
  // What the open flow would lose, as it reports it, and what leaving it would lose with the flows that wait under it.
  const [reported, setReported] = useState<Held>('nothing');
  const held = heldInAll(flow, reported);
  // What a press asked to leave the flow for, while the question asks whether to.
  const [leaving, setLeaving] = useState<Leaving | null>(null);
  // The flow open now, for a save that finishes after the user moved on.
  const openFlow = useRef(flow);
  openFlow.current = flow;
  const [apps, setApps] = useState<readonly AppSummary[] | null>(null);
  // The reads of the saved lists, numbered as they start: the newest started, and the newest whose answer was
  // taken. `listed` is the number of the read `apps` is from.
  const reads = useRef({ started: 0, applied: 0 });
  const [listed, setListed] = useState(0);
  const [automations, setAutomations] = useState<readonly AutomationSummary[] | null>(null);
  const [drafts, setDrafts] = useState<readonly DraftSummary[] | null>(null);
  // Whether the background part runs, or null while that is not known, from the same reads as the lists.
  const [runtime, setRuntime] = useState<boolean | null>(null);
  // How the open flow saves its draft, which the leave question offers, and why its last try failed.
  const draftSaver = useRef<DraftSaver | null>(null);
  const [canDraft, setCanDraft] = useState(false);
  const [leaveError, setLeaveError] = useState<string | null>(null);
  // Whether leaving is saving the drafts, which the question's answers and everything in the open flow wait for.
  const keep = useRef<Keeping<Leaving>>({ kind: 'idle' });
  const [keeping, setKeeping] = useState(false);
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
    reads.current.started += 1;
    const read = reads.current.started;
    const [appsRead, automationsRead, draftsRead, runtimeRead] = await Promise.allSettled([
      listApps(),
      listAutomations(),
      listDrafts(),
      runtimeRuns(),
    ]);
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
    // Drafts that cannot be listed this time keep the last list, so their rows do not flicker away; none before
    // the first list shows as none, since the saved items are what matters.
    if (draftsRead.status === 'fulfilled') {
      setDrafts(draftsRead.value);
    } else {
      setDrafts((last) => last ?? []);
    }
    setRuntime(runtimeAfter(runtimeRead));
  }, []);
  // The overlays that are on open with the app.
  useEffect(() => {
    background(syncOverlays());
  }, []);
  // A click picture is kept as a file while its target is set up, and the automation's draft names it, also while
  // the editor waits under a value or application flow it opened. Once no automation flow is left, those that
  // neither the saved automations nor a saved draft name go, such as after Cancel.
  const automating = holdsAutomation(flow);
  useEffect(() => {
    if (!automating) {
      return undefined;
    }
    return () => {
      background(forgetUnsavedClickPictures());
    };
  }, [automating]);
  // A screen that holds the capture previews a window, so the lists wait until it lets go. Each save reads them at
  // once all the same.
  const capturing = useCaptureHeld();
  useRepeat(refresh, REFRESH_MS, !capturing);

  const counts: Readonly<Record<Page, number | null>> = {
    values: apps === null ? null : apps.reduce((total, app) => total + (app.file?.values.length ?? 0), 0),
    automations: automations === null ? null : automations.length,
    applications: apps === null ? null : apps.length,
    references: apps === null ? null : apps.reduce((total, app) => total + (app.file?.references.length ?? 0), 0),
    settings: null,
    debug: null,
  };

  const open = (next: Flow | null, after: 'page' | 'flow' = 'page'): void => {
    setLeaving(null);
    setLeaveError(null);
    setFlow(next);
    setContinued(after === 'flow');
  };
  const goTo = (target: Page): void => {
    setLeaving(null);
    setLeaveError(null);
    setFlow(null);
    setContinued(false);
    setPage(target);
  };
  let question = null;
  if (flow !== null) {
    question = leaving === 'discard' ? discardQuestion(flow, canDraft) : leaveQuestion(flow, reported, canDraft);
  }
  // Opens the flow that finishes `resumed`, on the saved item it changes when that is still there.
  const finish = (resumed: Resumed): void => {
    const next = resumedFlow(resumed, apps ?? [], automations ?? []);
    if (next !== null) {
      open(next);
    }
  };
  // Deletes a draft from its list. No flow is open then, so the click pictures it alone named go with it.
  const removeDraft = async (id: string): Promise<void> => {
    await deleteDraft(id);
    background(forgetUnsavedClickPictures());
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
        break;
      case 'wait':
        break;
    }
  };
  // Shows what follows the flow `ended` once it saved `made`, with 'drafted' saved or deleted its draft, or with
  // 'canceled' was canceled. What it saved is read into the lists at once, for the page or the flow that shows next.
  const end = (ended: Flow, made: Made | null | 'drafted' | 'canceled'): void => {
    const how: Ended =
      made === 'canceled' || made === 'drafted' ? { kind: made } : { kind: 'saved', made, read: reads.current.started };
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
    if (how.kind !== 'canceled') {
      background(refresh());
    }
  };
  // Does what leaving was for, once the user agreed or nothing was held: opens the page, ends the open flow alone,
  // or closes the window.
  const leaveFor = (target: Leaving): void => {
    const open = openFlow.current;
    if (target === 'discard') {
      setLeaving(null);
      if (open !== null) {
        end(open, 'canceled');
      }
    } else if (target === 'close') {
      background(getCurrentWindow().destroy());
    } else {
      goTo(target);
    }
  };
  // Leaving with Save as draft saves a draft of each flow that holds work, one after the other (`keepStep`). The
  // state lives in a ref beside the flag the window renders from, since the saves' answers arrive in callbacks of
  // an earlier render.
  const send = (event: KeepEvent<Leaving>): void => {
    const { state, effects } = keepStep(keep.current, event, openFlow.current, apps ?? []);
    keep.current = state;
    setKeeping(state.kind !== 'idle');
    for (const effect of effects) {
      switch (effect.kind) {
        case 'saveHere': {
          const save =
            draftSaver.current ?? (async () => Promise.resolve({ failed: 'It cannot be saved as a draft.' }));
          background(
            save().then((result) => {
              send({ kind: 'here', flow: effect.flow, result });
            }),
          );
          break;
        }
        case 'saveUnder':
          saveDraft(effect.id, effect.def, false).then(
            () => {
              send({ kind: 'under', flow: effect.flow, failed: null });
            },
            (error: unknown) => {
              send({ kind: 'under', flow: effect.flow, failed: errorText(error) });
            },
          );
          break;
        case 'adopt':
          // The answers that follow are for this flow, so it is the open one at once.
          openFlow.current = effect.flow;
          setFlow(effect.flow);
          break;
        case 'fail':
          setLeaveError(effect.text);
          background(refresh());
          break;
        case 'leave':
          leaveFor(effect.target);
          background(refresh());
          break;
      }
    }
  };
  // **Discard** or **Cancel** in the flow `ending` ends it once the user agrees to lose its own work, and never while
  // it saves. A flow that waits under it shows again as it was left, so only the open flow's work counts.
  const cancel = (ending: Flow): void => {
    switch (onLeave(reported)) {
      case 'leave':
        end(ending, 'canceled');
        break;
      case 'ask':
        setLeaving('discard');
        break;
      case 'wait':
        break;
    }
  };
  // **Save as draft** in the question before Discard saves the open flow's draft and ends that flow alone, as its
  // own Save as draft does.
  const draftAndDiscard = (): void => {
    const save = draftSaver.current;
    if (save === null) {
      return;
    }
    setKeeping(true);
    background(
      save().then((result) => {
        setKeeping(false);
        if ('failed' in result) {
          setLeaveError(`The draft could not be saved. ${result.failed}`);
          return;
        }
        setLeaving(null);
        const open = openFlow.current;
        if (open !== null) {
          end(open, 'drafted');
        }
      }),
    );
  };
  // Closing the window while a flow holds work asks first, as a tab does, and waits for a save under way.
  const heldNow = useRef(held);
  heldNow.current = held;
  useEffect(() => {
    const stop = getCurrentWindow().onCloseRequested((event) => {
      const now = onLeave(heldNow.current);
      if (now === 'leave') {
        return;
      }
      event.preventDefault();
      if (now === 'ask') {
        setLeaving('close');
      }
    });
    return () => {
      background(
        stop.then((unlisten) => {
          unlisten();
        }),
      );
    };
  }, []);
  // The saved applications that have a draft of changes, which the picker opens in place of the saved one.
  const draftedApps = new Set(
    draftsOf(drafts, 'application').flatMap(({ def }) => (def.changes === null ? [] : [def.changes])),
  );
  // What lets the flow `opener` open the application flow from its picker, and show again when that ends as its
  // picker stood.
  const pickerFor = (opener: Opener) => ({
    onAddApplication: (chosen: string | null): void => {
      open(applicationFor(opener, chosen, null, drafts), 'flow');
    },
    onEditApplication: (existing: AppSummary): void => {
      open(applicationFor(opener, existing.id, existing, drafts), 'flow');
    },
    draftedApps,
  });
  const path = flow === null ? [] : trail(flow);

  const content = (() => {
    if (flow?.kind === 'value' || flow?.kind === 'reference') {
      return (
        <CaptureFlow
          // A flow of the other kind starts over rather than take this one's place.
          key={flow.kind}
          flow={flow}
          trail={path}
          continued={continued}
          onCancel={() => {
            cancel(flow);
          }}
          onSaved={(made) => {
            end(flow, made);
          }}
          onDrafted={() => {
            end(flow, 'drafted');
          }}
          onMakeReference={
            flow.kind === 'value' && makesReference(flow)
              ? (reference, value) => {
                  open(
                    {
                      kind: 'reference',
                      existing: null,
                      app: reference.def.app,
                      resumed: reference,
                      forValue: value,
                    },
                    'flow',
                  );
                  background(refresh());
                }
              : null
          }
          {...pickerFor(flow)}
          onHeld={setReported}
          onDraft={reportDraft}
        />
      );
    }
    if (flow?.kind === 'application') {
      return (
        <ApplicationEditor
          existing={flow.existing}
          draft={flow.resumed}
          trail={path}
          onCancel={() => {
            cancel(flow);
          }}
          onSaved={(id) => {
            end(flow, { app: id, value: null, reference: null });
          }}
          onDrafted={() => {
            end(flow, 'drafted');
          }}
          onHeld={setReported}
          onDraft={reportDraft}
        />
      );
    }
    if (flow?.kind === 'automation') {
      return (
        <AutomationEditor
          apps={apps ?? []}
          listed={listed}
          readError={loadError}
          runtime={runtime}
          flow={flow}
          continued={continued}
          onCancel={() => {
            cancel(flow);
          }}
          onSaved={() => {
            end(flow, null);
          }}
          onDrafted={() => {
            end(flow, 'drafted');
          }}
          onAddApplication={(draft, app) => {
            open(applicationFor(waiting(flow, draft), app, null, drafts), 'flow');
          }}
          onNewValue={(draft, app) => {
            open(valueFor(waiting(flow, draft), app), 'flow');
          }}
          onHeld={setReported}
          onDraft={reportDraft}
        />
      );
    }
    switch (page) {
      case 'values':
        return (
          <ValuesPage
            apps={apps}
            drafts={drafts}
            automations={automations}
            loadError={loadError}
            onChanged={refresh}
            onNew={() => {
              open({ kind: 'value', changing: null, app: null, opener: null, resumed: null });
            }}
            onChange={(changing) => {
              open({ kind: 'value', changing, app: null, opener: null, resumed: null });
            }}
            onFinish={finish}
            onDeleteDraft={removeDraft}
          />
        );
      case 'automations':
        return (
          <AutomationsPage
            apps={apps}
            drafts={drafts}
            automations={automations}
            loadError={automationError ?? loadError}
            runtime={runtime}
            onChanged={refresh}
            onNew={() => {
              open({ kind: 'automation', existing: null, draft: null, app: null, handed: null, resumed: null });
            }}
            onEdit={(summary) => {
              open({
                kind: 'automation',
                existing: summary.automation,
                draft: null,
                app: null,
                handed: null,
                resumed: null,
              });
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
              open({ kind: 'application', existing: null, opener: null, resumed: null });
            }}
            onEdit={(app) => {
              open({ kind: 'application', existing: app, opener: null, resumed: null });
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
              open({ kind: 'reference', existing: null, app: null, resumed: null, forValue: null });
            }}
            onEdit={(existing) => {
              open({ kind: 'reference', existing, app: null, resumed: null, forValue: null });
            }}
            onFinish={finish}
            onDeleteDraft={removeDraft}
          />
        );
      case 'settings':
        return <SettingsPage problem={settingsProblem} />;
      case 'debug':
        // Turning debug details off on the Settings page hides the tab, so this shows only after a change made
        // elsewhere, such as in the file.
        return settings.showDebug ? <DebugPage /> : <SettingsPage problem={settingsProblem} />;
    }
  })();

  return (
    <SettingsContext value={settingsHandle}>
      <div className="shell">
        <nav className="nav" aria-label="Pages">
          {PAGES.filter(({ id }) => id !== 'debug' || settings.showDebug).map(({ id, name }) => {
            const count = counts[id];
            return (
              <button
                key={id}
                type="button"
                className="tab"
                aria-current={page === id && flow === null ? 'page' : undefined}
                disabled={held === 'saving' || keeping}
                onClick={() => {
                  pressTab(id);
                }}
              >
                {name}
                {count === null ? null : <span className="count">{count}</span>}
              </button>
            );
          })}
          <UpdateNotice busy={held !== 'nothing'} hidden={leaving !== null && flow !== null} />
        </nav>
        {leaving === null || question === null ? null : (
          <Dialog
            titleId="leave-title"
            describedBy="leave-what"
            busy={held === 'saving' || keeping}
            onClose={() => {
              setLeaving(null);
              setLeaveError(null);
            }}
          >
            <h2 id="leave-title">{question.title}</h2>
            <p id="leave-what">{question.text}</p>
            {leaveError === null ? null : (
              <p className="warn" role="alert">
                {leaveError}
              </p>
            )}
            <div className="dialog-actions">
              <button
                type="button"
                className="btn"
                disabled={held === 'saving' || keeping}
                onClick={() => {
                  setLeaving(null);
                  setLeaveError(null);
                }}
              >
                Stay
              </button>
              {question.drafts ? (
                <button
                  type="button"
                  className="btn"
                  aria-busy={keeping}
                  disabled={held === 'saving' || keeping}
                  onClick={() => {
                    if (leaving === 'discard') {
                      draftAndDiscard();
                    } else {
                      send({ kind: 'asked', target: leaving, here: reported === 'unsaved' && canDraft });
                    }
                  }}
                >
                  Save as draft
                </button>
              ) : null}
              <button
                type="button"
                className="btn danger"
                disabled={held === 'saving' || keeping}
                onClick={() => {
                  leaveFor(leaving);
                }}
              >
                {leaving === 'discard' ? 'Discard' : 'Leave'}
              </button>
            </div>
          </Dialog>
        )}
        {/* While leaving saves the drafts, nothing in the open flow can end or save it. */}
        <div className="flow-hold" inert={keeping}>
          {content}
        </div>
      </div>
    </SettingsContext>
  );
}
