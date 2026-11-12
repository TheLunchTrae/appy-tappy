/** The Debug page's metrics and trace rows, in the words the user reads. */

import {
  type Chart,
  type ReadProgress,
  TRACE_HEADER,
  type TraceCategory,
  type TraceRange,
  type TraceRow,
} from './bindings.ts';
import { durationText, sizeText } from './debug.ts';

/** The ranges the metrics and rows cover, as the page names them. */
export const RANGES: readonly { readonly range: TraceRange; readonly name: string }[] = [
  { range: 'lastMinute', name: 'Last minute' },
  { range: 'lastTenMinutes', name: 'Last 10 minutes' },
  { range: 'whole', name: 'Whole trace' },
];

/** The kinds of row, named as the trace settings name what they record. */
export const CATEGORIES: readonly { readonly category: TraceCategory; readonly name: string }[] = [
  { category: 'readings', name: 'Each read' },
  { category: 'changes', name: 'Value changes' },
  { category: 'rules', name: 'Rules' },
  { category: 'actions', name: 'Actions' },
  { category: 'lifecycle', name: 'Applications and reloads' },
];

/** How many a second `count` over `spanNs` is, as `12.5`, or `null` over less than a second. */
export function perSecondText(count: number, spanNs: number): string | null {
  if (spanNs < 1e9) {
    return null;
  }
  const rate = count / (spanNs / 1e9);
  return rate >= 100 ? rate.toFixed(0) : rate.toFixed(1);
}

/** `count` when the trace records `category`, or `null` for a number it did not record. */
export function recordedCount(
  count: number,
  category: TraceCategory,
  recorded: readonly TraceCategory[],
): string | null {
  return recorded.includes(category) ? count.toLocaleString('en-US') : null;
}

/** What reading a trace file is up to, or `null` once all of it is read. */
export function progressText(progress: ReadProgress): string | null {
  if (progress.read >= progress.bytes) {
    return null;
  }
  return `Reading ${progress.file}: ${sizeText(progress.read)} of ${sizeText(progress.bytes)}`;
}

/** Whether `progress` has more of the file to read. */
export function isBehind(progress: ReadProgress): boolean {
  return progress.problem === null && progress.read < progress.bytes;
}

/** One bar of a {@link Chart}: where it starts and ends, and how much of the tallest bar it is. */
export interface Bar {
  readonly fromNs: number;
  readonly toNs: number;
  readonly count: number;
  /** From 0 to 1, against the tallest bar. */
  readonly height: number;
}

/** The bars of `chart`, with the times past its last bar as one more, so the chart says how many it left out. */
export function chartBars(chart: Chart): readonly Bar[] {
  const tallest = Math.max(1, chart.over, ...chart.counts);
  const bars = chart.counts.map((count, index) => ({
    fromNs: index * chart.barNs,
    toNs: (index + 1) * chart.barNs,
    count,
    height: count / tallest,
  }));
  if (chart.over === 0) {
    return bars;
  }
  const end = chart.counts.length * chart.barNs;
  return [...bars, { fromNs: end, toNs: Infinity, count: chart.over, height: chart.over / tallest }];
}

/** A bar's label, as `4 ms to 6 ms`, or `Over 40 ms` for the times past the last bar. */
export function barText(bar: Bar): string {
  if (bar.toNs === Infinity) {
    return `Over ${durationText(bar.fromNs)}`;
  }
  return `${durationText(bar.fromNs)} to ${durationText(bar.toNs)}`;
}

/** Where `ns` falls across a chart of `chart`, from 0 to 1, for the median and 95th percentile marks. */
export function chartPlace(chart: Chart, ns: number): number {
  const bars = chart.counts.length + (chart.over === 0 ? 0 : 1);
  return Math.min(1, ns / (bars * chart.barNs));
}

/** A field of a CSV file: quoted when it holds a comma, a quote or a line break. */
function csvField(text: string): string {
  return /[",\r\n]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text;
}

/** `rows` as a trace file writes them, under its header, to paste into a spreadsheet or an issue. */
export function rowsCsv(rows: readonly TraceRow[]): string {
  const lines = rows.map((row) =>
    [
      String(row.timeNs),
      row.thread,
      row.event,
      row.app,
      row.item,
      row.value,
      row.tookNs === null ? '' : String(row.tookNs),
      row.cause === null ? '' : String(row.cause),
      row.detail,
    ]
      .map(csvField)
      .join(','),
  );
  return [TRACE_HEADER, ...lines].map((line) => `${line}\n`).join('');
}

/** The lines of `text` that hold `search`, in any case, and how many lines it has. */
export function logLines(
  text: string,
  search: string,
): { readonly shown: string; readonly count: number; readonly total: number } {
  const lines = text.split('\n');
  if (lines.at(-1) === '') {
    lines.pop();
  }
  const needle = search.trim().toLowerCase();
  if (needle === '') {
    return { shown: text, count: lines.length, total: lines.length };
  }
  const found = lines.filter((line) => line.toLowerCase().includes(needle));
  return { shown: found.map((line) => `${line}\n`).join(''), count: found.length, total: lines.length };
}

/**
 * A number of milliseconds the user typed, as nanoseconds, or `null` for nothing or not a number. A number too large
 * to send exactly is held at the largest that is, which no row takes longer than.
 */
export function tookOverNs(typed: string): number | null {
  const ms = Number(typed.trim());
  return typed.trim() === '' || !Number.isFinite(ms) || ms < 0
    ? null
    : Math.min(Math.round(ms * 1e6), Number.MAX_SAFE_INTEGER);
}
