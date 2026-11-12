import { useEffect, useRef, useState } from 'react';

import type { AppSummary, AutomationSummary, DraftSummary } from '../lib/bindings.ts';
import { deleteAutomation, errorText, setAutomationEnabled } from '../lib/api.ts';
import {
  COMPARISON_NAMES,
  type ValueChoice,
  actionText,
  findValue,
  targetText,
  valueChoices,
} from '../lib/automation.ts';
import { Hint } from '../components/Hint.tsx';
import { NEEDS_RUNNING_HINT } from '../lib/hints.ts';
import { background } from '../lib/repeat.ts';
import { type Resumed, draftOfSaved, draftProblem, newDrafts } from '../lib/drafts.ts';
import { keptAutomation } from '../lib/flow.ts';
import { DraftActions, DraftNote, DraftPill } from '../components/Drafts.tsx';

interface RowProps {
  readonly summary: AutomationSummary;
  readonly apps: readonly AppSummary[];
  readonly choices: readonly ValueChoice[];
  /** Whether a change to its switch is still on its way. */
  readonly toggling: boolean;
  readonly confirming: boolean;
  readonly onConfirm: (confirming: boolean) => void;
  readonly onToggle: () => void;
  readonly onEdit: () => void;
  readonly onDelete: () => void;
  /** Opens the draft of changes to it, in Edit's place, or null when it has none. */
  readonly onFinishDraft: (() => void) | null;
}

function Row({
  summary,
  apps,
  choices,
  toggling,
  confirming,
  onConfirm,
  onToggle,
  onEdit,
  onDelete,
  onFinishDraft,
}: RowProps) {
  const { automation, problem } = summary;
  const { when } = automation;
  const choice = findValue(choices, when);
  const keepRef = useRef<HTMLButtonElement>(null);
  const deleteRef = useRef<HTMLButtonElement>(null);
  // Pressing Delete or Keep swaps the two buttons, so focus moves to the one that took the pressed one's place.
  // Only this row's own press moves it: another row's confirm or a refresh leaves focus where it is.
  const focusNext = useRef<'keep' | 'delete' | null>(null);
  useEffect(() => {
    if (focusNext.current === 'keep' && confirming) {
      keepRef.current?.focus();
    } else if (focusNext.current === 'delete' && !confirming) {
      deleteRef.current?.focus();
    }
    focusNext.current = null;
  }, [confirming]);
  const app = apps.find((candidate) => candidate.id === when.app);
  const running = (app?.windows.length ?? 0) > 0;
  return (
    <tr>
      <td>
        <button
          type="button"
          role="switch"
          className="switch"
          aria-checked={automation.enabled}
          aria-label={automation.name}
          aria-busy={toggling}
          aria-disabled={toggling}
          onClick={onToggle}
        />
      </td>
      <td>
        <div className="strong">
          {automation.name}
          {onFinishDraft === null ? null : (
            <>
              {' '}
              <DraftPill changes />
            </>
          )}
        </div>
        {problem === null ? (
          <small className="note">
            {automation.undo ? 'Undoes its actions when the condition stops being true' : 'Leaves its changes in place'}
          </small>
        ) : (
          <small className="warn">{problem}</small>
        )}
      </td>
      <td>
        <span className="chip value">{choice?.value.name ?? when.value}</span> {COMPARISON_NAMES[when.is]}{' '}
        <strong>{targetText(when.target)}</strong>
      </td>
      <td>
        <ol className="steps">
          {automation.actions.map((action, index) => (
            // The actions never reorder while listed, so their place names them.
            <li key={index} className="chip">
              {actionText(action, (id) => apps.find((candidate) => candidate.id === id)?.file?.name ?? id)}
            </li>
          ))}
        </ol>
      </td>
      <td>
        <span className="running-app">
          <span className={running ? 'dot running' : 'dot'} />
          {app?.file?.name ?? when.app}
        </span>
      </td>
      <td className="end">
        {confirming ? (
          <span className="confirm">
            <button
              ref={keepRef}
              type="button"
              className="btn small"
              onClick={() => {
                focusNext.current = 'delete';
                onConfirm(false);
              }}
            >
              Keep
            </button>
            <button type="button" className="btn small danger" onClick={onDelete}>
              Delete automation
            </button>
          </span>
        ) : (
          <span className="confirm">
            {onFinishDraft === null ? (
              <button type="button" className="btn small" onClick={onEdit}>
                Edit<span className="visually-hidden"> {automation.name}</span>
              </button>
            ) : (
              // Its draft holds the changes, so editing goes on from there rather than from the saved one.
              <button type="button" className="btn small" onClick={onFinishDraft}>
                Finish draft<span className="visually-hidden"> of {automation.name}</span>
              </button>
            )}
            <button
              ref={deleteRef}
              type="button"
              className="btn small"
              onClick={() => {
                focusNext.current = 'keep';
                onConfirm(true);
              }}
            >
              Delete
            </button>
          </span>
        )}
      </td>
    </tr>
  );
}

