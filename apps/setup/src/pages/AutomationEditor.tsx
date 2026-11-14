import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { type AppSummary, type MediaCommand, NO_STATE, type ReadingDetails, type WhenDef } from '../lib/bindings.ts';
import {
  errorText,
  explainWatched,
  listWindows,
  saveAutomation,
  stopWatching,
  watchValue,
  watchedReading,
} from '../lib/api.ts';
import {
  type ActionDraft,
  type Draft,
  type LiveState,
  COMPARISONS,
  COMPARISON_NAMES,
  MEDIA_NAMES,
  type ValueChoice,
  buildAutomation,
  buildWhen,
  clicksBlocked,
  keysBlocked,
  liveAnnouncement,
  liveText,
  newAction,
  readsState,
  readsText,
  undoText,
  unfinished,
  unusedKey,
  valueChoices,
  withSource,
} from '../lib/automation.ts';
import { clickApps, returnsFocus, targetSummary } from '../lib/click.ts';
import { type AutomationFlow, automationDraft, startOf } from '../lib/flow.ts';
import { ACTIONS_HINT, COOLDOWN_HINT, DRAFT_HINT, UNDO_HINT, WHEN_HINT } from '../lib/hints.ts';
import { editorLanding, useLanding } from '../lib/landing.ts';
import { background, useRepeat } from '../lib/repeat.ts';
import { ask, useMounted } from '../lib/request.ts';
import { type LiveWatchEvent, NOT_WATCHING, liveWatchStep } from '../lib/liveWatch.ts';
import { ClickPictureThumb } from '../components/ClickPictureThumb.tsx';
import { ClickTarget } from '../components/ClickTarget.tsx';
import { DeleteDraft } from '../components/Drafts.tsx';
import { Hint } from '../components/Hint.tsx';
import { ReadingDetailsBox } from '../components/ReadingDetails.tsx';
import { type Held, held, useReportHeld } from '../lib/unsaved.ts';
import { type ReportDraft, useDraft } from '../lib/drafts.ts';
import { actionKeys } from '../lib/shortcut.ts';
import { arrived, readiness, returned, watchApps, watchGuide, watchStep, watching } from '../lib/watch.ts';

/** The action menu's choices: each media command, then setting a volume, pressing keys, opening, and playing a sound. */
type ActionChoice = MediaCommand | Exclude<ActionDraft['kind'], 'media'>;

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

/** The ID of the When card's guide, which describes the controls the next step is taken with. */
const GUIDE_ID = 'watch-guide';

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
          <span>{targetSummary(action.button, action.at, action.delivery)}</span>
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
      {action.delivery === 'cursor' ? (
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
      ) : null}
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

/**
 * What the watched value reads on its window now, and whether `when` holds for it. Its watch is `liveWatchStep`'s:
 * it watches again once the window closes or another capture takes its place, and stops when it goes away.
 */
