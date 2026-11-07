import { useEffect, useMemo, useState } from 'react';

import {
  type AppSummary,
  type AutomationDef,
  type MediaCommand,
  errorText,
  listWindows,
  saveAutomation,
} from './api.ts';
import {
  type ActionDraft,
  type Draft,
  COMPARISONS,
  COMPARISON_NAMES,
  EMPTY_DRAFT,
  MEDIA_NAMES,
  buildAutomation,
  findValue,
  newAction,
  toDraft,
  undoText,
  valueChoices,
  withSource,
} from './automation.ts';
import { background } from './repeat.ts';

/** The action menu's choices: each media command, then setting a volume. */
type ActionChoice = MediaCommand | 'volume';

const ACTION_CHOICES: readonly ActionChoice[] = ['pause', 'play', 'next', 'previous', 'volume'];

const ACTION_NAMES: Readonly<Record<ActionChoice, string>> = { ...MEDIA_NAMES, volume: 'Set volume' };

const EXECUTABLES_ID = 'running-executables';

interface ActionRowProps {
  readonly action: ActionDraft;
  readonly position: number;
  readonly count: number;
  readonly onChange: (action: ActionDraft) => void;
  readonly onMove: (by: -1 | 1) => void;
  readonly onRemove: () => void;
}

function ActionRow({ action, position, count, onChange, onMove, onRemove }: ActionRowProps) {
  const choice: ActionChoice = action.kind === 'volume' ? 'volume' : action.command;
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
              picked === 'volume' ? { ...action, kind: 'volume' } : { ...action, kind: 'media', command: picked },
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

interface Props {
  readonly apps: readonly AppSummary[];
  /** The saved automation to change, or null for a new one. */
  readonly existing: AutomationDef | null;
  readonly onCancel: () => void;
  readonly onSaved: () => void;
}

/** Making or changing an automation: the value it watches, the actions it runs, and whether it undoes them. */
export function AutomationEditor({ apps, existing, onCancel, onSaved }: Props) {
  const [draft, setDraft] = useState<Draft>(() => (existing === null ? EMPTY_DRAFT : toDraft(existing)));
  const [nextKey, setNextKey] = useState(() => draft.actions.length);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [executables, setExecutables] = useState<readonly string[]>([]);

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
  const built = buildAutomation(draft, choices, existing?.id ?? '');
  const undoWords = built.ok ? undoText(built.automation.actions) : null;

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
      onSaved();
    } catch (error: unknown) {
      setSaveError(errorText(error));
      setSaving(false);
    }
  };

  const appsWithValues = apps.filter((app) => app.file !== null && app.file.values.length > 0);
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
          <h2 id="when-heading">When</h2>
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
            {choice?.kind === 'flag' ? (
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
          <p className="note">
            {runningApp === null || runningApp === undefined
              ? 'One value for now. Conditions that combine several values come later.'
              : `Runs while ${runningApp.file?.name ?? runningApp.id} is open. ${choice?.kind === 'flag' ? `${choice.value.name} reads yes or no.` : `${choice?.value.name ?? 'The value'} reads a number.`}`}
          </p>
        </section>

        <section className="card" aria-labelledby="do-heading">
          <h2 id="do-heading">Do, in order</h2>
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
          <label className="toggle">
            <input
              type="checkbox"
              role="switch"
              className="switch"
              aria-checked={draft.undo}
              checked={draft.undo}
              onChange={(event) => {
                update({ undo: event.target.checked });
              }}
            />
            Undo these when the condition stops being true
          </label>
          <p className="note">
            {draft.undo
              ? (undoWords ?? 'Undo puts back paused or started media and changed volumes.')
              : 'The changes stay until something else changes them.'}{' '}
            It can run again once the condition has stopped being true.
          </p>
          <div className="field narrow">
            <label htmlFor="cooldown">Wait before it can run again, in seconds</label>
            <input
              id="cooldown"
              className="input"
              inputMode="decimal"
              value={draft.cooldown}
              onChange={(event) => {
                update({ cooldown: event.target.value });
              }}
            />
          </div>
        </section>

        <div className="editor-foot">
          {saveError === null ? (
            <span className="note" role="status">
              {built.ok ? 'Ready to save.' : built.next}
            </span>
          ) : (
            <span className="warn" role="alert">
              {saveError}
            </span>
          )}
          <button type="button" className="btn" onClick={onCancel}>
            Cancel
          </button>
          <button
            type="button"
            className="btn primary"
            disabled={!built.ok || saving}
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
