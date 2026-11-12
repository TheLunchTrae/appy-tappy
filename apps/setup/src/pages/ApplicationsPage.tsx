import { useRef, useState } from 'react';

import type { AntiCheatDef, AppSummary, DraftSummary, OverlayModeDef } from '../lib/bindings.ts';
import { Question, useQuestion } from '../components/Question.tsx';
import { errorText, removeApp, setAntiCheat, setOverlay } from '../lib/api.ts';
import { Hint } from '../components/Hint.tsx';
import { ANTI_CHEAT_NAMES } from '../lib/describe.ts';
import { ANTI_CHEAT_HINT, OVERLAY_HINT } from '../lib/hints.ts';
import { type AppKept, matchSummary } from '../lib/matching.ts';
import { background } from '../lib/repeat.ts';
import {
  type Resumed,
  draftOfSaved,
  draftProblem,
  draftsWith,
  isKept,
  newDrafts,
  removalLoses,
} from '../lib/drafts.ts';
import { DraftActions, DraftNote, DraftPill } from '../components/Drafts.tsx';

/**
 * The anti-cheat menu's choices: not set first, since every application starts there, and none last,
 * so stepping through the menu with the arrow keys reaches every other choice before the one that asks.
 */
const ANTI_CHEAT_CHOICES: readonly AntiCheatDef[] = ['unknown', 'userMode', 'kernelLevel', 'none'];

interface AntiCheatCellProps {
  readonly name: string;
  readonly antiCheat: AntiCheatDef;
  readonly confirming: boolean;
  readonly onConfirm: (confirming: boolean) => void;
  readonly onSet: (antiCheat: AntiCheatDef) => void;
  /** Whether a change is saving, which locks the question's answers. */
  readonly busy: boolean;
}

/**
 * An application's anti-cheat, picked from a menu. None lets automations
 * press keys into it, so picking None asks first, in the menu's place.
 */
function AntiCheatCell({ name, antiCheat, confirming, onConfirm, onSet, busy }: AntiCheatCellProps) {
  const { control, refs } = useQuestion<HTMLSelectElement>(confirming);
  if (confirming) {
    return (
      <Question
        refs={refs}
        label={`Set ${name}'s anti-cheat to None?`}
        keepLabel={`Keep ${ANTI_CHEAT_NAMES[antiCheat]}`}
        actLabel="Set to None"
        busy={busy}
        onKeep={() => {
          onConfirm(false);
        }}
        onAct={() => {
          onSet('none');
        }}
      >
        Only pick None if {name} has no anti-cheat. Pressing keys into an application with anti-cheat can get your
        account banned.
      </Question>
    );
  }
  return (
    <select
      ref={control}
      className="input"
      aria-label={`${name}: anti-cheat`}
      value={antiCheat}
      onChange={(event) => {
        const picked = ANTI_CHEAT_CHOICES.find((choice) => choice === event.target.value);
        if (picked === 'none') {
          onConfirm(true);
        } else if (picked !== undefined) {
          onSet(picked);
        }
      }}
    >
      {ANTI_CHEAT_CHOICES.map((choice) => (
        <option key={choice} value={choice}>
          {ANTI_CHEAT_NAMES[choice]}
        </option>
      ))}
    </select>
  );
}

/** The overlay menu's choices: off, or one of the two places the readings show. */
type OverlayChoice = 'off' | OverlayModeDef;

const OVERLAY_NAMES: Readonly<Record<OverlayChoice, string>> = {
  off: 'Off',
  overlay: 'Over the window',
  pictureInPicture: 'Picture in picture',
};

const OVERLAY_CHOICES: readonly OverlayChoice[] = ['off', 'overlay', 'pictureInPicture'];

/** The overlay menu's choice for a saved overlay setting. */
function overlayChoice(
  overlay: { readonly enabled: boolean; readonly mode: OverlayModeDef } | undefined,
): OverlayChoice {
  return overlay?.enabled === true ? overlay.mode : 'off';
}

interface RemoveCellProps {
  readonly name: string;
  /** What else goes with the application, said in the question. */
  readonly loses: string;
  readonly confirming: boolean;
  readonly onConfirm: (confirming: boolean) => void;
  readonly onRemove: () => void;
  /** Whether the removal is under way, which locks both answers. */
  readonly busy: boolean;
}

/** Removing an application, which asks first, in the button's place, since its values go with it. */
function RemoveCell({ name, loses, confirming, onConfirm, onRemove, busy }: RemoveCellProps) {
  const { control, refs } = useQuestion<HTMLButtonElement>(confirming);
  if (confirming) {
    return (
      <Question
        refs={refs}
        label={`Remove ${name}?`}
        keepLabel="Keep"
        actLabel="Remove"
        busy={busy}
        onKeep={() => {
          onConfirm(false);
        }}
        onAct={onRemove}
      >
        Remove {name}? {loses}
      </Question>
    );
  }
  return (
    <button
      ref={control}
      type="button"
      className="btn small"
      disabled={busy}
      aria-label={`Remove ${name}`}
      onClick={() => {
        onConfirm(true);
      }}
    >
      Remove
    </button>
  );
}