function LiveReading({ choice, when }: { readonly choice: ValueChoice; readonly when: WhenDef | null }) {
  const [live, setLive] = useState<LiveState>(null);
  const state = useRef(NOT_WATCHING);
  const send = (event: LiveWatchEvent): void => {
    const next = liveWatchStep(state.current, event);
    state.current = next.state;
    if (next.shows !== undefined) {
      setLive(next.shows);
    }
  };
  const isMounted = useMounted();
  useEffect(
    () => () => {
      const { watch } = state.current;
      if (watch !== null) {
        background(stopWatching(watch));
        state.current = NOT_WATCHING;
      }
    },
    [],
  );
  // Reads watch `watch`, and hears whether it answered a reading.
  const read = async (watch: number): Promise<void> => {
    try {
      send({ kind: 'read', live: await watchedReading(watch, when), now: Date.now() });
    } catch (error: unknown) {
      send({ kind: 'refused', message: errorText(error), now: Date.now() });
    }
  };
  useRepeat(
    async () => {
      if (state.current.watch !== null) {
        await read(state.current.watch);
      }
      // A watch that was refused starts again in the same turn; one that read is done.
      if (state.current.watch !== null || !isMounted() || Date.now() < state.current.retryAt) {
        return;
      }
      let started: number;
      try {
        started = await watchValue(choice.app, choice.value.id);
      } catch (error: unknown) {
        send({ kind: 'unstarted', message: errorText(error), now: Date.now() });
        return;
      }
      if (!isMounted()) {
        // The editor closed while the capture started.
        await stopWatching(started);
        return;
      }
      send({ kind: 'started', watch: started });
      await read(started);
    },
    LIVE_PAUSE_MS,
    true,
  );
  const holds = live?.kind === 'read' && live.holds === true;
  // Reads the watch the live reading holds now, which can change between reads.
  const askDetails = useCallback(
    async (): Promise<ReadingDetails | null> =>
      state.current.watch === null ? null : explainWatched(state.current.watch),
    [],
  );
  return (
    <>
      <div className={holds ? 'live-reading holds' : 'live-reading'}>
        <p>{liveText(live, choice, when)}</p>
        <p className="visually-hidden" role="status">
          {liveAnnouncement(live)}
        </p>
      </div>
      <ReadingDetailsBox ask={live?.kind === 'read' ? askDetails : null} />
    </>
  );
}

interface Props {
  readonly apps: readonly AppSummary[];
  /** The number of the read of the saved lists `apps` is from. */
  readonly listed: number;
  /** Why the newest read of the saved lists failed, or null when it answered. */
  readonly readError: string | null;
  /**
   * The automation to make or change. After a flow the editor opened ends, it also has the draft to show again, the
   * application its condition starts with, and what that flow saved.
   */
  readonly flow: AutomationFlow;
  /** Whether the editor shows because a flow it opened ended, so the focus has to land somewhere. */
  readonly continued: boolean;
  readonly onCancel: () => void;
  readonly onSaved: () => void;
  /** Closes the editor once its draft is saved or deleted. */
  readonly onDrafted: () => void;
  /** Leaves to add an application, and comes back with `draft` and that application chosen. */
  readonly onAddApplication: (draft: Draft, app: string | null) => void;
  /** Leaves to make a value for the application `app`, and comes back with `draft` and that value picked. */
  readonly onNewValue: (draft: Draft, app: string) => void;
  /** Hears what leaving the editor would lose. */
  readonly onHeld: (held: Held) => void;
  /** Hears how to save the editor's draft. */
  readonly onDraft: ReportDraft;
}

/**
 * Making or changing an automation: the application and the value it watches, the actions it runs, and whether it
 * undoes them. It leads to adding the application and making the value when the one to watch is not saved yet.
 */
