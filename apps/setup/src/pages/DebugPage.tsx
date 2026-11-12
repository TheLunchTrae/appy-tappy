import { useCallback, useEffect, useRef, useState } from 'react';

import type { DebugOverview, LogName, TraceSummary } from '../lib/bindings.ts';
import { debugOverview, errorText, openLogsFolder, readLog } from '../lib/api.ts';
import { durationText, elapsedText } from '../lib/debug.ts';
import { background, useRepeat } from '../lib/repeat.ts';
import { endText, traceRecords, useSettings } from '../lib/settings.ts';
import { useNowUntil } from '../components/RuntimeTrace.tsx';
import { ExportBundle } from '../components/ExportBundle.tsx';

/** How often the page reads the runtime's state, the newest trace and the log again. */
const REFRESH_MS = 2000;

/** The two logs, as the page names them. */
const LOGS: readonly { readonly log: LogName; readonly name: string }[] = [
  { log: 'runtime', name: 'Runtime log' },
  { log: 'setup', name: 'Setup log' },
];

/** The runtime's state in a line: whether it runs, and whether it records a trace. */
function RuntimeLine({ overview }: { readonly overview: DebugOverview | null }) {
  const { trace } = useSettings().settings;
  const now = useNowUntil(trace.on ? trace.until : null);
  const running = overview?.runtime;
  const state = running === undefined ? 'Checking' : running === null ? 'Unknown' : running ? 'Running' : 'Not running';
  const recording = traceRecords(trace, now)
    ? trace.until === null
      ? 'trace recording'
      : `trace recording until ${endText(trace.until)}`
    : 'trace off';
  return (
    <div className="status-line" role="status">
      <span className={running === true ? 'dot running' : 'dot'} aria-hidden="true" />
      <span>{state}</span>
      <span className="note">· {recording}</span>
    </div>
  );
}

/** One number the overview shows, or a dash when the trace did not record what it needs. */
function Stat({ value, label }: { readonly value: string | null; readonly label: string }) {
  return (
    <div className="stat">
      <span className="stat-value">{value ?? '–'}</span>
      <span className="note">{label}</span>
    </div>
  );
}

