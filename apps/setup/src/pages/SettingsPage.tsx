import { type KeyboardEvent, useEffect, useState } from 'react';

import { errorText } from '../lib/api.ts';
import { background } from '../lib/repeat.ts';
import {
  FASTEST_READ_MS,
  type ReadInterval,
  SLOWEST_READ_MS,
  readMs,
  setStartsWithWindows,
  startsWithWindows,
  useSettings,
  withWait,
} from '../lib/settings.ts';
import { accelerator, shortcutLabel } from '../lib/shortcut.ts';

interface Props {
  /** Why the saved settings cannot be read, when the page shows the defaults. */
  readonly problem: string | null;
}

/** Whether the runtime starts at sign-in, as Windows says, with the switch that changes it. */
function StartWithWindows() {
  const [on, setOn] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let current = true;
    startsWithWindows().then(
      (starts) => {
        if (current) {
          setOn(starts);
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
  }, []);
  const toggle = async (): Promise<void> => {
    if (on === null || busy) {
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
        aria-disabled={on === null || busy}
        onClick={() => {
          background(toggle());
        }}
      />
    </div>
  );
}

/** The screenshot shortcut, and a button that records a new one from the next key press. */
function ScreenshotShortcut({ onError }: { readonly onError: (error: string | null) => void }) {
  const { settings, change } = useSettings();
  const [changing, setChanging] = useState(false);
  const onKey = (event: KeyboardEvent<HTMLButtonElement>): void => {
    if (!changing) {
      return;
    }
    event.preventDefault();
    if (event.code === 'Escape') {
      setChanging(false);
      return;
    }
    const next = accelerator(event);
    if (next !== null) {
      setChanging(false);
      change({ ...settings, screenshotShortcut: next }).then(
        () => {
          onError(null);
        },
        (failure: unknown) => {
          onError(errorText(failure));
        },
      );
    }
  };
  return (
    <div className="set-row">
      <div className="what">
        <span>Screenshot shortcut</span>
        <span className="note">Takes a screenshot of the picked window while you make a value or a reference.</span>
      </div>
      <span className="chip">{shortcutLabel(settings.screenshotShortcut)}</span>
      <button
        type="button"
        className="btn small"
        aria-pressed={changing}
        onClick={() => {
          setChanging((current) => !current);
        }}
        onBlur={() => {
          setChanging(false);
        }}
        onKeyDown={onKey}
      >
        {changing ? 'Press the new shortcut, or Esc' : 'Change'}
      </button>
    </div>
  );
}

interface WaitProps {
  readonly which: keyof ReadInterval;
  readonly label: string;
  readonly note: string;
  readonly onError: (error: string | null) => void;
}

/** One of the read interval's waits, saved when the field loses focus or Enter is pressed. */
function Wait({ which, label, note, onError }: WaitProps) {
  const { settings, change } = useSettings();
  const saved = settings.readInterval[which];
  const [text, setText] = useState(String(saved));
  useEffect(() => {
    setText(String(saved));
  }, [saved]);
  const commit = (): void => {
    const ms = readMs(text);
    if (ms === null) {
      onError(`${label} must be a whole number of milliseconds from ${FASTEST_READ_MS} to ${SLOWEST_READ_MS}.`);
      return;
    }
    if (ms === saved) {
      onError(null);
      return;
    }
    const next = withWait(settings.readInterval, which, ms);
    if ('problem' in next) {
      onError(next.problem);
      return;
    }
    change({ ...settings, readInterval: next.interval }).then(
      () => {
        onError(null);
      },
      (failure: unknown) => {
        onError(errorText(failure));
      },
    );
  };
  const id = `wait-${which}`;
  return (
    <div className="set-row">
      <div className="what">
        <label htmlFor={id}>{label}</label>
        <span className="note">{note}</span>
      </div>
      <span className="num">
        <input
          id={id}
          className="input"
          inputMode="numeric"
          value={text}
          onChange={(event) => {
            setText(event.target.value);
          }}
          onBlur={commit}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              commit();
            }
          }}
        />
        ms
      </span>
    </div>
  );
}

/** The settings that apply to AppyTappy as a whole. Each change is saved as it is made. */
export function SettingsPage({ problem }: Props) {
  const [generalError, setGeneralError] = useState<string | null>(null);
  const [readingError, setReadingError] = useState<string | null>(null);
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
          <ScreenshotShortcut onError={setGeneralError} />
          {generalError === null ? null : (
            <p className="warn" role="alert">
              {generalError}
            </p>
          )}
        </section>
        <section className="card" aria-labelledby="settings-reading">
          <h2 id="settings-reading">Reading</h2>
          <Wait
            which="fastestMs"
            label="Fastest read"
            note="How often a value is read while it changes. Shorter notices changes sooner."
            onError={setReadingError}
          />
          <Wait
            which="slowestMs"
            label="Slowest read"
            note="How long AppyTappy waits at most while a value holds still. Longer uses less CPU."
            onError={setReadingError}
          />
          {readingError === null ? null : (
            <p className="warn" role="alert">
              {readingError}
            </p>
          )}
        </section>
      </div>
    </main>
  );
}
