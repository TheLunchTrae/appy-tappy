import { useCallback, useEffect, useRef, useState } from 'react';

import type { DebugOverview, LogName, TraceRange, TraceSummary } from '../lib/bindings.ts';
import { debugOverview, errorText, openLogsFolder, readLog } from '../lib/api.ts';
import { durationText, elapsedText } from '../lib/debug.ts';
import { background, useRepeat } from '../lib/repeat.ts';
import { endText, traceRecords, useSettings } from '../lib/settings.ts';
import { logLines } from '../lib/trace.ts';
import { useNowUntil } from '../components/RuntimeTrace.tsx';
import { ExportBundle } from '../components/ExportBundle.tsx';
import { TraceMetricsTab, TracePicker, TraceRowsTab, useTraceFiles } from '../components/TraceViewer.tsx';

/** How often the page reads the runtime's state, the newest trace and the log again. */
const REFRESH_MS = 2000;

/** The page's tabs, in order. */
const TABS = [
  { tab: 'overview', name: 'Overview' },
  { tab: 'metrics', name: 'Metrics' },
  { tab: 'rows', name: 'Trace rows' },
  { tab: 'logs', name: 'Logs' },
] as const;

type Tab = (typeof TABS)[number]['tab'];

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

/** The end of `log`, read again every few seconds, with only the lines that hold `search`. */
function LogText({ log, search }: { readonly log: LogName; readonly search: string }) {
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
  const lines = logLines(text ?? '', search);
  useEffect(() => {
    const element = box.current;
    if (element !== null && atBottom.current) {
      element.scrollTop = element.scrollHeight;
    }
  }, [lines.shown]);
  return (
    <>
      {error === null ? null : (
        <p className="warn" role="alert">
          {error}
        </p>
      )}
      {text === null || search.trim() === '' ? null : (
        <p className="note" role="status">
          Showing {lines.count.toLocaleString('en-US')} of {lines.total.toLocaleString('en-US')} lines
        </p>
      )}
      <pre
        ref={box}
        className="logbox tall"
        role="log"
        onScroll={(event) => {
          const element = event.currentTarget;
          atBottom.current = element.scrollHeight - element.scrollTop - element.clientHeight < 4;
        }}
        aria-label={LOGS.find((each) => each.log === log)?.name}
      >
        {text === null ? '' : text === '' ? 'Nothing logged yet.' : lines.shown}
      </pre>
    </>
  );
}

/** The end of one log, picked with a switch, and a search of its lines. */
function Logs() {
  const [log, setLog] = useState<LogName>('runtime');
  const [search, setSearch] = useState('');
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
        <label className="field end">
          <span className="visually-hidden">Search the log</span>
          <input
            className="input"
            type="search"
            placeholder="Search"
            value={search}
            onChange={(event) => {
              setSearch(event.currentTarget.value);
            }}
          />
        </label>
      </div>
      {/* Keyed, so a switch reads the other log at once and a read still on its way for the old one is dropped. */}
      <LogText key={log} log={log} search={search} />
    </section>
  );
}

/** The trace file and range picker over the metrics or the rows of the picked file. */
function TraceTab({ tab }: { readonly tab: 'metrics' | 'rows' }) {
  const { trace } = useSettings().settings;
  const now = useNowUntil(trace.on ? trace.until : null);
  const { files, error, file, pick } = useTraceFiles(true);
  const [range, setRange] = useState<TraceRange>('lastTenMinutes');
  return (
    <section className="card" aria-labelledby="debug-trace">
      <h2 id="debug-trace">{tab === 'metrics' ? 'Metrics' : 'Trace rows'}</h2>
      {error === null ? null : (
        <p className="warn" role="alert">
          {error}
        </p>
      )}
      {files === null ? null : file === null ? (
        <p className="note">No trace yet. Record one under Runtime trace on the Settings page.</p>
      ) : (
        <>
          <TracePicker
            files={files}
            file={file}
            onPick={pick}
            range={range}
            onRange={setRange}
            recording={traceRecords(trace, now)}
          />
          {tab === 'metrics' ? (
            <TraceMetricsTab file={file} range={range} />
          ) : (
            <TraceRowsTab file={file} range={range} />
          )}
        </>
      )}
    </section>
  );
}

/**
 * The Debug page, shown while debug details are on: whether the runtime runs and the newest trace's timings, the
 * metrics and rows of a picked trace, both logs, and the debug bundle.
 */
export function DebugPage() {
  const [tab, setTab] = useState<Tab>('overview');
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
  useRepeat(refresh, REFRESH_MS, tab === 'overview');
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
      <div className="page-tabs" role="tablist" aria-label="Debug">
        {TABS.map((each) => (
          <button
            key={each.tab}
            type="button"
            role="tab"
            id={`debug-tab-${each.tab}`}
            aria-selected={tab === each.tab}
            aria-controls="debug-panel"
            className="tab"
            onClick={() => {
              setTab(each.tab);
            }}
          >
            {each.name}
          </button>
        ))}
      </div>
      {error === null ? null : (
        <p className="warn" role="alert">
          {error}
        </p>
      )}
      <div id="debug-panel" role="tabpanel" aria-labelledby={`debug-tab-${tab}`} className="tab-panel">
        {tab === 'overview' ? (
          <section className="card" aria-labelledby="debug-runtime">
            <h2 id="debug-runtime">Runtime</h2>
            <RuntimeLine overview={overview} />
            {overview === null ? null : <TraceCard trace={overview.trace} problem={overview.traceProblem} />}
          </section>
        ) : tab === 'logs' ? (
          <Logs />
        ) : (
          // Keyed, so each tab reads its own way from the start.
          <TraceTab key={tab} tab={tab} />
        )}
      </div>
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