interface Props {
  readonly apps: readonly AppSummary[] | null;
  readonly loadError: string | null;
  readonly onChanged: () => Promise<void>;
  readonly onAdd: () => void;
  readonly onEdit: (app: AppSummary) => void;
  /** Every saved draft, or null before they are read. */
  readonly drafts: readonly DraftSummary[] | null;
  readonly onFinish: (resumed: Resumed) => void;
  /** Deletes a draft; the page shows why when it fails. */
  readonly onDeleteDraft: (id: string) => Promise<void>;
}

/** A new application saved as a draft: how it would be found, and that nothing is set or watched yet. */
function DraftRow({
  resumed,
  onFinish,
  onDelete,
}: {
  readonly resumed: Resumed;
  readonly onFinish: () => void;
  readonly onDelete: () => Promise<void>;
}) {
  const { def } = resumed;
  const problem = draftProblem(def, []);
  const window = isKept<AppKept>(def.state, 'application') ? def.state.state.window : null;
  const summary = window === null ? null : matchSummary(window);
  return (
    <tr className="draft-row">
      <td>
        {def.name} <DraftPill />
        <DraftNote missing={def.missing} problem={problem} />
      </td>
      <td className="recog">
        {summary === null ? (
          <span className="note">No window picked yet</span>
        ) : (
          <>
            <span>{summary.must}</span>
            {summary.then === null ? null : <span className="note">{summary.then}</span>}
          </>
        )}
      </td>
      <td className="note">Not set</td>
      <td className="note">Off</td>
      <td className="note">Not watched</td>
      <td className="row-actions">
        <DraftActions name={def.name} problem={problem} onFinish={onFinish} onDelete={onDelete} />
      </td>
    </tr>
  );
}

