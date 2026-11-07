import { type KeyboardEvent, useEffect, useId, useRef, useState } from 'react';

import { type AntiCheat, type AppSummary, type OverlayMode, errorText, setAntiCheat, setOverlay } from './api.ts';
import { ANTI_CHEAT_NAMES, PRIORITY_NAMES } from './describe.ts';
import { background } from './repeat.ts';

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
}

/**
 * An application's anti-cheat, picked from a menu. None lets automations
 * press keys into it, so picking None asks first.
 */
function AntiCheatCell({ name, antiCheat, confirming, onConfirm, onSet }: AntiCheatCellProps) {
  const keep = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLSelectElement>(null);
  const question = useRef<HTMLSpanElement>(null);
  // Whether focus was in the question when it went, so it comes back to the menu only then.
  const hadFocus = useRef(false);
  const warning = useId();
  // The question takes the menu's place, so focus moves to its safe answer, and back to the menu after.
  useEffect(() => {
    if (confirming) {
      keep.current?.focus();
    } else if (hadFocus.current) {
      hadFocus.current = false;
      menu.current?.focus();
    }
  }, [confirming]);
  const answer = (set: boolean): void => {
    hadFocus.current = question.current?.contains(document.activeElement) ?? false;
    if (set) {
      onSet('none');
    } else {
      onConfirm(false);
    }
  };
  // Escape is the same as Keep, on either answer.
  const escape = (event: KeyboardEvent): void => {
    if (event.key === 'Escape') {
      event.preventDefault();
      answer(false);
    }
  };
  if (confirming) {
    return (
      <span
        ref={question}
        className="confirm-none"
        role="alertdialog"
        aria-label={`Set ${name}'s anti-cheat to None?`}
        aria-describedby={warning}
      >
        <span id={warning} className="warn">
          Only pick None if {name} has no anti-cheat. Pressing keys into an application with anti-cheat can get your
          account banned.
        </span>
        <span className="confirm">
          <button
            ref={keep}
            type="button"
            className="btn small"
            onKeyDown={escape}
            onClick={() => {
              answer(false);
            }}
          >
            Keep {ANTI_CHEAT_NAMES[antiCheat]}
          </button>
          <button
            type="button"
            className="btn small danger"
            onKeyDown={escape}
            onClick={() => {
              answer(true);
            }}
          >
            Set to None
          </button>
        </span>
      </span>
    );
  }
  return (
    <select
      ref={menu}
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

interface Props {
  readonly apps: readonly AppSummary[] | null;
  readonly loadError: string | null;
  readonly onChanged: () => Promise<void>;
}

export function ApplicationsPage({ apps, loadError, onChanged }: Props) {
  const [confirming, setConfirming] = useState<string | null>(null);
  const [setError, setSetError] = useState<string | null>(null);

  const set = async (app: string, antiCheat: AntiCheat): Promise<void> => {
    try {
      await setAntiCheat(app, antiCheat);
      setSetError(null);
      setConfirming(null);
      await onChanged();
    } catch (error: unknown) {
      setSetError(errorText(error));
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

  return (
    <main className="page">
      <div className="page-head">
        <h1>Applications</h1>
        <span className="lead">
          How AppyTappy recognizes each application&apos;s window, whether automations may press keys into it, and
          whether its live values show on top of it while this app is open.
        </span>
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
          <p>An application is saved with its first value. Add a value on the Values page to start.</p>
        </div>
      ) : null}
      {apps !== null && apps.length > 0 ? (
        <table className="table">
          <thead>
            <tr>
              <th scope="col">Application</th>
              <th scope="col">Window title</th>
              <th scope="col">Class</th>
              <th scope="col">Executable</th>
              <th scope="col">Fallback</th>
              <th scope="col">Anti-cheat</th>
              <th scope="col">Overlay</th>
              <th scope="col">Running</th>
            </tr>
          </thead>
          <tbody>
            {apps.map((app) =>
              app.file === null ? (
                <tr key={app.id}>
                  <td>{app.id}</td>
                  <td colSpan={7} className="warn">
                    {app.error ?? 'This application could not be read.'}
                  </td>
                </tr>
              ) : (
                <tr key={app.id}>
                  <td>{app.file.name}</td>
                  <td>{app.file.window.title}</td>
                  <td>{app.file.window.class}</td>
                  <td>{app.file.window.executable}</td>
                  <td className="note">{PRIORITY_NAMES[app.file.window.priority]}</td>
                  <td>
                    <AntiCheatCell
                      name={app.file.name}
                      antiCheat={app.file.antiCheat}
                      confirming={confirming === app.id}
                      onConfirm={(now) => {
                        setConfirming(now ? app.id : null);
                      }}
                      onSet={(antiCheat) => {
                        background(set(app.id, antiCheat));
                      }}
                    />
                  </td>
                  <td>
                    <select
                      className="input"
                      aria-label={`${app.file.name}: overlay`}
                      value={overlayChoice(app.file.overlay)}
                      onChange={(event) => {
                        const picked = OVERLAY_CHOICES.find((choice) => choice === event.target.value);
                        if (picked !== undefined) {
                          background(changeOverlay(app.id, picked, app.file?.overlay?.mode ?? 'overlay'));
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
                </tr>
              ),
            )}
          </tbody>
        </table>
      ) : null}
    </main>
  );
}
