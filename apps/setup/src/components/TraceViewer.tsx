import { useCallback, useEffect, useMemo, useState } from 'react';

import {
  type Chart,
  type ReadProgress,
  type RowFilter,
  type Spread,
  type TraceCategory,
  type TraceFile,
  type TraceMetrics,
  type TraceRange,
  TRACE_SAMPLES,
  type TraceRows,
} from '../lib/bindings.ts';
import { closeTrace, errorText, traceFiles, traceMetrics, traceRows } from '../lib/api.ts';
import { durationText, elapsedText, sizeText } from '../lib/debug.ts';
import { background, useNewest, useOnChange, useRepeat } from '../lib/repeat.ts';
import {
  CATEGORIES,
  RANGES,
  barText,
  chartBars,
  chartPlace,
  isBehind,
  perSecondText,
  progressText,
  recordedCount,
  rowsCsv,
  tookOverNs,
} from '../lib/trace.ts';

/** How often the metrics and rows read the chosen trace on. */
const REFRESH_MS = 1000;

/** How many reads in a row one refresh makes at most while the file is still being read. */
const CATCH_UP = 8;

/**
 * Reads `read` again at once while it says more of the file is left, so a large file is read in one go. Stops, and
 * gives `null`, once `current` says a newer read started or the page left.
 */
async function readAll<T extends { readonly progress: ReadProgress }>(
  read: () => Promise<T>,
  current: () => boolean,
): Promise<T | null> {
  let answer = await read();
  for (let round = 1; round < CATCH_UP && current() && isBehind(answer.progress); round += 1) {
    answer = await read();
  }
  return current() ? answer : null;
}

