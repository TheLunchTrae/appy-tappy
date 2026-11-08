import { useRef, useState } from 'react';

import { Question, useQuestion } from '../components/Question.tsx';
import {
  type AntiCheat,
  type AppSummary,
  type OverlayMode,
  errorText,
  removeApp,
  setAntiCheat,
  setOverlay,
} from '../lib/api.ts';
import { ANTI_CHEAT_NAMES } from '../lib/describe.ts';
import { matchSummary } from '../lib/matching.ts';
import { background } from '../lib/repeat.ts';

/**
 * The anti-cheat menu's choices: not set first, since every application starts there, and none last,
 * so stepping through the menu with the arrow keys reaches every other choice before the one that asks.
 */
const ANTI_CHEAT_CHOICES: readonly AntiCheat[] = ['unknown', 'userMode', 'kernelLevel', 'none'];

interface AntiCheatCellProps {
  readonly name: string;
  readonly antiCheat: AntiCheat;
  readonly confirming: boolean;
  readonly onConfirm: (confirming: boolean) => void;
  readonly onSet: (antiCheat: AntiCheat) => void;
  /** Whether a change is saving, which locks the menu and the question's answers. */
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
      disabled={busy}
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
type OverlayChoice = 'off' | OverlayMode;

const OVERLAY_NAMES: Readonly<Record<OverlayChoice, string>> = {
  off: 'Off',
  overlay: 'Over the window',
  pictureInPicture: 'Picture in picture',
};

const OVERLAY_CHOICES: readonly OverlayChoice[] = ['off', 'overlay', 'pictureInPicture'];

/** The overlay menu's choice for a saved overlay setting. */
function overlayChoice(overlay: { readonly enabled: boolean; readonly mode: OverlayMode } | undefined): OverlayChoice {
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

/** What removing an application takes with it. */
function loses(app: AppSummary): string {
  const values = app.file?.values.length ?? 0;
  if (values === 0) {
    return 'This cannot be undone.';
  }
  return `Its ${values === 1 ? 'value goes' : `${values} values go`} with it, and this cannot be undone.`;
}

interface Props {
  readonly apps: readonly AppSummary[] | null;
  readonly loadError: string | null;
  readonly onChanged: () => Promise<void>;
  readonly onAdd: () => void;
  readonly onEdit: (app: AppSummary) => void;
}

export function ApplicationsPage({ apps, loadError, onChanged, onAdd, onEdit }: Props) {
  const [confirming, setConfirming] = useState<string | null>(null);
  const [removing, setRemoving] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // A removed application's row goes, so focus moves to Add application.
  const add = useRef<HTMLButtonElement>(null);
  const [setError, setSetError] = useState<string | null>(null);
  // The application whose anti-cheat is saving.
  const [settingAntiCheat, setSettingAntiCheat] = useState<string | null>(null);

  const set = async (app: string, antiCheat: AntiCheat): Promise<void> => {
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
      await removeApp(app);
      setSetError(null);
      add.current?.focus();
      await onChanged();
    } catch (error: unknown) {
      setSetError(errorText(error));
    } finally {
      setBusy(false);
      setRemoving(null);
    }
  };

  const changeOverlay = async (app: string, choice: OverlayChoice, kept: OverlayMode): Promise<void> => {
    try {
      // Turning it off keeps the mode, so turning it on again shows it where it was.
      await (choice === 'off' ? setOverlay(app, false, kept) : setOverlay(app, true, choice));
      setSetError(null);
      await onChanged();
    } catch (error: unknown) {
      setSetError(errorText(error));
    }
  };

  const removeCell = (app: AppSummary, name: string) => (
    <RemoveCell
      name={name}
      loses={loses(app)}
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
        <h1>Applications</h1>
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
      {apps?.length === 0 ? (
        <div className="empty">
          <h2>No applications yet</h2>
          <p>Add the application whose screen AppyTappy should read, then give it values on the Values page.</p>
        </div>
      ) : null}
      {apps !== null && apps.length > 0 ? (
        <table className="table">
          <thead>
            <tr>
              <th scope="col">Application</th>
              <th scope="col">Found by</th>
              <th scope="col">Anti-cheat</th>
              <th scope="col">Overlay</th>
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
              return (
                <tr key={app.id}>
                  <td>{file.name}</td>
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
                    {removing === app.id ? null : (
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
          </tbody>
        </table>
      ) : null}
    </main>
  );
}