export function ApplicationsPage({
  apps,
  loadError,
  onChanged,
  onAdd,
  onEdit,
  drafts,
  onFinish,
  onDeleteDraft,
}: Props) {
  const [confirming, setConfirming] = useState<string | null>(null);
  const [removing, setRemoving] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // A removed application's row goes, so focus moves to Add application.
  const add = useRef<HTMLButtonElement>(null);
  const [setError, setSetError] = useState<string | null>(null);
  // The application whose anti-cheat is saving.
  const [settingAntiCheat, setSettingAntiCheat] = useState<string | null>(null);

  const set = async (app: string, antiCheat: AntiCheatDef): Promise<void> => {
    setSettingAntiCheat(app);
    try {
      await setAntiCheat(app, antiCheat);
      setSetError(null);
      setConfirming(null);
      await onChanged();
    } catch (error: unknown) {
      setSetError(errorText(error));
    } finally {
      setSettingAntiCheat(null);
    }
  };

  const remove = async (app: string): Promise<void> => {
    setBusy(true);
    try {
      setSetError(await removeApp(app));
      add.current?.focus();
      await onChanged();
    } catch (error: unknown) {
      setSetError(errorText(error));
    } finally {
      setBusy(false);
      setRemoving(null);
    }
  };

  const changeOverlay = async (app: string, choice: OverlayChoice, kept: OverlayModeDef): Promise<void> => {
    try {
      // Turning it off keeps the mode, so turning it on again shows it where it was.
      setSetError(await (choice === 'off' ? setOverlay(app, false, kept) : setOverlay(app, true, choice)));
      await onChanged();
    } catch (error: unknown) {
      setSetError(errorText(error));
    }
  };

  const headingRef = useRef<HTMLHeadingElement>(null);
  const removeDraft = async (id: string): Promise<void> => {
    try {
      await onDeleteDraft(id);
      setSetError(null);
      // The deleted row took the focused button with it.
      headingRef.current?.focus();
    } catch (error: unknown) {
      setSetError(errorText(error));
    }
  };
  const fresh = newDrafts(
    drafts,
    'application',
    (id) => apps?.some((app) => app.id === id && app.file !== null) ?? false,
  );

  const removeCell = (app: AppSummary, name: string) => (
    <RemoveCell
      name={name}
      loses={removalLoses(app.file?.values.length ?? 0, draftsWith(drafts, app.id))}
      confirming={removing === app.id}
      busy={busy}
      onConfirm={(now) => {
        setRemoving(now ? app.id : null);
      }}
      onRemove={() => {
        background(remove(app.id));
      }}
    />
  );

  return (
    <main className="page">
      <div className="page-head">
        <h1 ref={headingRef} tabIndex={-1}>
          Applications
        </h1>
        <span className="lead">
          How AppyTappy finds each application&apos;s window, whether automations may press keys into it, and whether
          its live values show on top of it while this app is open.
        </span>
        <div className="end">
          <button ref={add} type="button" className="btn primary" onClick={onAdd}>
            Add application
          </button>
        </div>
      </div>
      {loadError === null ? null : <p className="warn">{loadError}</p>}
      {setError === null ? null : (
        <p className="warn" role="alert">
          {setError}
        </p>
      )}
      {apps?.length === 0 && fresh.length === 0 ? (
        <div className="empty">
          <h2>No applications yet</h2>
          <p>Add the application whose screen AppyTappy should read, then give it values on the Values page.</p>
        </div>
      ) : null}
      {apps !== null && (apps.length > 0 || fresh.length > 0) ? (
        <table className="table">
          <thead>
            <tr>
              <th scope="col">Application</th>
              <th scope="col">Found by</th>
              <th scope="col" aria-label="Anti-cheat">
                <span className="with-hint">
                  Anti-cheat
                  <Hint topic="Anti-cheat">{ANTI_CHEAT_HINT}</Hint>
                </span>
              </th>
              <th scope="col" aria-label="Overlay">
                <span className="with-hint">
                  Overlay
                  <Hint topic="Overlay">{OVERLAY_HINT}</Hint>
                </span>
              </th>
              <th scope="col">Running</th>
              <th scope="col">
                <span className="visually-hidden">Change</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {apps.map((app) => {
              if (app.file === null) {
                return (
                  <tr key={app.id}>
                    <td>{app.id}</td>
                    <td colSpan={4} className="recog">
                      <span className="warn">
                        AppyTappy can&apos;t read this application, so it is never found. Remove it and add it again.
                      </span>
                      {app.error === null ? null : <span className="note">{app.error}</span>}
                    </td>
                    <td className="row-actions">{removeCell(app, app.id)}</td>
                  </tr>
                );
              }
              const file = app.file;
              const summary = matchSummary(file.window);
              const changes = draftOfSaved(drafts, 'application', app.id);
              return (
                <tr key={app.id}>
                  <td>
                    {file.name}
                    {changes === null ? null : (
                      <>
                        {' '}
                        <DraftPill changes />
                      </>
                    )}
                  </td>
                  <td className="recog">
                    <span>{summary.must}</span>
                    {summary.then === null ? null : <span className="note">{summary.then}</span>}
                  </td>
                  <td>
                    <AntiCheatCell
                      name={file.name}
                      antiCheat={file.antiCheat}
                      confirming={confirming === app.id}
                      onConfirm={(now) => {
                        setConfirming(now ? app.id : null);
                      }}
                      onSet={(antiCheat) => {
                        background(set(app.id, antiCheat));
                      }}
                      busy={settingAntiCheat === app.id}
                    />
                  </td>
                  <td>
                    <select
                      className="input"
                      aria-label={`${file.name}: overlay`}
                      value={overlayChoice(file.overlay)}
                      onChange={(event) => {
                        const picked = OVERLAY_CHOICES.find((choice) => choice === event.target.value);
                        if (picked !== undefined) {
                          background(changeOverlay(app.id, picked, file.overlay?.mode ?? 'overlay'));
                        }
                      }}
                    >
                      {OVERLAY_CHOICES.map((choice) => (
                        <option key={choice} value={choice}>
                          {OVERLAY_NAMES[choice]}
                        </option>
                      ))}
                    </select>
                  </td>
                  <td>
                    {app.windows.length > 0 ? (
                      <span className="ok">
                        {app.windows.length === 1 ? '1 window' : `${app.windows.length} windows`}
                      </span>
                    ) : (
                      <span className="note">No</span>
                    )}
                  </td>
                  <td className="row-actions">
                    {removing === app.id || changes === null ? null : (
                      // Its draft holds the changes, so editing goes on from there rather than from the saved one.
                      <button
                        type="button"
                        className="btn small"
                        onClick={() => {
                          onFinish(changes);
                        }}
                      >
                        Finish draft<span className="visually-hidden"> of {file.name}</span>
                      </button>
                    )}
                    {removing === app.id || changes !== null ? null : (
                      <button
                        type="button"
                        className="btn small"
                        aria-label={`Edit ${file.name}`}
                        onClick={() => {
                          onEdit(app);
                        }}
                      >
                        Edit
                      </button>
                    )}
                    {removeCell(app, file.name)}
                  </td>
                </tr>
              );
            })}
            {fresh.map((resumed) => (
              <DraftRow
                key={resumed.id}
                resumed={resumed}
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
      ) : null}
    </main>
  );
}
