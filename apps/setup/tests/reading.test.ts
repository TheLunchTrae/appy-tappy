import { describe, expect, test } from 'bun:test';

import type { ValueChoice } from '../src/lib/automation.ts';
import { liveAnnouncement, liveText, readingText } from '../src/lib/reading.ts';

describe('readingText', () => {
  test('shows numbers as read, flags as yes or no', () => {
    expect(readingText({ kind: 'number', value: 87 })).toBe('87');
    expect(readingText({ kind: 'number', value: 12.46 })).toBe('12.46');
    expect(readingText({ kind: 'number', value: 99.95 })).toBe('99.95');
    expect(readingText({ kind: 'number', value: 100 / 3 })).toBe('33.3333');
    expect(readingText({ kind: 'flag', value: true })).toBe('Yes');
    expect(readingText({ kind: 'flag', value: false }, true)).toBe('No');
    expect(readingText({ kind: 'state', value: 'Paused' })).toBe('Paused');
  });

  test('never rounds a number onto or across the target it is compared with', () => {
    const close = 99.999_996;
    expect(readingText({ kind: 'number', value: close })).toBe('100.0000');
    expect(readingText({ kind: 'number', value: close }, 100)).toBe(String(close));
    expect(readingText({ kind: 'number', value: close }, 99)).toBe('100.0000');
    expect(readingText({ kind: 'number', value: 33.333_333_7 }, 33.3333)).toBe('33.3333337');
    expect(readingText({ kind: 'number', value: 33.333_333_7 }, 33)).toBe('33.3333');
  });
});

describe('liveText', () => {
  const progress: ValueChoice = {
    app: 'example',
    appName: 'Example App',
    antiCheat: 'unknown',
    value: {
      id: 'progress',
      name: 'Progress',
      origin: { kind: 'byHand' },
      screen: {
        windowSize: { width: 100, height: 50 },
        region: { x: 0, y: 0, width: 10, height: 10 },
        check: { kind: 'number', reference: 'percent', whole: false },
        fixes: [],
      },
    },
    kind: 'number',
    states: [],
  };
  const when = { app: 'example', value: 'progress', is: 'atLeast' as const, target: 100 };
  const read = (holds: boolean | null) =>
    ({ kind: 'read', reading: { kind: 'number', value: 99.999_996 }, holds }) as const;

  test('says what the value reads and whether the condition is true', () => {
    expect(liveText(null, progress, when)).toBe('Looking for Example App…');
    expect(liveText({ kind: 'waiting' }, progress, when)).toBe('Waiting for Example App to show Progress…');
    expect(liveText(read(null), progress, null)).toBe('Progress reads 100.0000 now.');
    expect(liveText(read(false), progress, when)).toBe('Progress reads 99.999996 now, so the condition is not true.');
    expect(liveText({ ...read(true), reading: { kind: 'number', value: 100 } }, progress, when)).toBe(
      'Progress reads 100 now, so the condition is true.',
    );
    for (const kind of ['problem', 'lost', 'error'] as const) {
      expect(liveText({ kind, message: 'Resize the window.' }, progress, when)).toBe('Resize the window.');
    }
  });

  test('announces only the verdict or what is wrong', () => {
    expect(liveAnnouncement(null)).toBe('');
    expect(liveAnnouncement({ kind: 'waiting' })).toBe('');
    expect(liveAnnouncement(read(null))).toBe('');
    expect(liveAnnouncement(read(true))).toBe('The condition is true.');
    expect(liveAnnouncement(read(false))).toBe('The condition is not true.');
    expect(liveAnnouncement({ kind: 'lost', message: 'The window closed.' })).toBe('The window closed.');
  });
});
