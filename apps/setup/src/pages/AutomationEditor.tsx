import { useEffect, useMemo, useRef, useState } from 'react';

import { type AppSummary, type AutomationDef, type MediaCommandDef, NO_STATE, type WhenDef } from '../lib/bindings.ts';
import { errorText, listWindows, saveAutomation, stopWatching, watchValue, watchedReading } from '../lib/api.ts';
import {
  type ActionDraft,
  type Draft,
  type LiveState,
  COMPARISONS,
  COMPARISON_NAMES,
  EMPTY_DRAFT,
  MEDIA_NAMES,
  type ValueChoice,
  buildAutomation,
  buildWhen,
  clicksBlocked,
  findValue,
  keysBlocked,
  liveAnnouncement,
  liveText,
  newAction,
  draftPictures,
  readsState,
  readsText,
  toDraft,
  undoText,
  unfinished,
  valueChoices,
  withSource,
} from '../lib/automation.ts';
import { clickApps, targetSummary } from '../lib/click.ts';
import { ACTIONS_HINT, COOLDOWN_HINT, DRAFT_HINT, UNDO_HINT, WHEN_HINT } from '../lib/hints.ts';
import { background, useRepeat } from '../lib/repeat.ts';
import { ClickPictureThumb, ClickTarget } from '../components/ClickTarget.tsx';
import { DeleteDraft } from '../components/Drafts.tsx';
import { Hint } from '../components/Hint.tsx';
import { type Held, held, useReportHeld } from '../lib/unsaved.ts';
import { type Kept, type ReportDraft, type Resumed, isKept, keep, useDraft } from '../lib/drafts.ts';
import { actionKeys } from '../lib/shortcut.ts';

/** What a draft of an automation keeps: the editor's state. */
type AutomationKept = Kept<'automation', Draft>;

/** The editor's state as `resumed` keeps it, or else as `existing` is saved, or a new automation's. */
function startDraft(existing: AutomationDef | null, resumed: Resumed | null): Draft {
  if (resumed !== null && isKept<AutomationKept>(resumed.def.state, 'automation')) {
    return resumed.def.state.state;
  }
  return existing === null ? EMPTY_DRAFT : toDraft(existing);
}

/** The action menu's choices: each media command, then setting a volume, pressing keys, opening, and playing a sound. */
type ActionChoice = MediaCommandDef | Exclude<ActionDraft['kind'], 'media'>;

const ACTION_CHOICES: readonly ActionChoice[] = [
  'pause',
  'play',
  'next',
  'previous',
  'volume',
  'keys',
  'open',
  'sound',
  'click',
];

const ACTION_NAMES: Readonly<Record<ActionChoice, string>> = {
  ...MEDIA_NAMES,
  volume: 'Set volume',
  keys: 'Press keys',
  open: 'Open a program, file, or link',
  sound: 'Play a sound',
  click: 'Click in another application',
};

interface KeysFieldProps {
  readonly label: string;
  readonly keys: string;
  readonly onChange: (keys: string) => void;
}

/**
 * The keys an action presses: typed, such as `Ctrl+Shift+M`, or recorded from the next keys pressed.
 * While it records, every key but Escape goes to the recording, Tab included; Escape stops it.
 */
function KeysField({ label, keys, onChange }: KeysFieldProps) {
  const [recording, setRecording] = useState(false);
  const [heard, setHeard] = useState('');
  return (
    <span className="keys-field">
      <input
        className="input"
        aria-label={`${label}: keys`}
        placeholder="Ctrl+Shift+M"
        value={keys}
        onChange={(event) => {
          onChange(event.target.value);
        }}
      />
      <button
        type="button"
        className={recording ? 'btn small recording' : 'btn small'}
        aria-pressed={recording}
        aria-label={`Record the keys for ${label.toLowerCase()}`}
        onClick={() => {
          setRecording((now) => !now);
          setHeard(recording ? '' : 'Press the keys to record, or Escape to stop.');
        }}
        onBlur={() => {
          setRecording(false);
        }}
        onKeyDown={(event) => {
          if (!recording) {
            return;
          }
          event.preventDefault();
          if (event.repeat) {
            return;
          }
          if (event.key === 'Escape') {
            setRecording(false);
            setHeard('Stopped recording.');
            return;
          }
          const pressed = actionKeys(event);
          if (pressed === null) {
            // A modifier alone waits for its key; any other key cannot be pressed by an action.
            if (!['Control', 'Alt', 'Shift', 'Meta', 'AltGraph'].includes(event.key)) {
              setHeard('An action cannot press that key. Try another, or Escape to stop.');
            }
            return;
          }
          onChange(pressed);
          setRecording(false);
          setHeard(`Recorded ${pressed}.`);
        }}
      >
        {recording ? 'Press the keys' : 'Record'}
      </button>
      <span className="visually-hidden" role="status">
        {heard}
      </span>
    </span>
  );
}