/** The trace files, read again every few seconds while `active`, and the one picked, the newest at first. */
export function useTraceFiles(active: boolean): {
  readonly files: readonly TraceFile[] | null;
  readonly error: string | null;
  readonly file: string | null;
  readonly pick: (file: string) => void;
} {
  const [files, setFiles] = useState<readonly TraceFile[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [picked, setPicked] = useState<string | null>(null);
  const list = useCallback(async (): Promise<void> => {
    try {
      setFiles(await traceFiles());
      setError(null);
    } catch (failure: unknown) {
      setError(errorText(failure));
    }
  }, []);
  useRepeat(list, 3 * REFRESH_MS, active);
  // The page lets go of the file it read as it leaves its trace tabs, so a trace it showed is not held.
  useEffect(
    () => () => {
      background(closeTrace());
    },
    [],
  );
  const file =
    picked !== null && files?.some((each) => each.name === picked) === true ? picked : (files?.[0]?.name ?? null);
  return { files, error, file, pick: setPicked };
}

/** The picker of the trace file and the range, shared by the metrics and the rows. */
export function TracePicker({
  files,
  file,
  onPick,
  range,
  onRange,
  recording,
}: {
  readonly files: readonly TraceFile[];
  readonly file: string;
  readonly onPick: (file: string) => void;
  readonly range: TraceRange;
  readonly onRange: (range: TraceRange) => void;
  /** Whether the runtime records into the newest file now. */
  readonly recording: boolean;
}) {
  return (
    <div className="trace-picker">
      <label className="field">
        <span>Trace file</span>
        <select
          className="input"
          value={file}
          onChange={(event) => {
            onPick(event.currentTarget.value);
          }}
        >
          {files.map((each, index) => (
            <option key={each.name} value={each.name}>
              {each.name} · {sizeText(each.bytes)}
              {index === 0 && recording ? ' · recording' : ''}
            </option>
          ))}
        </select>
      </label>
      <div className="seg" role="group" aria-label="Which rows">
        {RANGES.map((each) => (
          <button
            key={each.range}
            type="button"
            aria-pressed={range === each.range}
            onClick={() => {
              onRange(each.range);
            }}
          >
            {each.name}
          </button>
        ))}
      </div>
    </div>
  );
}

/** One number, or a dash when the trace did not record what it needs. */
function Tile({ value, label }: { readonly value: string | null; readonly label: string }) {
  return (
    <div className="stat">
      <span className="stat-value">{value ?? '–'}</span>
      <span className="note">{label}</span>
    </div>
  );
}

const median = (spread: Spread | null): string | null => (spread === null ? null : durationText(spread.medianNs));
const p95 = (spread: Spread | null): string | null => (spread === null ? null : durationText(spread.p95Ns));

/** How the frame to action times spread, with the median and the 95th percentile marked. */
function FrameChart({ chart, spread }: { readonly chart: Chart; readonly spread: Spread }) {
  const bars = chartBars(chart);
  return (
    <figure className="chart">
      <figcaption className="note">
        Frame to action, {chart.samples.toLocaleString('en-US')} times. Each bar is {durationText(chart.barNs)} wide.
      </figcaption>
      <div
        className="chart-bars"
        role="img"
        aria-label={`Median ${durationText(spread.medianNs)}, 95th percentile ${durationText(spread.p95Ns)}${
          chart.over === 0 ? '' : `, ${chart.over.toLocaleString('en-US')} times past the last bar`
        }.`}
      >
        {bars.map((bar) => (
          <div
            key={bar.fromNs}
            aria-hidden="true"
            className={bar.toNs === Infinity ? 'chart-bar over' : 'chart-bar'}
            style={{ height: `${Math.max(bar.count === 0 ? 0 : 2, bar.height * 100)}%` }}
            title={`${barText(bar)}: ${bar.count.toLocaleString('en-US')}`}
          />
        ))}
        <span
          className="chart-mark"
          aria-hidden="true"
          style={{ left: `${chartPlace(chart, spread.medianNs) * 100}%` }}
        >
          Median
        </span>
        <span
          className="chart-mark p95"
          aria-hidden="true"
          style={{ left: `${chartPlace(chart, spread.p95Ns) * 100}%` }}
        >
          95th
        </span>
      </div>
      <div className="chart-axis note">
        <span>0 ms</span>
        <span>
          {durationText(chart.counts.length * chart.barNs)}
          {chart.over === 0 ? '' : ' and over'}
        </span>
      </div>
    </figure>
  );
}

/** The metrics of `metrics`: the tiles, the chart, and each value and automation. */
function MetricsView({ metrics }: { readonly metrics: TraceMetrics }) {
  const { recorded } = metrics;
  // Reads and check times come from the reading rows, so without them those columns would only hold zeros.
  const reads = recorded.includes('readings');
  return (
    <>
      <div className="stats">
        <Tile value={median(metrics.frameToAction)} label="Frame to action, median" />
        <Tile value={p95(metrics.frameToAction)} label="Frame to action, 95th percentile" />
        <Tile value={median(metrics.actionRun)} label="Running actions, median" />
        <Tile value={p95(metrics.ruleHandling)} label="Rule handling, 95th percentile" />
        <Tile value={reads ? perSecondText(metrics.reads, metrics.spanNs) : null} label="Reads a second" />
        <Tile value={recordedCount(metrics.changes, 'changes', recorded)} label="Value changes" />
        <Tile value={metrics.dropped.toLocaleString('en-US')} label="Rows dropped" />
        <Tile value={recordedCount(metrics.failed, 'actions', recorded)} label="Actions that failed" />
      </div>
      <p className="note">
        These cover {elapsedText(metrics.spanNs)} of the trace. A dash means the trace has no rows of that kind, as when
        it does not record them.
        {metrics.sampled ? ` Times come from the newest ${TRACE_SAMPLES.toLocaleString('en-US')} of each kind.` : ''}
      </p>
      {metrics.chart === null || metrics.frameToAction === null ? null : (
        <FrameChart chart={metrics.chart} spread={metrics.frameToAction} />
      )}
      {metrics.values.length === 0 ? null : (
        <table className="table mono">
          <caption className="note">Values</caption>
          <thead>
            <tr>
              <th scope="col">Application</th>
              <th scope="col">Value</th>
              {reads ? (
                <>
                  <th scope="col">Reads</th>
                  <th scope="col">Reads a second</th>
                </>
              ) : null}
              <th scope="col">Changes</th>
              <th scope="col">Last change</th>
              {reads ? (
                <>
                  <th scope="col">Check, median</th>
                  <th scope="col">Check, 95th</th>
                </>
              ) : null}
            </tr>
          </thead>
          <tbody>
            {metrics.values.map((value) => (
              <tr key={`${value.app}\n${value.name}`}>
                <td>{value.app}</td>
                <td>{value.name}</td>
                {reads ? (
                  <>
                    <td>{value.reads.toLocaleString('en-US')}</td>
                    <td>{perSecondText(value.reads, metrics.spanNs) ?? ''}</td>
                  </>
                ) : null}
                <td>{value.changes.toLocaleString('en-US')}</td>
                <td>{value.lastChangeNs === null ? '' : elapsedText(value.lastChangeNs)}</td>
                {reads ? (
                  <>
                    <td>{median(value.check) ?? ''}</td>
                    <td>{p95(value.check) ?? ''}</td>
                  </>
                ) : null}
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {metrics.automations.length === 0 ? null : (
        <table className="table mono">
          <caption className="note">Automations</caption>
          <thead>
            <tr>
              <th scope="col">Application</th>
              <th scope="col">Automation</th>
              <th scope="col">Ran</th>
              <th scope="col">Held by cooldown</th>
              <th scope="col">Failed</th>
              <th scope="col">Frame to action, median</th>
              <th scope="col">Frame to action, 95th</th>
            </tr>
          </thead>
          <tbody>
            {metrics.automations.map((automation) => (
              <tr key={`${automation.app}\n${automation.name}`}>
                <td>{automation.app}</td>
                <td>{automation.name}</td>
                <td>{automation.ran.toLocaleString('en-US')}</td>
                <td>{automation.held.toLocaleString('en-US')}</td>
                <td>{automation.failed.toLocaleString('en-US')}</td>
                <td>{median(automation.frameToAction) ?? ''}</td>
                <td>{p95(automation.frameToAction) ?? ''}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </>
  );
}

/** What reading the file is up to, and why it cannot be read, if it cannot. */
function Progress({ progress }: { readonly progress: ReadProgress }) {
  if (progress.problem !== null) {
    return (
      <p className="warn" role="alert">
        {progress.problem}
      </p>
    );
  }
  const reading = progressText(progress);
  // Not announced: it changes every second while a large file is read.
  return (
    <>
      {reading === null ? null : <p className="note">{reading}</p>}
      {progress.skipped ? (
        <p className="warn">Some rows of this trace could not be read, so these numbers leave them out.</p>
      ) : null}
    </>
  );
}

/** The metrics of the trace file `file` over `range`, read on every second. */
export function TraceMetricsTab({ file, range }: { readonly file: string; readonly range: TraceRange }) {
  const [metrics, setMetrics] = useState<TraceMetrics | null>(null);
  const [error, setError] = useState<string | null>(null);
  const begin = useNewest();
  const read = useCallback(async (): Promise<void> => {
    const current = begin();
    try {
      const answer = await readAll(() => traceMetrics(file, range), current);
      if (answer !== null) {
        setMetrics(answer);
        setError(null);
      }
    } catch (failure: unknown) {
      if (current()) {
        setError(errorText(failure));
      }
    }
  }, [begin, file, range]);
  // Another file or range reads at once.
  useOnChange(read);
  useRepeat(read, REFRESH_MS, true);
  return (
    <>
      {error === null ? null : (
        <p className="warn" role="alert">
          {error}
        </p>
      )}
      {metrics === null ? null : (
        <>
          <Progress progress={metrics.progress} />
          {metrics.recorded.length === 0 ? (
            <p className="note">This trace has no rows yet.</p>
          ) : (
            <MetricsView metrics={metrics} />
          )}
        </>
      )}
    </>
  );
}

/** Every kind of row, as the rows start. */
const EVERY_CATEGORY: readonly TraceCategory[] = CATEGORIES.map((each) => each.category);

/** The trace file `file`'s rows over `range`, filtered as the user picks. */
export function TraceRowsTab({ file, range }: { readonly file: string; readonly range: TraceRange }) {
  const [categories, setCategories] = useState<readonly TraceCategory[]>(EVERY_CATEGORY);
  const [app, setApp] = useState<string | null>(null);
  const [took, setTook] = useState('');
  const [search, setSearch] = useState('');
  const [follow, setFollow] = useState(true);
  const [cause, setCause] = useState<number | null>(null);
  const [rows, setRows] = useState<TraceRows | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  const filter = useMemo<RowFilter>(
    () => ({ range, categories: [...categories], app, tookOverNs: tookOverNs(took), search }),
    [range, categories, app, took, search],
  );
  const begin = useNewest();
  const read = useCallback(async (): Promise<void> => {
    const current = begin();
    try {
      const answer = await readAll(() => traceRows(file, filter), current);
      if (answer !== null) {
        setRows(answer);
        setError(null);
      }
    } catch (failure: unknown) {
      if (current()) {
        setError(errorText(failure));
      }
    }
  }, [begin, file, filter]);
  // A changed file or filter reads at once, even while the rows do not follow the newest.
  useOnChange(read);
  useRepeat(read, REFRESH_MS, follow);
  const copy = async (): Promise<void> => {
    if (rows === null) {
      return;
    }
    try {
      await navigator.clipboard.writeText(rowsCsv(rows.rows));
      setCopied(`Copied ${rows.rows.length.toLocaleString('en-US')} rows.`);
    } catch (failure: unknown) {
      setCopied(`AppyTappy Setup cannot copy the rows: ${errorText(failure)}`);
    }
  };
  return (
    <>
      <div className="row-filters">
        <div className="seg kinds" role="group" aria-label="Kinds of row">
          {CATEGORIES.map((each) => (
            <button
              key={each.category}
              type="button"
              aria-pressed={categories.includes(each.category)}
              onClick={() => {
                setCategories((current) =>
                  current.includes(each.category)
                    ? current.filter((category) => category !== each.category)
                    : EVERY_CATEGORY.filter((category) => category === each.category || current.includes(category)),
                );
              }}
            >
              {each.name}
            </button>
          ))}
        </div>
        <label className="field">
          <span>Application</span>
          <select
            className="input"
            value={app ?? ''}
            onChange={(event) => {
              const picked = event.currentTarget.value;
              setApp(picked === '' ? null : picked);
            }}
          >
            <option value="">Every application</option>
            {/* One picked before is kept though the rows no longer name it, so the select still shows why none match. */}
            {[...(rows?.apps ?? []), ...(app === null || rows?.apps.includes(app) === true ? [] : [app])].map(
              (each) => (
                <option key={each} value={each}>
                  {each}
                </option>
              ),
            )}
          </select>
        </label>
        <label className="field narrow">
          <span>Took over (ms)</span>
          <input
            className="input"
            inputMode="decimal"
            value={took}
            onChange={(event) => {
              setTook(event.currentTarget.value);
            }}
          />
        </label>
        <label className="field">
          <span>Search</span>
          <input
            className="input"
            type="search"
            value={search}
            onChange={(event) => {
              setSearch(event.currentTarget.value);
            }}
          />
        </label>
      </div>
      <div className="trace-tools">
        <label className="toggle">
          <input
            type="checkbox"
            checked={follow}
            onChange={(event) => {
              setFollow(event.currentTarget.checked);
            }}
          />
          <span>Follow newest</span>
        </label>
        <button
          type="button"
          className="btn small"
          disabled={rows === null || rows.rows.length === 0}
          onClick={() => {
            background(copy());
          }}
        >
          Copy rows as CSV
        </button>
        {copied === null ? null : (
          <span className="note" role="status">
            {copied}
          </span>
        )}
      </div>
      {error === null ? null : (
        <p className="warn" role="alert">
          {error}
        </p>
      )}
      {rows === null ? null : (
        <>
          <Progress progress={rows.progress} />
          <p className="note">
            Showing {rows.rows.length.toLocaleString('en-US')} of {rows.matching.toLocaleString('en-US')} matching rows
            {rows.held < rows.total
              ? `, from the newest ${rows.held.toLocaleString('en-US')} of ${rows.total.toLocaleString('en-US')}`
              : ''}
            . Times are since the trace started. Pick a frame to mark the rows that came from it.
          </p>
          {rows.rows.length === 0 ? null : (
            <table className="table mono trace-rows">
              <thead>
                <tr>
                  <th scope="col">Time</th>
                  <th scope="col">Thread</th>
                  <th scope="col">Event</th>
                  <th scope="col">Application</th>
                  <th scope="col">Item</th>
                  <th scope="col">Value</th>
                  <th scope="col">Took</th>
                  <th scope="col">Frame</th>
                  <th scope="col">Detail</th>
                </tr>
              </thead>
              <tbody>
                {rows.rows.map((row, index) => (
                  // Rows only ever change as a whole list, and two can share every field, so their place is their key.
                  <tr key={index} className={cause !== null && row.cause === cause ? 'same-frame' : undefined}>
                    <td>{elapsedText(row.timeNs)}</td>
                    <td>{row.thread}</td>
                    <td>{row.event}</td>
                    <td>{row.app}</td>
                    <td>{row.item}</td>
                    <td>{row.value}</td>
                    <td>{row.tookNs === null ? '' : durationText(row.tookNs)}</td>
                    <td>
                      {row.cause === null ? null : (
                        <button
                          type="button"
                          className="frame-link"
                          aria-pressed={row.cause === cause}
                          title="Mark the rows from this frame"
                          onClick={() => {
                            setCause(row.cause === cause ? null : row.cause);
                          }}
                        >
                          {elapsedText(row.cause)}
                        </button>
                      )}
                    </td>
                    <td>{row.detail}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </>
      )}
    </>
  );
}