/** The newest trace file's timings and its newest rows. */
function TraceCard({ trace, problem }: { readonly trace: TraceSummary | null; readonly problem: string | null }) {
  if (problem !== null) {
    return (
      <p className="warn" role="alert">
        {problem}
      </p>
    );
  }
  if (trace === null) {
    return <p className="note">No trace yet. Record one under Runtime trace on the Settings page.</p>;
  }
  return (
    <>
      <div className="stats">
        <Stat
          value={trace.frameToAction === null ? null : durationText(trace.frameToAction.medianNs)}
          label="Frame to action, median"
        />
        <Stat
          value={trace.frameToAction === null ? null : durationText(trace.frameToAction.p95Ns)}
          label="Frame to action, 95th percentile"
        />
        <Stat
          value={trace.ruleHandling === null ? null : durationText(trace.ruleHandling.medianNs)}
          label="Rule handling, median"
        />
        <Stat
          value={trace.ruleHandling === null ? null : durationText(trace.ruleHandling.p95Ns)}
          label="Rule handling, 95th percentile"
        />
        <Stat
          value={trace.actionRun === null ? null : durationText(trace.actionRun.medianNs)}
          label="Running actions, median"
        />
        <Stat
          value={trace.actionRun === null ? null : durationText(trace.actionRun.p95Ns)}
          label="Running actions, 95th percentile"
        />
        <Stat value={String(trace.dropped)} label="Rows dropped" />
      </div>
      <p className="note">
        From {trace.file}
        {trace.partial ? ', its newest rows only, as the file is large' : ''}. A dash is a number this trace did not
        record.
      </p>
      {trace.problem === null ? null : (
        <p className="warn" role="alert">
          {trace.problem}
        </p>
      )}
      {trace.newest.length === 0 ? null : (
        <table className="table mono">
          <caption className="note">Newest rows, with times since the trace started</caption>
          <thead>
            <tr>
              <th scope="col">Time</th>
              <th scope="col">Thread</th>
              <th scope="col">Event</th>
              <th scope="col">Application</th>
              <th scope="col">Item</th>
              <th scope="col">Value</th>
              <th scope="col">Took</th>
              <th scope="col">Detail</th>
            </tr>
          </thead>
          <tbody>
            {trace.newest.map((row, index) => (
              // Rows only ever change as a whole list, and two can share every field, so their place is their key.
              <tr key={index}>
                <td>{elapsedText(row.timeNs)}</td>
                <td>{row.thread}</td>
                <td>{row.event}</td>
                <td>{row.app}</td>
                <td>{row.item}</td>
                <td>{row.value}</td>
                <td>{row.tookNs === null ? '' : durationText(row.tookNs)}</td>
                <td>{row.detail}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </>
  );
}

/** The end of `log`, read again every few seconds. */
function LogText({ log }: { readonly log: LogName }) {
  const [text, setText] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const read = useCallback(async (): Promise<void> => {
    try {
      setText(await readLog(log));
      setError(null);
    } catch (failure: unknown) {
      setError(errorText(failure));
    }
  }, [log]);
  useRepeat(read, REFRESH_MS, true);
  const box = useRef<HTMLPreElement>(null);
  // The newest lines are at the bottom, so the box follows them while it shows the bottom, and stays put while the
  // user reads further up.
  const atBottom = useRef(true);
  useEffect(() => {
    const element = box.current;
    if (element !== null && atBottom.current) {
      element.scrollTop = element.scrollHeight;
    }
  }, [text]);
  return (
    <>
      {error === null ? null : (
        <p className="warn" role="alert">
          {error}
        </p>
      )}
      <pre
        ref={box}
        className="logbox"
        role="log"
        onScroll={(event) => {
          const element = event.currentTarget;
          atBottom.current = element.scrollHeight - element.scrollTop - element.clientHeight < 4;
        }}
        aria-label={LOGS.find((each) => each.log === log)?.name}
      >
        {text === null ? '' : text === '' ? 'Nothing logged yet.' : text}
      </pre>
    </>
  );
}

/** The end of one log, picked with a switch. */
function Logs() {
  const [log, setLog] = useState<LogName>('runtime');
  return (
    <section className="card" aria-labelledby="debug-logs">
      <div className="card-head">
        <h2 id="debug-logs">Logs</h2>
        <div className="seg" role="group" aria-label="Which log">
          {LOGS.map((each) => (
            <button
              key={each.log}
              type="button"
              aria-pressed={log === each.log}
              onClick={() => {
                setLog(each.log);
              }}
            >
              {each.name}
            </button>
          ))}
        </div>
      </div>
      {/* Keyed, so a switch reads the other log at once and a read still on its way for the old one is dropped. */}
      <LogText key={log} log={log} />
    </section>
  );
}

/**
 * The Debug page, shown while debug details are on: whether the runtime runs, the newest trace's timings and rows,
 * both logs, and the debug bundle.
 */
export function DebugPage() {
  const [overview, setOverview] = useState<DebugOverview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);
  const refresh = useCallback(async (): Promise<void> => {
    try {
      setOverview(await debugOverview());
      setError(null);
    } catch (failure: unknown) {
      setError(errorText(failure));
    }
  }, []);
  useRepeat(refresh, REFRESH_MS, true);
  const openFolder = async (): Promise<void> => {
    try {
      await openLogsFolder();
      setError(null);
    } catch (failure: unknown) {
      setError(errorText(failure));
    }
  };
  return (
    <main className="page">
      <div className="page-head">
        <h1>Debug</h1>
        <span className="lead">What AppyTappy and AppyTappy Setup see and do.</span>
        <div className="end">
          <button
            type="button"
            className="btn"
            onClick={() => {
              background(openFolder());
            }}
          >
            Open logs folder
          </button>
          <button
            type="button"
            className="btn primary"
            onClick={() => {
              setExporting(true);
            }}
          >
            Export debug bundle
          </button>
        </div>
      </div>
      {error === null ? null : (
        <p className="warn" role="alert">
          {error}
        </p>
      )}
      <section className="card" aria-labelledby="debug-runtime">
        <h2 id="debug-runtime">Runtime</h2>
        <RuntimeLine overview={overview} />
        {overview === null ? null : <TraceCard trace={overview.trace} problem={overview.traceProblem} />}
      </section>
      <Logs />
      {exporting ? (
        <ExportBundle
          trace={overview?.trace ?? null}
          onClose={() => {
            setExporting(false);
          }}
        />
      ) : null}
    </main>
  );
}