const EXECUTABLES_ID = 'running-executables';

interface ActionTargetProps {
  readonly action: ActionDraft;
  readonly label: string;
  readonly onChange: (action: ActionDraft) => void;
}

/** Whose media or volume a media or volume action changes, and the level a volume action sets. */
function ActionTarget({ action, label, onChange }: ActionTargetProps) {
  return (
    <>
      <select
        className="input"
        aria-label={`${label}: for`}
        value={action.target}
        onChange={(event) => {
          onChange({ ...action, target: event.target.value === 'app' ? 'app' : 'default' });
        }}
      >
        <option value="default">{action.kind === 'volume' ? 'The whole system' : 'Whatever is playing'}</option>
        <option value="app">One application</option>
      </select>
      {action.target === 'app' ? (
        <input
          className="input"
          aria-label={`${label}: application`}
          placeholder="Player.exe"
          list={EXECUTABLES_ID}
          value={action.app}
          onChange={(event) => {
            onChange({ ...action, app: event.target.value });
          }}
        />
      ) : (
        <span />
      )}
      {action.kind === 'volume' ? (
        <span className="percent">
          <input
            className="input"
            aria-label={`${label}: volume`}
            inputMode="numeric"
            value={action.percent}
            onChange={(event) => {
              onChange({ ...action, percent: event.target.value });
            }}
          />
          %
        </span>
      ) : (
        <span />
      )}
    </>
  );
}

interface ClickFieldsProps {
  readonly action: ActionDraft;
  readonly label: string;
  readonly apps: readonly AppSummary[];
  readonly onChange: (action: ActionDraft) => void;
  readonly onTarget: () => void;
}

/** The application a click clicks in, what its target is, and whether the focus goes back after. */
function ClickFields({ action, label, apps, onChange, onTarget }: ClickFieldsProps) {
  const choices = clickApps(apps);
  const known = choices.some((choice) => choice.id === action.clickApp);
  const hintApp = `hint-click-app-${action.key}`;
  const hintTarget = `hint-click-target-${action.key}`;
  const hintFocus = `hint-click-focus-${action.key}`;
  return (
    <>
      <span className="click-app keys-field">
        <select
          className="input"
          aria-label={`${label}: application to click in`}
          aria-describedby={hintApp}
          value={action.clickApp}
          onChange={(event) => {
            // A target belongs to its application's window, so another application needs a new one.
            onChange({ ...action, clickApp: event.target.value, at: null });
          }}
        >
          {known ? null : (
            <option value="" disabled>
              {action.clickApp === '' ? 'Pick an application' : `${action.clickApp} (not saved)`}
            </option>
          )}
          {choices.map((choice) => (
            <option key={choice.id} value={choice.id}>
              In {choice.name}
            </option>
          ))}
        </select>
        <Hint topic={`${label}: application to click in`} id={hintApp}>
          The saved application whose window gets the click. It can be the one the value comes from, or another one.
        </Hint>
      </span>
      <span className="click-row">
        <span className="target-sum">
          {action.at?.kind === 'picture' ? (
            <ClickPictureThumb image={action.at.image} label={`${label}: the picture it clicks on`} />
          ) : null}
          <span>{targetSummary(action.button, action.at)}</span>
        </span>
        <button
          type="button"
          className="btn small"
          aria-describedby={hintTarget}
          disabled={action.clickApp === ''}
          onClick={onTarget}
        >
          {action.at === null ? 'Set up the target' : 'Change the target'}
        </button>
        <Hint topic={`${label}: target`} id={hintTarget}>
          Where in the window the click lands: a spot, or a picture such as a button that AppyTappy looks for each time.
          You set it up on a screenshot of the window, so the application must be open.
        </Hint>
      </span>
      <span className="click-options">
        <label>
          <input
            type="checkbox"
            aria-describedby={hintFocus}
            checked={action.returnFocus}
            onChange={(event) => {
              onChange({ ...action, returnFocus: event.target.checked });
            }}
          />
          Go back to the window that was in front
        </label>
        <Hint topic="Go back to the window that was in front" id={hintFocus}>
          After the click, the window you were using gets the focus back, so you can carry on. Turn it off when the
          click opens something you want to use, such as a menu.
        </Hint>
      </span>
    </>
  );
}

