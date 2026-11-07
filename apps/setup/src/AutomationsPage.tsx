import { useState } from 'react';

import { type AppSummary, type AutomationSummary, deleteAutomation, errorText, setAutomationEnabled } from './api.ts';
import { COMPARISON_NAMES, actionText, findValue, targetText, valueChoices } from './automation.ts';
import { background } from './repeat.ts';

interface RowProps {
  readonly summary: AutomationSummary;
  readonly apps: readonly AppSummary[];
  readonly confirming: boolean;
  readonly onConfirm: (confirming: boolean) => void;
  readonly onToggle: () => void;
  readonly onEdit: () => void;
  readonly onDelete: () => void;
}

function Row({ summary, apps, confirming, onConfirm, onToggle, onEdit, onDelete }: RowProps) {
  const { automation, problem } = summary;
  const { when } = automation;
  const choice = findValue(valueChoices(apps), when);
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
          aria-label={`${automation.name} on`}
          onClick={onToggle}
        />
      </td>
      <td>
        <div className="strong">{automation.name}</div>
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
              {actionText(action)}
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
              type="button"
              className="btn small"
              onClick={() => {
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
            <button type="button" className="btn small" onClick={onEdit}>
              Edit
            </button>
            <button
              type="button"
              className="btn small"
              onClick={() => {
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

interface Props {
  readonly apps: readonly AppSummary[] | null;
  readonly automations: readonly AutomationSummary[] | null;
  readonly loadError: string | null;
  readonly onChanged: () => Promise<void>;
  readonly onNew: () => void;
  readonly onEdit: (summary: AutomationSummary) => void;
}

/** The saved automations: what each watches, what it does, and whether it is on. */
export function AutomationsPage({ apps, automations, loadError, onChanged, onNew, onEdit }: Props) {
  const [confirming, setConfirming] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const run = async (change: () => Promise<void>): Promise<void> => {
    try {
      await change();
      setActionError(null);
      setConfirming(null);
      await onChanged();
    } catch (error: unknown) {
      setActionError(errorText(error));
    }
  };

  const hasValues = apps !== null && valueChoices(apps).length > 0;
  const head = (
    <div className="page-head">
      <h1>Automations</h1>
      <span className="lead">When a value becomes true, AppyTappy runs the actions in order.</span>
      <div className="end">
        <button type="button" className="btn primary" disabled={!hasValues} onClick={onNew}>
          New automation
        </button>
      </div>
    </div>
  );

  const body = (() => {
    if (apps === null || automations === null) {
      return null;
    }
    if (automations.length === 0) {
      return (
        <div className="empty">
          <h2>No automations yet</h2>
          {hasValues ? (
            <p>
              An automation watches one of your values and runs actions in order when it becomes true, such as pausing
              music when a number on screen reaches 90 or lowering an application&apos;s volume while an icon shows.
            </p>
          ) : (
            <p>
              An automation watches a value, so save a value first on the Values page. Then come back here to say what
              happens when it changes.
            </p>
          )}
          {hasValues ? (
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
            <th scope="col">Needs running</th>
            <th scope="col">
              <span className="note">Actions</span>
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
                confirming={confirming === id}
                onConfirm={(confirm) => {
                  setConfirming(confirm ? id : null);
                }}
                onToggle={() => {
                  background(
                    run(async () => {
                      await setAutomationEnabled(id, !enabled);
                    }),
                  );
                }}
                onEdit={() => {
                  onEdit(summary);
                }}
                onDelete={() => {
                  background(
                    run(async () => {
                      await deleteAutomation(id);
                    }),
                  );
                }}
              />
            );
          })}
        </tbody>
      </table>
    );
  })();

  return (
    <main className="page">
      {head}
      {loadError === null ? null : <p className="warn">{loadError}</p>}
      {actionError === null ? null : <p className="warn">{actionError}</p>}
      {body}
      {automations !== null && automations.length > 0 ? (
        <p className="note">
          Each automation watches one value for now; conditions that combine several values come later. The runtime
          starts running saved automations in the next version of AppyTappy.
        </p>
      ) : null}
    </main>
  );
}
