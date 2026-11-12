import { describe, expect, test } from 'bun:test';

import type { Chart, TraceRow } from '../src/lib/bindings.ts';
import {
  barText,
  chartBars,
  chartPlace,
  isBehind,
  logLines,
  perSecondText,
  progressText,
  recordedCount,
  rowsCsv,
  tookOverNs,
} from '../src/lib/trace.ts';

const MS = 1_000_000;

describe('perSecondText', () => {
  test('needs a second of trace, and keeps a decimal below 100', () => {
    expect(perSecondText(5, 0.5e9)).toBeNull();
    expect(perSecondText(25, 2e9)).toBe('12.5');
    expect(perSecondText(1000, 4e9)).toBe('250');
  });
});

describe('recordedCount', () => {
  test('is a dash for a kind of row the trace did not record', () => {
    expect(recordedCount(1200, 'changes', ['changes'])).toBe('1,200');
    expect(recordedCount(0, 'actions', ['readings'])).toBeNull();
  });
});

describe('progress', () => {
  const progress = { file: 'trace-a.csv', bytes: 2 * 1024 * 1024, read: 1024 * 1024, problem: null };
  test('says how much is read until all of it is', () => {
    expect(progressText(progress)).toBe('Reading trace-a.csv: 1.0 MB of 2.0 MB');
    expect(isBehind(progress)).toBe(true);
    expect(progressText({ ...progress, read: progress.bytes })).toBeNull();
    expect(isBehind({ ...progress, read: progress.bytes })).toBe(false);
  });
  test('stops reading a trace it cannot read', () => {
    expect(isBehind({ ...progress, problem: 'another version' })).toBe(false);
  });
});

describe('chartBars', () => {
  const chart: Chart = { barNs: 2 * MS, counts: [1, 4, 2], over: 0, samples: 7 };
  test('spans each bar and sizes it against the tallest', () => {
    const bars = chartBars(chart);
    expect(bars.map((bar) => [bar.fromNs, bar.toNs, bar.height])).toEqual([
      [0, 2 * MS, 0.25],
      [2 * MS, 4 * MS, 1],
      [4 * MS, 6 * MS, 0.5],
    ]);
    expect(barText(bars[1]!)).toBe('2.0 ms to 4.0 ms');
    expect(chartPlace(chart, 3 * MS)).toBe(0.5);
    expect(chartPlace(chart, 60 * MS)).toBe(1);
  });
  test('adds a bar for the times past the last one', () => {
    const over = chartBars({ ...chart, over: 8 });
    expect(over).toHaveLength(4);
    expect(barText(over[3]!)).toBe('Over 6.0 ms');
    expect(over[3]!.height).toBe(1);
    expect(chartPlace({ ...chart, over: 8 }, 4 * MS)).toBe(0.5);
  });
  test('draws an empty chart flat', () => {
    expect(chartBars({ barNs: MS, counts: [0, 0], over: 0, samples: 0 }).map((bar) => bar.height)).toEqual([0, 0]);
  });
});

describe('rowsCsv', () => {
  const row: TraceRow = {
    timeNs: 5,
    thread: 'reader',
    event: 'done',
    app: 'Example, App',
    item: 'say "hi"',
    value: '',
    tookNs: 7,
    cause: null,
    detail: 'ok',
  };
  test('writes rows as a trace file does, quoting what needs it', () => {
    expect(rowsCsv([row])).toBe(
      'time_ns,thread,event,app,item,value,took_ns,cause,detail\n5,reader,done,"Example, App","say ""hi""",,7,,ok\n',
    );
  });
});

describe('logLines', () => {
  const text = 'first line\nSecond line\nthird\n';
  test('keeps the lines that hold the search, in any case', () => {
    expect(logLines(text, ' LINE ')).toEqual({ shown: 'first line\nSecond line\n', count: 2, total: 3 });
    expect(logLines(text, '')).toEqual({ shown: text, count: 3, total: 3 });
    expect(logLines('', 'x')).toEqual({ shown: '', count: 0, total: 0 });
  });
});

describe('tookOverNs', () => {
  test('reads milliseconds, and nothing for what is not a time', () => {
    expect(tookOverNs(' 2.5 ')).toBe(2_500_000);
    expect(tookOverNs('')).toBeNull();
    expect(tookOverNs('soon')).toBeNull();
    expect(tookOverNs('-1')).toBeNull();
  });
});