interface ActionRowProps {
  readonly action: ActionDraft;
  readonly position: number;
  readonly count: number;
  readonly apps: readonly AppSummary[];
  readonly onTarget: () => void;
  readonly onChange: (action: ActionDraft) => void;
  readonly onMove: (by: -1 | 1) => void;
  readonly onRemove: () => void;
}

function ActionRow({ action, position, count, apps, onTarget, onChange, onMove, onRemove }: ActionRowProps) {
  const choice: ActionChoice = action.kind === 'media' ? action.command : action.kind;
  const label = `Action ${position}`;
  return (
    <li className="action-row">
      <span className="step-number">{position}</span>
      <select
        className="input"
        aria-label={`${label}: what to do`}
        value={choice}
        onChange={(event) => {
          const picked = ACTION_CHOICES.find((option) => option === event.target.value);
          if (picked !== undefined) {
            onChange(
              picked === 'pause' || picked === 'play' || picked === 'next' || picked === 'previous'
                ? { ...action, kind: 'media', command: picked }
                : { ...action, kind: picked },
            );
          }
        }}
      >
        {ACTION_CHOICES.map((option) => (
          <option key={option} value={option}>
            {ACTION_NAMES[option]}
          </option>
        ))}
      </select>
      {action.kind === 'click' ? (
        <ClickFields action={action} label={label} apps={apps} onChange={onChange} onTarget={onTarget} />
      ) : action.kind === 'keys' ? (
        <KeysField
          label={label}
          keys={action.keys}
          onChange={(keys) => {
            onChange({ ...action, keys });
          }}
        />
      ) : action.kind === 'open' ? (
        <input
          className="input path-field"
          aria-label={`${label}: program, file, or link`}
          placeholder="C:\Tools\Timer.exe or https://example.com"
          spellCheck={false}
          value={action.opens}
          onChange={(event) => {
            onChange({ ...action, opens: event.target.value });
          }}
        />
      ) : action.kind === 'sound' ? (
        <input
          className="input path-field"
          aria-label={`${label}: WAV file`}
          placeholder="C:\Sounds\Ding.wav"
          spellCheck={false}
          value={action.sound}
          onChange={(event) => {
            onChange({ ...action, sound: event.target.value });
          }}
        />
      ) : (
        <ActionTarget action={action} label={label} onChange={onChange} />
      )}
      <span className="row-tools">
        <button
          type="button"
          className="btn small"
          aria-label={`Move action ${position} up`}
          disabled={position === 1}
          onClick={() => {
            onMove(-1);
          }}
        >
          ↑
        </button>
        <button
          type="button"
          className="btn small"
          aria-label={`Move action ${position} down`}
          disabled={position === count}
          onClick={() => {
            onMove(1);
          }}
        >
          ↓
        </button>
        <button type="button" className="btn small" aria-label={`Remove action ${position}`} onClick={onRemove}>
          Remove
        </button>
      </span>
    </li>
  );
}

/** The pause between two live readings: often enough to follow a value as it changes. */
const LIVE_PAUSE_MS = 500;

/** How long the live reading waits before it looks for the window again, as the runtime does. */
const RETRY_MS = 1000;

