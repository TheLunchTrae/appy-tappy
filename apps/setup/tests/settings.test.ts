import { describe, expect, test } from 'bun:test';

import { DEFAULT_SETTINGS } from '../src/lib/bindings.ts';
import {
  readMs,
  readWhole,
  sameSettings,
  traceEnd,
  traceRecords,
  withCategory,
  withWait,
} from '../src/lib/settings.ts';

describe('readMs', () => {
  test('takes a whole number of milliseconds in range', () => {
    expect(readMs('16')).toBe(16);
    expect(readMs(' 500 ')).toBe(500);
    expect(readMs('1')).toBe(1);
    expect(readMs('10000')).toBe(10_000);
  });

  test('refuses anything else', () => {
    for (const text of ['', '0', '10001', '1.5', '-3', '16ms', 'abc']) {
      expect(readMs(text)).toBeNull();
    }
  });
});

describe('withWait', () => {
  const interval = { fastestMs: 16, slowestMs: 500 };

  test('sets one wait', () => {
    expect(withWait(interval, 'fastestMs', 40)).toEqual({ interval: { fastestMs: 40, slowestMs: 500 } });
    expect(withWait(interval, 'slowestMs', 16)).toEqual({ interval: { fastestMs: 16, slowestMs: 16 } });
  });

  test('keeps the fastest read no slower than the slowest', () => {
    const problem = { problem: 'The fastest read cannot wait longer than the slowest.' };
    expect(withWait(interval, 'fastestMs', 501)).toEqual(problem);
    expect(withWait(interval, 'slowestMs', 15)).toEqual(problem);
  });
});

describe('sameSettings', () => {
  test('compares each setting, whatever order the keys are in', () => {
    const reordered = {
      screenshotShortcut: DEFAULT_SETTINGS.screenshotShortcut,
      readInterval: {
        slowestMs: DEFAULT_SETTINGS.readInterval.slowestMs,
        fastestMs: DEFAULT_SETTINGS.readInterval.fastestMs,
      },
      format: DEFAULT_SETTINGS.format,
      trace: { ...DEFAULT_SETTINGS.trace, record: [...DEFAULT_SETTINGS.trace.record].reverse() },
      rememberEnabled: DEFAULT_SETTINGS.rememberEnabled,
      showDebug: DEFAULT_SETTINGS.showDebug,
    };
    expect(sameSettings(DEFAULT_SETTINGS, reordered)).toBe(true);
    expect(sameSettings(DEFAULT_SETTINGS, { ...DEFAULT_SETTINGS, format: 2 })).toBe(false);
    expect(sameSettings(DEFAULT_SETTINGS, { ...DEFAULT_SETTINGS, screenshotShortcut: 'F8' })).toBe(false);
    expect(sameSettings(DEFAULT_SETTINGS, { ...DEFAULT_SETTINGS, rememberEnabled: true })).toBe(false);
    expect(sameSettings(DEFAULT_SETTINGS, { ...DEFAULT_SETTINGS, showDebug: true })).toBe(false);
    for (const which of ['fastestMs', 'slowestMs'] as const) {
      const readInterval = { ...DEFAULT_SETTINGS.readInterval, [which]: 1 };
      expect(sameSettings(DEFAULT_SETTINGS, { ...DEFAULT_SETTINGS, readInterval })).toBe(false);
    }
    const trace = DEFAULT_SETTINGS.trace;
    for (const changed of [
      { ...trace, on: true },
      { ...trace, until: 5 },
      { ...trace, fileLimitMb: 51 },
      { ...trace, keepFiles: 6 },
      { ...trace, record: [...trace.record, 'readings' as const] },
      { ...trace, record: trace.record.slice(1) },
    ]) {
      expect(sameSettings(DEFAULT_SETTINGS, { ...DEFAULT_SETTINGS, trace: changed })).toBe(false);
    }
  });
});

describe('readWhole', () => {
  test('takes a whole number in range', () => {
    expect(readWhole(' 5 ', 1, 100)).toBe(5);
    expect(readWhole('1', 1, 100)).toBe(1);
    expect(readWhole('100', 1, 100)).toBe(100);
    for (const text of ['0', '101', '2.5', '', 'five']) {
      expect(readWhole(text, 1, 100)).toBeNull();
    }
  });
});

describe('the runtime trace', () => {
  const now = 1_762_612_800_000;
  const on = { ...DEFAULT_SETTINGS.trace, on: true };

  test('ends after the length picked, or never', () => {
    expect(traceEnd(10, now)).toBe(now + 600_000);
    expect(traceEnd(60, now)).toBe(now + 3_600_000);
    expect(traceEnd('off', now)).toBeNull();
  });

  test('records while on and before its end time', () => {
    expect(traceRecords(DEFAULT_SETTINGS.trace, now)).toBe(false);
    expect(traceRecords({ ...on, until: null }, now)).toBe(true);
    expect(traceRecords({ ...on, until: now + 1 }, now)).toBe(true);
    expect(traceRecords({ ...on, until: now }, now)).toBe(false);
  });

  test('keeps its categories in the page order', () => {
    expect(withCategory(on, 'readings', true).record).toEqual(['readings', 'changes', 'rules', 'actions', 'lifecycle']);
    expect(withCategory(on, 'rules', false).record).toEqual(['changes', 'actions', 'lifecycle']);
    expect(withCategory(on, 'rules', true).record).toEqual(on.record);
  });
});