interface DraftRowProps {
  readonly resumed: Resumed;
  readonly choices: readonly ValueChoice[];
  readonly problem: string | null;
  readonly onFinish: () => void;
  readonly onDelete: () => Promise<void>;
}

/** A new automation saved as a draft: what it has so far, and that it doesn't run. */
function DraftRow({ resumed, choices, problem, onFinish, onDelete }: DraftRowProps) {
  const { def } = resumed;
  const kept = keptAutomation(def)?.draft ?? null;
  const source = kept?.source ?? null;
  const value = source === null ? null : findValue(choices, source);
  const actions = kept?.actions.length ?? 0;
  return (
    <tr className="draft-row">
      <td>
        <DraftPill />
      </td>
      <td>
        <div className="strong">{def.name}</div>
        <DraftNote missing={def.missing} problem={problem} />
      </td>
      <td>
        {value === null ? (
          <span className="note">No value yet</span>
        ) : (
          <span className="chip value">{value.value.name}</span>
        )}
      </td>
      <td>
        <span className="note">
          {actions === 0 ? 'No actions yet' : actions === 1 ? '1 action' : `${String(actions)} actions`}
        </span>
      </td>
      <td>
        <span className="note">Doesn&apos;t run</span>
      </td>
      <td className="end">
        <DraftActions name={def.name} problem={problem} onFinish={onFinish} onDelete={onDelete} />
      </td>
    </tr>
  );
}

interface Props {
  readonly apps: readonly AppSummary[] | null;
  readonly automations: readonly AutomationSummary[] | null;
  readonly loadError: string | null;
  readonly onChanged: () => Promise<void>;
  readonly onNew: () => void;
  readonly onEdit: (summary: AutomationSummary) => void;
  /** Every saved draft, or null before they are read. */
  readonly drafts: readonly DraftSummary[] | null;
  readonly onFinish: (resumed: Resumed) => void;
  /** Deletes a draft; the page shows why when it fails. */
  readonly onDeleteDraft: (id: string) => Promise<void>;
}

