import { describe, expect, test } from 'bun:test';

import { DEFAULT_SETTINGS, readMs, sameSettings, withWait } from '../src/lib/settings.ts';

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
    };
    expect(sameSettings(DEFAULT_SETTINGS, reordered)).toBe(true);
    expect(sameSettings(DEFAULT_SETTINGS, { ...DEFAULT_SETTINGS, format: 2 })).toBe(false);
    expect(sameSettings(DEFAULT_SETTINGS, { ...DEFAULT_SETTINGS, screenshotShortcut: 'F8' })).toBe(false);
    for (const which of ['fastestMs', 'slowestMs'] as const) {
      const readInterval = { ...DEFAULT_SETTINGS.readInterval, [which]: 1 };
      expect(sameSettings(DEFAULT_SETTINGS, { ...DEFAULT_SETTINGS, readInterval })).toBe(false);
    }
  });
});