/**
 * What the watched value reads on its window now, and whether `when` holds
 * for it. It watches again once the window closes, and stops its watch when
 * it goes away.
 */
function LiveReading({ choice, when }: { readonly choice: ValueChoice; readonly when: WhenDef | null }) {
  const [live, setLive] = useState<LiveState>(null);
  const watch = useRef<number | null>(null);
  const retryAt = useRef(0);
  const mounted = useRef(true);
  // A function, because the check after an await must see an unmount made meanwhile.
  const isMounted = (): boolean => mounted.current;
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      if (watch.current !== null) {
        background(stopWatching(watch.current));
        watch.current = null;
      }
    };
  }, []);
  useRepeat(
    async () => {
      if (watch.current !== null) {
        try {
          const read = await watchedReading(watch.current, when);
          if (read.kind === 'lost') {
            watch.current = null;
            retryAt.current = Date.now() + RETRY_MS;
          }
          setLive(read);
          return;
        } catch {
          // Another capture replaced the watch: watch again now.
          watch.current = null;
        }
      }
      if (!isMounted() || Date.now() < retryAt.current) {
        return;
      }
      let started: number;
      try {
        started = await watchValue(choice.app, choice.value.id);
        if (!isMounted()) {
          // The editor closed while the capture started.
          await stopWatching(started);
          return;
        }
        watch.current = started;
      } catch (error: unknown) {
        watch.current = null;
        retryAt.current = Date.now() + RETRY_MS;
        setLive({ kind: 'error', message: errorText(error) });
        return;
      }
      try {
        setLive(await watchedReading(started, when));
      } catch {
        // Another capture replaced the watch already: watch again next time.
        if (watch.current === started) {
          watch.current = null;
        }
      }
    },
    LIVE_PAUSE_MS,
    true,
  );
  const holds = live?.kind === 'read' && live.holds === true;
  return (
    <div className={holds ? 'live-reading holds' : 'live-reading'}>
      <p>{liveText(live, choice, when)}</p>
      <p className="visually-hidden" role="status">
        {liveAnnouncement(live)}
      </p>
    </div>
  );
}

interface Props {
  readonly apps: readonly AppSummary[];
  /** The saved automation to change, or null for a new one. */
  readonly existing: AutomationDef | null;
  /** The draft the editor finishes, or null. */
  readonly draft: Resumed | null;
  readonly onCancel: () => void;
  /** Closes the editor once the automation or its draft is saved. */
  readonly onSaved: () => void;
  /** Hears what leaving the editor would lose. */
  readonly onHeld: (held: Held) => void;
  /** Hears how to save the editor's draft. */
  readonly onDraft: ReportDraft;
}