/** The saved automations: what each watches, what it does, and whether it is on. Drafts of new ones follow them. */
export function AutomationsPage({
  apps,
  automations,
  loadError,
  onChanged,
  onNew,
  onEdit,
  drafts,
  onFinish,
  onDeleteDraft,
}: Props) {
  const [confirming, setConfirming] = useState<string | null>(null);
  const [toggling, setToggling] = useState<ReadonlySet<string>>(new Set());
  // The same set, read when a press arrives: a second press can come before the first one's render.
  const inFlight = useRef(new Set<string>());
  const [actionError, setActionError] = useState<string | null>(null);
  const headingRef = useRef<HTMLHeadingElement>(null);

  const run = async (change: () => Promise<void>): Promise<boolean> => {
    try {
      await change();
      setActionError(null);
      setConfirming(null);
      await onChanged();
      return true;
    } catch (error: unknown) {
      setActionError(errorText(error));
      return false;
    }
  };

  // A switch sends the state it shows, so it waits for the list to show the change before it takes another press.
  // It stays focusable meanwhile, because disabling a focused button drops keyboard focus.
  const toggle = async (id: string, enabled: boolean): Promise<void> => {
    if (inFlight.current.has(id)) {
      return;
    }
    inFlight.current.add(id);
    setToggling((current) => new Set(current).add(id));
    await run(async () => {
      await setAutomationEnabled(id, enabled);
    });
    inFlight.current.delete(id);
    setToggling((current) => {
      const next = new Set(current);
      next.delete(id);
      return next;
    });
  };

  const remove = async (id: string): Promise<void> => {
    if (
      await run(async () => {
        await deleteAutomation(id);
      })
    ) {
      // The deleted row took the focused button with it.
      headingRef.current?.focus();
    }
  };

  const removeDraft = async (id: string): Promise<void> => {
    try {
      await onDeleteDraft(id);
      setActionError(null);
      headingRef.current?.focus();
    } catch (error: unknown) {
      setActionError(errorText(error));
    }
  };

  const choices = apps === null ? [] : valueChoices(apps);
  const fresh = newDrafts(
    drafts,
    'automation',
    (id) => automations?.some((each) => each.automation.id === id) ?? false,
  );
  // A draft that cannot be read has no kind, so it is listed here, on the first page.
  const unreadable = (drafts ?? []).filter((summary) => summary.draft === null);
  // A new automation leads to adding its application and making its value when none is saved yet.
  const canCreate = apps !== null && loadError === null;
  const head = (
    <div className="page-head">
      <h1 ref={headingRef} tabIndex={-1}>
        Automations
      </h1>
      <span className="lead">When a value becomes true, AppyTappy runs the actions in order.</span>
      <div className="end">
        <button type="button" className="btn primary" disabled={!canCreate} onClick={onNew}>
          New automation
        </button>
      </div>
    </div>
  );

  const body = (() => {
    if (apps === null || automations === null) {
      return null;
    }
    if (automations.length === 0 && fresh.length === 0) {
      return (
        <div className="empty">
          <h2>No automations yet</h2>
          <p>
            An automation watches one of your values and runs actions in order when it becomes true, such as pausing
            music when a number on screen reaches 90 or lowering an application&apos;s volume while an icon shows.
          </p>
          {choices.length > 0 ? null : (
            <p>
              No value is saved yet. New automation takes you through adding the application and making the value to
              watch, then back to say what happens when it changes.
            </p>
          )}
          {canCreate ? (
            <button type="button" className="btn primary" onClick={onNew}>
              New automation
            </button>
          ) : null}
        </div>
      );
    }
    return (
      <table className="table">
        <thead>
          <tr>
            <th scope="col">On</th>
            <th scope="col">Automation</th>
            <th scope="col">When</th>
            <th scope="col">Do, in order</th>
            <th scope="col" aria-label="Needs running">
              <span className="with-hint">
                Needs running
                <Hint topic="Needs running">{NEEDS_RUNNING_HINT}</Hint>
              </span>
            </th>
            <th scope="col">
              <span className="visually-hidden">Edit or delete</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {automations.map((summary) => {
            const { id, enabled } = summary.automation;
            return (
              <Row
                key={id}
                summary={summary}
                apps={apps}
                choices={choices}
                toggling={toggling.has(id)}
                confirming={confirming === id}
                onConfirm={(confirm) => {
                  setConfirming(confirm ? id : null);
                }}
                onToggle={() => {
                  background(toggle(id, !enabled));
                }}
                onEdit={() => {
                  onEdit(summary);
                }}
                onDelete={() => {
                  background(remove(id));
                }}
                onFinishDraft={(() => {
                  const changes = draftOfSaved(drafts, 'automation', id);
                  return changes === null
                    ? null
                    : () => {
                        onFinish(changes);
                      };
                })()}
              />
            );
          })}
          {fresh.map((resumed) => (
            <DraftRow
              key={resumed.id}
              resumed={resumed}
              choices={choices}
              problem={draftProblem(resumed.def, apps)}
              onFinish={() => {
                onFinish(resumed);
              }}
              onDelete={async () => {
                await removeDraft(resumed.id);
              }}
            />
          ))}
        </tbody>
      </table>
    );
  })();

  return (
    <main className="page">
      {head}
      {loadError === null ? null : (
        <p className="warn" role="alert">
          {loadError}
        </p>
      )}
      {actionError === null ? null : (
        <p className="warn" role="alert">
          {actionError}
        </p>
      )}
      {body}
      {unreadable.length === 0 ? null : (
        <ul className="draft-list" aria-label="Drafts that cannot be read">
          {unreadable.map((summary, index) => (
            <li key={summary.id} className="draft-row">
              <DraftPill />
              <span>
                <span className="strong">A draft that cannot be read</span>
                <DraftNote
                  missing={[]}
                  problem={`${summary.problem ?? 'Its file is not valid.'} It can only be deleted. Until then, AppyTappy keeps every picture that clicks look for, since this draft may use some.`}
                />
              </span>
              <DraftActions
                name={unreadable.length === 1 ? 'that cannot be read' : `that cannot be read, number ${index + 1}`}
                problem={summary.problem ?? 'Its file is not valid.'}
                onFinish={() => {
                  // A draft that cannot be read cannot be finished.
                }}
                onDelete={async () => {
                  await removeDraft(summary.id);
                }}
              />
            </li>
          ))}
        </ul>
      )}
      {automations !== null && automations.length > 0 ? (
        <p className="note">
          Each automation watches one value for now; conditions that combine several values come later.
        </p>
      ) : null}
    </main>
  );
}
