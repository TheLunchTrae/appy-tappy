import { useEffect, useState } from 'react';

import { LIMITS, type SettingsFile, TRACE_CATEGORIES, type TraceCategory } from '../lib/bindings.ts';
import { errorText } from '../lib/api.ts';
import { background } from '../lib/repeat.ts';
import {
  type TraceLength,
  openTraceFolder,
  traceEnd,
  traceFolder,
  traceRecords,
  useSettings,
  withCategory,
} from '../lib/settings.ts';
import { NumberSetting } from './NumberSetting.tsx';

/** Each category's name and what it records. */
const CATEGORIES: Readonly<Record<TraceCategory, { readonly name: string; readonly note: string }>> = {
  readings: {
    name: 'Each read',
    note: 'Every read of every value and how long it took. Makes large files quickly.',
  },
  changes: { name: 'Value changes', note: 'Each changed value, and how long it took to reach the rules.' },
  rules: {
    name: 'Rules',
    note: 'Each event the runtime handled, and each rule that fired, waited for its cooldown, or was released.',
  },
  actions: { name: 'Actions', note: 'When each automation’s actions were queued, started, and finished.' },
  lifecycle: {
    name: 'Applications and reloads',
    note: 'Each check for running applications, readers starting and stopping, and each reload.',
  },
};

/** How long a trace can record for, as the page offers it. */
const LENGTHS: readonly { readonly length: TraceLength; readonly name: string }[] = [
  { length: 10, name: '10 minutes' },
  { length: 60, name: '1 hour' },
  { length: 'off', name: 'Until I turn it off' },
];

/** Where the page keeps the length picked last, which the settings file has no place for. */
const LENGTH_KEY = 'trace-length';

/** The length picked last, if one is kept and is still offered. */
function keptLength(): TraceLength | null {
  try {
    const kept = window.localStorage.getItem(LENGTH_KEY);
    return LENGTHS.find((each) => String(each.length) === kept)?.length ?? null;
  } catch {
    return null;
  }
}

/** Keeps `length` for the next time the page opens; the page works without it. */
function keepLength(length: TraceLength): void {
  try {
    window.localStorage.setItem(LENGTH_KEY, String(length));
  } catch {
    // A page that cannot keep it starts at 1 hour next time.
  }
}

/** The length the page shows first: the one the saved trace runs for, or the one picked last. */
function firstLength(on: boolean, until: number | null): TraceLength {
  if (on && until === null) {
    return 'off';
  }
  const kept = keptLength();
  return kept === null || (on && kept === 'off') ? 60 : kept;
}

/** `until` as the user reads it: the time, with the day when it is not today. */
function endText(until: number): string {
  const end = new Date(until);
  const today = new Date().toDateString() === end.toDateString();
  return today
    ? end.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })
    : end.toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' });
}

/** The time now, which moves on when a trace's end time passes, so the page shows the trace stopped. */
function useNowUntil(until: number | null): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (until === null || until <= now) {
      return undefined;
    }
    const timer = setTimeout(
      () => {
        setNow(Date.now());
      },
      until - now + 50,
    );
    return () => {
      clearTimeout(timer);
    };
  }, [until, now]);
  return now;
}