/** Making or changing an automation: the value it watches, the actions it runs, and whether it undoes them. */
export function AutomationEditor({ apps, existing, draft: resumed, onCancel, onSaved, onHeld, onDraft }: Props) {
  const [start] = useState<Draft>(() => startDraft(existing, resumed));
  const [draft, setDraft] = useState<Draft>(start);
  // Each action's key is new to the editor, so the next one goes past the highest a draft kept.
  const [nextKey, setNextKey] = useState(() => Math.max(-1, ...draft.actions.map((action) => action.key)) + 1);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [executables, setExecutables] = useState<readonly string[]>([]);
  // The click action whose target is being set up, by its key, or null while the editor shows.
  const [targeting, setTargeting] = useState<number | null>(null);

  // The running applications' executables, offered as the applications an action can target.
  useEffect(() => {
    let current = true;
    background(
      listWindows()
        .then((list) => {
          if (current) {
            setExecutables([...new Set(list.windows.map((window) => window.executable))].toSorted());
          }
        })
        .catch(() => {
          // Typing the executable works without the suggestions.
        }),
    );
    return () => {
      current = false;
    };
  }, []);

  const choices = useMemo(() => valueChoices(apps), [apps]);
  const choice = draft.source === null ? null : findValue(choices, draft.source);
  const built = buildAutomation(draft, choices, existing?.id ?? '', apps);
  const missing = unfinished(draft, choices, apps);
  const drafting = useDraft(
    resumed,
    () => ({
      kind: 'automation',
      app: null,
      changes: existing?.id ?? null,
      name: draft.name.trim() === '' ? (existing?.name ?? 'New automation') : draft.name.trim(),
      missing,
      pictures: draftPictures(draft),
      state: keep('automation', draft),
    }),
    false,
    onDraft,
    onSaved,
  );
  const holds = held(saving || drafting.saving, start, draft);
  useReportHeld(onHeld, holds);
  const typedWhen = choice === null ? null : buildWhen(draft, choice);
  const liveWhen = typeof typedWhen === 'string' ? null : typedWhen;
  const undoWords = built.ok ? undoText(built.automation.actions) : null;
  const pressesKeys = draft.actions.some((action) => action.kind === 'keys');
  const usesPaths = draft.actions.some((action) => action.kind === 'open' || action.kind === 'sound');
  const opens = draft.actions.some((action) => action.kind === 'open');
  const keysWarning = choice === null ? null : keysBlocked(draft.actions, choice.antiCheat, choice.appName);
  const clickedApps = draft.actions.flatMap((action) =>
    action.kind === 'click' && action.clickApp !== '' ? [action.clickApp] : [],
  );
  const clickWarning = choice === null ? null : clicksBlocked(clickedApps, choice, apps);
  const clickedNames = [...new Set(clickedApps.map((id) => apps.find((app) => app.id === id)?.file?.name ?? id))];

  const update = (change: Partial<Draft>): void => {
    setDraft((current) => ({ ...current, ...change }));
    setSaveError(null);
  };
  const setActions = (change: (actions: readonly ActionDraft[]) => readonly ActionDraft[]): void => {
    setDraft((current) => ({ ...current, actions: change(current.actions) }));
    setSaveError(null);
  };

  const save = async (): Promise<void> => {
    if (!built.ok) {
      return;
    }
    setSaving(true);
    try {
      await saveAutomation(existing?.id ?? null, built.automation);
      await drafting.finished();
      onSaved();
    } catch (error: unknown) {
      setSaveError(errorText(error));
      setSaving(false);
    }
  };

  const appsWithValues = apps.filter((app) => app.file !== null && app.file.values.length > 0);
  const targeted = targeting === null ? undefined : draft.actions.find((action) => action.key === targeting);
  if (targeted !== undefined) {
    return (
      <ClickTarget
        apps={apps}
        automation={draft.name.trim() === '' ? (existing?.name ?? 'New automation') : draft.name.trim()}
        initial={{ app: targeted.clickApp, button: targeted.button, at: targeted.at }}
        onCancel={() => {
          setTargeting(null);
        }}
        onUse={(target) => {
          setActions((actions) =>
            actions.map((old) =>
              old.key === targeted.key
                ? {
                    ...old,
                    clickApp: target.app,
                    button: target.button,
                    at: target.at,
                    // A right click usually opens a menu, which going back to the window in front would close.
                    returnFocus: target.button === old.button ? old.returnFocus : target.button !== 'right',
                  }
                : old,
            ),
          );
          setTargeting(null);
        }}
      />
    );
  }
  const runningApp = choice === null ? null : apps.find((app) => app.id === choice.app);

  return (
    <main className="page">
      <div className="page-head">
        <span className="crumb">
          Automations / <b>{existing === null ? 'New automation' : existing.name}</b>
        </span>
      </div>
      <div className="editor">
        <div className="field">
          <label htmlFor="automation-name">Name</label>
          <input
            id="automation-name"
            className="input"
            placeholder="Pause music near the end"
            value={draft.name}
            onChange={(event) => {
              update({ name: event.target.value });
            }}
          />
        </div>

        <section className="card" aria-labelledby="when-heading">
          <div className="with-hint">
            <h2 id="when-heading">When</h2>
            <Hint topic="When">{WHEN_HINT}</Hint>
          </div>
          <div className="when-row">
            <select
              className="input value-select"
              aria-label="Value"
              value={draft.source === null ? '' : `${draft.source.app}/${draft.source.value}`}
              onChange={(event) => {
                const picked = choices.find((option) => `${option.app}/${option.value.id}` === event.target.value);
                if (picked !== undefined) {
                  setDraft((current) => withSource(current, picked));
                  setSaveError(null);
                }
              }}
            >
              <option value="" disabled>
                Pick a value
              </option>
              {appsWithValues.map((app) => (
                <optgroup key={app.id} label={app.file?.name ?? app.id}>
                  {app.file?.values.map((value) => (
                    <option key={value.id} value={`${app.id}/${value.id}`}>
                      {value.name}
                    </option>
                  ))}
                </optgroup>
              ))}
            </select>
            <select
              className="input"
              aria-label="Comparison"
              value={draft.is}
              disabled={choice === null}
              onChange={(event) => {
                const picked = COMPARISONS[choice?.kind ?? 'number'].find(
                  (comparison) => comparison === event.target.value,
                );
                if (picked !== undefined) {
                  update({ is: picked });
                }
              }}
            >
              {COMPARISONS[choice?.kind ?? 'number'].map((comparison) => (
                <option key={comparison} value={comparison}>
                  {COMPARISON_NAMES[comparison]}
                </option>
              ))}
            </select>
            {choice?.kind === 'state' ? (
              <select
                className="input"
                aria-label="State"
                value={draft.state}
                onChange={(event) => {
                  update({ state: event.target.value });
                }}
              >
                {readsState(choice, draft.state) ? null : (
                  <option value={draft.state} disabled>
                    {draft.state === '' ? 'Pick a state' : `${draft.state} (no longer read)`}
                  </option>
                )}
                {[...choice.states, NO_STATE].map((state) => (
                  <option key={state} value={state}>
                    {state}
                  </option>
                ))}
              </select>
            ) : choice?.kind === 'flag' ? (
              <div className="seg" role="group" aria-label="Yes or no">
                {[true, false].map((flag) => (
                  <button
                    key={String(flag)}
                    type="button"
                    aria-pressed={draft.flag === flag}
                    onClick={() => {
                      update({ flag });
                    }}
                  >
                    {flag ? 'Yes' : 'No'}
                  </button>
                ))}
              </div>
            ) : (
              <input
                className="input"
                aria-label="Number"
                inputMode="decimal"
                placeholder="90"
                disabled={choice === null}
                value={draft.number}
                onChange={(event) => {
                  update({ number: event.target.value });
                }}
              />
            )}
          </div>
          {choice === null ? null : (
            <LiveReading key={`${choice.app}/${choice.value.id}`} choice={choice} when={liveWhen} />
          )}
          <p className="note">
            {runningApp === null || runningApp === undefined
              ? 'One value for now. Conditions that combine several values come later.'
              : `Runs while ${runningApp.file?.name ?? runningApp.id} is open. ${choice === null ? 'The value reads a number.' : readsText(choice)}`}
          </p>
        </section>

        <section className="card" aria-labelledby="do-heading">
          <div className="with-hint">
            <h2 id="do-heading">Do, in order</h2>
            <Hint topic="Actions">{ACTIONS_HINT}</Hint>
          </div>
          {draft.actions.length === 0 ? (
            <p className="note">Add what AppyTappy does when the condition becomes true.</p>
          ) : null}
          <ol className="action-list">
            {draft.actions.map((action, index) => (
              <ActionRow
                key={action.key}
                action={action}
                position={index + 1}
                count={draft.actions.length}
                apps={apps}
                onTarget={() => {
                  setTargeting(action.key);
                }}
                onChange={(changed) => {
                  setActions((actions) => actions.map((old) => (old.key === action.key ? changed : old)));
                }}
                onMove={(by) => {
                  setActions((actions) => {
                    const moved = [...actions];
                    const [row] = moved.splice(index, 1);
                    if (row !== undefined) {
                      moved.splice(index + by, 0, row);
                    }
                    return moved;
                  });
                }}
                onRemove={() => {
                  setActions((actions) => actions.filter((old) => old.key !== action.key));
                }}
              />
            ))}
          </ol>
          {keysWarning === null ? null : <p className="warn">{keysWarning}</p>}
          {clickWarning === null ? null : <p className="warn">{clickWarning}</p>}
          {clickedNames.length === 0 || choice === null ? null : (
            <p className="note">
              The click happens only while{' '}
              {[choice.appName, ...clickedNames.filter((name) => name !== choice.appName)].join(' or ')} is the window
              in front. AppyTappy brings {clickedNames.join(' and ')} to the front for a moment, clicks, puts the cursor
              back, and goes back to the window that was in front if you ask it to. A picture that is not found is not
              clicked.
            </p>
          )}
          {pressesKeys ? (
            <p className="note">
              Keys are pressed only while {choice?.appName ?? 'the application'} is the window in front, so they never
              reach another application.
            </p>
          ) : null}
          {usesPaths ? (
            <p className="note">
              To get a file's full path, right-click it in File Explorer and select Copy as path (hold Shift as you
              right-click on Windows 10).
              {opens ? ' What opens may take you out of a full-screen game.' : ''}
            </p>
          ) : null}
          <button
            type="button"
            className="btn add"
            onClick={() => {
              setActions((actions) => [...actions, newAction(nextKey)]);
              setNextKey((key) => key + 1);
            }}
          >
            Add an action
          </button>
          <datalist id={EXECUTABLES_ID}>
            {executables.map((executable) => (
              <option key={executable} value={executable} />
            ))}
          </datalist>
        </section>

        <section className="card" aria-labelledby="after-heading">
          <h2 id="after-heading">Afterwards</h2>
          <div className="with-hint">
            <label className="toggle">
              <input
                type="checkbox"
                role="switch"
                className="switch"
                aria-checked={draft.undo}
                checked={draft.undo}
                aria-describedby="hint-undo"
                onChange={(event) => {
                  update({ undo: event.target.checked });
                }}
              />
              Undo these when the condition stops being true
            </label>
            <Hint topic="Undo" id="hint-undo">
              {UNDO_HINT}
            </Hint>
          </div>
          <p className="note">
            {draft.undo
              ? (undoWords ?? 'Undo puts back paused or started media and changed volumes.')
              : 'The changes stay until something else changes them.'}{' '}
            It can run again once the condition has stopped being true.
          </p>
          <div className="field narrow">
            <span className="name">
              <label htmlFor="cooldown">Wait before it can run again, in seconds</label>
              <Hint topic="Wait before it can run again" id="hint-cooldown">
                {COOLDOWN_HINT}
              </Hint>
            </span>
            <input
              id="cooldown"
              className="input"
              aria-describedby="hint-cooldown"
              inputMode="decimal"
              value={draft.cooldown}
              onChange={(event) => {
                update({ cooldown: event.target.value });
              }}
            />
          </div>
        </section>

        <div className="editor-foot">
          {/* The regions stay mounted, so a screen reader announces each change of their text. */}
          <div className="missing-list" role="status">
            {saveError !== null ? null : built.ok ? (
              <span className="note">Ready to save.</span>
            ) : (
              <>
                <span className="note">Save it as a draft to finish later. Before it can run:</span>
                <ul>
                  {missing.map((part) => (
                    <li key={part} className="warn">
                      {part}
                    </li>
                  ))}
                </ul>
              </>
            )}
          </div>
          <span className="warn" role="alert">
            {saveError ?? drafting.error ?? ''}
          </span>
          <button type="button" className="btn" disabled={saving || drafting.saving} onClick={onCancel}>
            Cancel
          </button>
          {resumed === null ? null : (
            <DeleteDraft changes={existing !== null} disabled={saving || drafting.saving} onDelete={drafting.discard} />
          )}
          <span className="with-hint">
            <button
              type="button"
              className="btn"
              disabled={holds !== 'unsaved'}
              aria-describedby="hint-draft"
              onClick={() => {
                background(drafting.saveAndClose());
              }}
            >
              Save as draft
            </button>
            <Hint topic="Save as draft" id="hint-draft">
              {DRAFT_HINT}
            </Hint>
          </span>
          <button
            type="button"
            className="btn primary"
            disabled={!built.ok || saving || drafting.saving}
            onClick={() => {
              background(save());
            }}
          >
            Save automation
          </button>
        </div>
      </div>
    </main>
  );
}
