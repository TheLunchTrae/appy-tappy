import { useEffect, useState } from 'react';

import { LIMITS, type ReadIntervalDef } from '../lib/bindings.ts';
import { errorText } from '../lib/api.ts';
import { READING_HINT, SHORTCUT_HINT } from '../lib/hints.ts';
import { background } from '../lib/repeat.ts';
import { Hint } from '../components/Hint.tsx';
import { NumberSetting } from '../components/NumberSetting.tsx';
import { RuntimeTrace } from '../components/RuntimeTrace.tsx';
import { setStartsWithWindows, startsWithWindows, useSettings, withWait } from '../lib/settings.ts';
import { useShortcutChoice } from '../lib/useShortcutChoice.ts';

interface Props {
  /** Why the saved settings cannot be read, when the page shows the defaults. */
  readonly problem: string | null;
}

/** Whether the runtime starts at sign-in, as Windows says, with the switch that changes it. */
function StartWithWindows() {
  const [on, setOn] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Asking Windows again after a failed read is what a press does while the state is unknown.
  const [asks, setAsks] = useState(0);
  useEffect(() => {
    let current = true;
    startsWithWindows().then(
      (starts) => {
        if (current) {
          setOn(starts);
          setError(null);
        }
      },
      (failure: unknown) => {
        if (current) {
          setError(errorText(failure));
        }
      },
    );
    return () => {
      current = false;
    };
  }, [asks]);
  const toggle = async (): Promise<void> => {
    if (busy) {
      return;
    }
    if (on === null) {
      if (error !== null) {
        setAsks((count) => count + 1);
      }
      return;
    }
    setBusy(true);
    try {
      await setStartsWithWindows(!on);
      setOn(!on);
      setError(null);
    } catch (failure: unknown) {
      setError(errorText(failure));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="set-row">
      <div className="what">
        <span id="start-with-windows">Start AppyTappy when I sign in to Windows</span>
        <span className="note">Runs automations in the background from sign-in.</span>
        {error === null ? null : (
          <span className="warn" role="alert">
            {error}
          </span>
        )}
      </div>
      <button
        type="button"
        role="switch"
        className="switch"
        aria-labelledby="start-with-windows"
        aria-checked={on === true}
        aria-busy={busy}
        aria-disabled={busy || (on === null && error === null)}
        onClick={() => {
          background(toggle());
        }}
      />
    </div>
  );
}

/** The screenshot shortcut, and a button that records a new one from the next key press. */
function ScreenshotShortcut() {
  const { label, choosing, saveError, toggle, cancel, onKeyDown } = useShortcutChoice();
  return (
    <div className="set-row">
      <div className="what">
        <span className="with-hint">
          Screenshot shortcut
          <Hint topic="Screenshot shortcut">{SHORTCUT_HINT}</Hint>
        </span>
        <span className="note">Takes a screenshot of the picked window while you make a value or a reference.</span>
        <span className="note" role="status">
          {choosing ? 'Press the new shortcut, or Esc to keep this one.' : null}
        </span>
        {saveError === null ? null : (
          <span className="warn" role="alert">
            {saveError}
          </span>
        )}
      </div>
      <span className="chip">{label}</span>
      <button
        type="button"
        className="btn small"
        aria-label="Change screenshot shortcut"
        aria-pressed={choosing}
        onClick={toggle}
        onBlur={cancel}
        onKeyDown={onKeyDown}
      >
        Change
      </button>
    </div>
  );
}

interface WaitProps {
  readonly which: keyof ReadIntervalDef;
  readonly label: string;
  readonly note: string;
}

/** One of the read interval's waits. */
function Wait({ which, label, note }: WaitProps) {
  const { settings } = useSettings();
  return (
    <NumberSetting
      id={`wait-${which}`}
      label={label}
      note={note}
      unit="ms"
      saved={settings.readInterval[which]}
      min={LIMITS.fastestReadMs}
      max={LIMITS.slowestReadMs}
      invalid={`${label} must be a whole number of milliseconds from ${LIMITS.fastestReadMs} to ${LIMITS.slowestReadMs}.`}
      update={(current, ms) => {
        const next = withWait(current.readInterval, which, ms);
        return 'problem' in next ? next : { ...current, readInterval: next.interval };
      }}
    />
  );
}

/** The settings that apply to AppyTappy as a whole. Each change is saved as it is made. */
export function SettingsPage({ problem }: Props) {
  return (
    <main className="page">
      <div className="page-head">
        <h1>Settings</h1>
        <span className="lead">Changes apply right away.</span>
      </div>
      {problem === null ? null : (
        <p className="warn" role="alert">
          {problem}
        </p>
      )}
      <div className="settings">
        <section className="card" aria-labelledby="settings-general">
          <h2 id="settings-general">General</h2>
          <StartWithWindows />
          <ScreenshotShortcut />
        </section>
        <section className="card" aria-labelledby="settings-reading">
          <div className="with-hint">
            <h2 id="settings-reading">Reading</h2>
            <Hint topic="Reading">{READING_HINT}</Hint>
          </div>
          <Wait
            which="fastestMs"
            label="Fastest read"
            note="How often a value is read while it changes. Shorter notices changes sooner."
          />
          <Wait
            which="slowestMs"
            label="Slowest read"
            note="How long AppyTappy waits at most while a value holds still. Longer uses less CPU."
          />
        </section>
        <RuntimeTrace />
      </div>
    </main>
  );
}