/** The folder the trace files go in, and a button that opens it. */
function TraceFolder() {
  const [folder, setFolder] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let current = true;
    traceFolder().then(
      (found) => {
        if (current) {
          setFolder(found);
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
  const open = async (): Promise<void> => {
    try {
      await openTraceFolder();
      setError(null);
    } catch (failure: unknown) {
      setError(errorText(failure));
    }
  };
  return (
    <div className="set-row">
      <div className="what">
        <span id="trace-folder">Trace folder</span>
        <span className="note path">{folder ?? ''}</span>
        {error === null ? null : (
          <span className="warn" role="alert">
            {error}
          </span>
        )}
      </div>
      <button
        type="button"
        className="btn small"
        aria-describedby="trace-folder"
        onClick={() => {
          background(open());
        }}
      >
        Open folder
      </button>
    </div>
  );
}

/**
 * The runtime trace: a switch that records what the runtime reads and does to a file, for how long, what it records,
 * and how much disk its files may take. Each change is saved as it is made, and the runtime starts or stops the trace
 * when it reads the settings again.
 */
export function RuntimeTrace() {
  const { settings, change } = useSettings();
  const { trace } = settings;
  const now = useNowUntil(trace.on ? trace.until : null);
  const recording = traceRecords(trace, now);
  const [length, setLength] = useState<TraceLength>(() => firstLength(trace.on, trace.until));
  const [error, setError] = useState<string | null>(null);
  const save = (update: (current: SettingsFile) => SettingsFile | { readonly problem: string }): void => {
    change(update).then(
      (problem) => {
        setError(problem);
      },
      (failure: unknown) => {
        setError(errorText(failure));
      },
    );
  };
  const turn = (on: boolean): void => {
    save((current) => ({
      ...current,
      trace: { ...current.trace, on, until: on ? traceEnd(length, Date.now()) : null },
    }));
  };
  const status = recording
    ? trace.until === null
      ? 'Recording until you turn it off.'
      : `Recording until ${endText(trace.until)}.`
    : trace.on && trace.until !== null
      ? `Stopped at ${endText(trace.until)}.`
      : 'Not recording.';
  return (
    <section className="card" aria-labelledby="settings-trace">
      <h2 id="settings-trace">Runtime trace</h2>
      <div className="set-row">
        <div className="what">
          <span id="trace-on">Record a runtime trace</span>
          <span className="note" id="trace-on-note">
            Writes what the runtime reads and does, with timings, to a file for measuring how fast it responds.
            Recording makes the runtime do a little more work.
          </span>
          <span className="note" role="status">
            {status}
          </span>
        </div>
        <button
          type="button"
          role="switch"
          className="switch"
          aria-labelledby="trace-on"
          aria-describedby="trace-on-note"
          aria-checked={recording}
          onClick={() => {
            turn(!recording);
          }}
        />
      </div>
      <div className="set-row">
        <div className="what">
          <label htmlFor="trace-length">Record for</label>
          <span className="note" id="trace-length-note">
            A trace left on stops by itself at the end of this time.
          </span>
        </div>
        <select
          id="trace-length"
          className="input"
          aria-describedby="trace-length-note"
          value={String(length)}
          onChange={(event) => {
            const picked = LENGTHS.find((each) => String(each.length) === event.target.value);
            if (picked === undefined) {
              return;
            }
            setLength(picked.length);
            keepLength(picked.length);
            if (recording) {
              save((current) => ({
                ...current,
                trace: { ...current.trace, until: traceEnd(picked.length, Date.now()) },
              }));
            }
          }}
        >
          {LENGTHS.map((each) => (
            <option key={each.length} value={String(each.length)}>
              {each.name}
            </option>
          ))}
        </select>
      </div>
      <div className="set-row">
        <fieldset className="trace-record">
          <legend>Record</legend>
          {TRACE_CATEGORIES.map((category) => (
            <div key={category} className="trace-category">
              <input
                id={`trace-${category}`}
                type="checkbox"
                checked={trace.record.includes(category)}
                aria-describedby={`trace-${category}-note`}
                onChange={(event) => {
                  const { checked } = event.target;
                  save((current) =>
                    !checked && current.trace.record.every((each) => each === category)
                      ? { problem: 'A trace must record at least one thing.' }
                      : { ...current, trace: withCategory(current.trace, category, checked) },
                  );
                }}
              />
              <span className="what">
                <label htmlFor={`trace-${category}`}>{CATEGORIES[category].name}</label>
                <span className="note" id={`trace-${category}-note`}>
                  {CATEGORIES[category].note}
                </span>
              </span>
            </div>
          ))}
        </fieldset>
      </div>
      {error === null ? null : (
        <p className="warn" role="alert">
          {error}
        </p>
      )}
      <NumberSetting
        id="trace-file-limit"
        label="Largest trace file"
        note="A trace continues in a new file once its file reaches this size."
        unit="MB"
        saved={trace.fileLimitMb}
        min={1}
        max={LIMITS.maxTraceFileMb}
        invalid={`A trace file must hold between 1 and ${LIMITS.maxTraceFileMb} MB.`}
        update={(current, fileLimitMb) => ({ ...current, trace: { ...current.trace, fileLimitMb } })}
      />
      <NumberSetting
        id="trace-keep"
        label="Trace files to keep"
        note="The oldest files are deleted once there are more."
        unit="files"
        saved={trace.keepFiles}
        min={1}
        max={LIMITS.maxTraceFiles}
        invalid={`Keep between 1 and ${LIMITS.maxTraceFiles} trace files.`}
        update={(current, keepFiles) => ({ ...current, trace: { ...current.trace, keepFiles } })}
      />
      <TraceFolder />
    </section>
  );
}