export function AutomationEditor({
  apps,
  listed,
  readError,
  flow,
  continued,
  onCancel,
  onSaved,
  onDrafted,
  onAddApplication,
  onNewValue,
  onHeld,
  onDraft,
}: Props) {
  const { existing, resumed } = flow;
  // What the editor starts with, which leaving it loses nothing of: what its draft kept, or the saved automation.
  const [start] = useState<Draft>(() => startOf(flow).draft);
  const [draft, setDraft] = useState<Draft>(() => returned(flow.draft ?? start, flow.app, flow.handed));
  // The application chosen for the condition before a value of it is picked.
  const [chosenApp, setChosenApp] = useState(flow.app);
  // What a flow just saved for the condition, until the draft picks its value or the user chooses something else.
  const [handed, setHanded] = useState(flow.handed);
  // The value the draft picked because a flow just saved it, which the card names.
  const [justPicked, setJustPicked] = useState<string | null>(null);
  const card = useRef<HTMLElement>(null);
  const [nextKey, setNextKey] = useState(() => unusedKey(draft.actions));
  const [saveError, setSaveError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  // A draft keeps what the flow holds of its automation, as it would wait under a flow the editor opens.
  const drafting = useDraft(
    resumed,
    () => automationDraft(flow, { draft, app: chosenApp }, apps),
    false,
    onDraft,
    onDrafted,
  );
  const busy = saving || drafting.saving;
  const holds = held(busy, start, draft);
  useReportHeld(onHeld, holds);
  const [executables, setExecutables] = useState<readonly string[]>([]);
  // The click action whose target is being set up, by its key, or null while the editor shows.
  const [targeting, setTargeting] = useState<number | null>(null);

  // The running applications' executables, offered as the applications an action can target.
  useEffect(
    () =>
      ask(
        listWindows,
        (list) => {
          setExecutables([...new Set(list.windows.map((window) => window.executable))].toSorted());
        },
        () => {
          // Typing the executable works without the suggestions.
        },
      ),
    [],
  );

  const choices = useMemo(() => valueChoices(apps), [apps]);
  const saved = useMemo(() => watchApps(apps), [apps]);
  // The value a flow just saved is picked as soon as the saved values list it. The state is set while rendering,
  // so no frame shows the card between the two.
  const landed = arrived({ choices, chosen: chosenApp, handed });
  if (landed !== null) {
    setDraft(withSource(draft, landed));
    setHanded(null);
    setJustPicked(landed.value.id);
  }
  const step = watchStep({ apps, choices, listed, readError, draft, chosen: chosenApp, handed });
  const guide = watchGuide(step, justPicked);
  const choice = step.kind === 'ready' ? step.choice : null;
  // The application the condition watches, or null before one is chosen.
  const watched = 'app' in step ? step.app.id : 'id' in step ? step.id : null;
  useLanding(continued, editorLanding(step), card);
  const built = buildAutomation(draft, choices, existing?.id ?? '', apps);
  const ready = readiness(built, step);
  const missing = unfinished(draft, choices, apps);
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
  // The applications clicked in with the cursor, and those a click is sent to, since each note says what its way does.
  const clickedBy = (delivery: ActionDraft['delivery']): readonly string[] => [
    ...new Set(
      draft.actions.flatMap((action) =>
        action.kind === 'click' && action.clickApp !== '' && action.delivery === delivery
          ? [apps.find((app) => app.id === action.clickApp)?.file?.name ?? action.clickApp]
          : [],
      ),
    ),
  ];
  const clickedNames = clickedBy('cursor');
  const sentNames = clickedBy('message');

  const update = (change: Partial<Draft>): void => {
    setDraft((current) => ({ ...current, ...change }));
    setSaveError(null);
  };
  const setActions = (change: (actions: readonly ActionDraft[]) => readonly ActionDraft[]): void => {
    setDraft((current) => ({ ...current, actions: change(current.actions) }));
    setSaveError(null);
  };

  const save = async (): Promise<void> => {
    if (!built.ok || !ready.ready) {
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

  const targeted = targeting === null ? undefined : draft.actions.find((action) => action.key === targeting);
  if (targeted !== undefined) {
    return (
      <ClickTarget
        apps={apps}
        automation={draft.name.trim() === '' ? (existing?.name ?? 'New automation') : draft.name.trim()}
        watched={draft.source?.app ?? null}
        initial={{
          app: targeted.clickApp,
          button: targeted.button,
          delivery: targeted.delivery,
          returnFocus: targeted.returnFocus,
          at: targeted.at,
        }}
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
                    delivery: target.delivery,
                    at: target.at,
                    returnFocus: returnsFocus(target.button, old),
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

        <section className="card" aria-labelledby="when-heading" ref={card}>
          <div className="with-hint">
            <h2 id="when-heading">When</h2>
            <Hint topic="When">{WHEN_HINT}</Hint>
          </div>
          {/* Stays mounted, so a screen reader announces each step, and what a flow saved once it is picked. */}
          <p id={GUIDE_ID} className={guide?.warn === true ? 'warn guide' : 'guide'} role="status">
            {guide?.text ?? ''}
          </p>
          <div className="watch-row">
            {step.kind === 'no-applications' ? null : (
              <select
                className="input"
                aria-label="Application"
                aria-describedby={GUIDE_ID}
                data-landing="application"
                value={watched ?? ''}
                onChange={(event) => {
                  const picked = event.target.value;
                  setChosenApp(picked);
                  setHanded(null);
                  setDraft((current) => watching(current, picked));
                  setSaveError(null);
                }}
              >
                {step.kind === 'choosing' ? (
                  <option value="" disabled>
                    Pick an application
                  </option>
                ) : null}
                {watched === null || saved.some((app) => app.id === watched) ? null : (
                  <option value={watched} disabled>
                    {step.kind === 'arriving' ? 'Reading…' : `${watched} (not saved)`}
                  </option>
                )}
                {saved.map((app) => (
                  <option key={app.id} value={app.id}>
                    {app.name}
                  </option>
                ))}
              </select>
            )}
            {'app' in step ? (
              <button
                type="button"
                className={guide?.opens === 'value' ? 'btn primary' : 'btn'}
                aria-describedby={GUIDE_ID}
                data-landing="new-value"
                onClick={() => {
                  onNewValue(draft, step.app.id);
                }}
              >
                New value
              </button>
            ) : null}
            <button
              type="button"
              className={guide?.opens === 'application' ? 'btn primary' : 'btn'}
              aria-describedby={GUIDE_ID}
              data-landing="add-application"
              onClick={() => {
                onAddApplication(draft, watched);
              }}
            >
              Add application
            </button>
          </div>
          {step.kind === 'pick' || step.kind === 'ready' ? (
            <div className="when-row">
              <select
                className="input value-select"
                aria-label="Value"
                aria-describedby={GUIDE_ID}
                data-landing="value"
                value={choice === null ? '' : choice.value.id}
                onChange={(event) => {
                  const picked = step.values.find((option) => option.value.id === event.target.value);
                  if (picked !== undefined) {
                    setHanded(null);
                    setDraft((current) => withSource(current, picked));
                    setSaveError(null);
                  }
                }}
              >
                <option value="" disabled>
                  Pick a value
                </option>
                {step.values.map((option) => (
                  <option key={option.value.id} value={option.value.id}>
                    {option.value.name}
                  </option>
                ))}
              </select>
              <select
                className="input"
                aria-label="Comparison"
                aria-describedby={GUIDE_ID}
                data-landing="comparison"
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
          ) : null}
          {choice === null ? null : (
            <LiveReading key={`${choice.app}/${choice.value.id}`} choice={choice} when={liveWhen} />
          )}
          {step.kind === 'no-applications' || step.kind === 'value' || step.kind === 'arriving' ? null : (
            <p className="note">
              {runningApp === null || runningApp === undefined
                ? 'One value for now. Conditions that combine several values come later.'
                : `Runs while ${runningApp.file?.name ?? runningApp.id} is open. ${choice === null ? 'The value reads a number.' : readsText(choice)}`}
            </p>
          )}
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
          {sentNames.length === 0 ? null : (
            <p className="note">
              For a click sent to the window of {sentNames.join(' or ')}, AppyTappy does not move the cursor or bring a
              window to the front. Some applications ignore a click sent this way, and nothing tells AppyTappy whether
              it worked, so test it with Click it now where you set up the target. A picture that is not found is not
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
          {/* Both regions stay mounted, so a screen reader announces each change of their text. */}
          <div className="missing-list" role="status">
            {saveError !== null ? null : built.ok || step.kind === 'arriving' ? (
              <span className="note">{ready.text}</span>
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
          <button type="button" className="btn" disabled={busy} onClick={onCancel}>
            Cancel
          </button>
          {resumed === null ? null : (
            <DeleteDraft changes={existing !== null} disabled={busy} onDelete={drafting.discard} />
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
            disabled={!ready.ready || busy}
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
