import { describe, expect, test } from 'bun:test';

import type { RecordedReading } from '../src/lib/api.ts';
import { clock, entries, keptLabels, seen, startEntry, tileNote } from '../src/lib/review.ts';

const read = (text: string): RecordedReading => ({ firstMs: 0, seen: 1, text: { kind: 'read', text } });
const pending: RecordedReading = { firstMs: 0, seen: 1, text: { kind: 'pending' } };
const failed: RecordedReading = { firstMs: 0, seen: 1, text: { kind: 'failed', text: 'No model.' } };

describe('clock and seen', () => {
  test('show minutes and seconds, and how often', () => {
    expect([clock(0), clock(4_999), clock(65_000), clock(-5)]).toEqual(['0:00', '0:04', '1:05', '0:00']);
    expect([seen(1), seen(3)]).toEqual(['seen once', 'seen 3 times']);
  });
});

describe('review entries', () => {
  test('start from what was kept, else from what the reader read', () => {
    expect(startEntry(read('87%'), '88%')).toEqual({ keep: true, text: '88%' });
    expect(startEntry(read(' 87% '), undefined)).toEqual({ keep: true, text: '87%' });
    expect(startEntry(read(''), undefined)).toEqual({ keep: false, text: '' });
    expect(startEntry(pending, undefined)).toEqual({ keep: false, text: '' });
    expect(startEntry(failed, undefined)).toEqual({ keep: false, text: '' });
  });

  test("take the user's edits over the start", () => {
    const all = entries([read('1'), read('2')], new Map([[1, { keep: false, text: '2' }]]), new Map());
    expect(all).toEqual([
      { keep: true, text: '1' },
      { keep: false, text: '2' },
    ]);
  });

  test('keep only kept readings with text, trimmed', () => {
    const all = [
      { keep: true, text: ' 1 ' },
      { keep: false, text: '2' },
      { keep: true, text: ' ' },
      { keep: true, text: '4' },
    ];
    expect(keptLabels(all)).toEqual([
      { reading: 0, text: '1' },
      { reading: 3, text: '4' },
    ]);
  });
});

describe('tile notes', () => {
  test('say why a kept reading cannot be learned first', () => {
    expect(tileNote(read('9l%'), { keep: true, text: '9l%' }, 'Its l has another shape.')).toBe(
      'Its l has another shape.',
    );
    expect(tileNote(read('9l%'), { keep: false, text: '9l%' }, 'Its l has another shape.')).toBeNull();
  });

  test('ask for text the reader did not find', () => {
    expect(tileNote(read(''), { keep: true, text: '' }, undefined)).toBe('Type what it shows, or leave it out.');
    expect(tileNote(read(''), { keep: false, text: '' }, undefined)).toBe('No text found. Type it, or leave it out.');
    expect(tileNote(failed, { keep: false, text: '' }, undefined)).toBe('No model.');
    expect(tileNote(failed, { keep: false, text: '5' }, undefined)).toBeNull();
    expect(tileNote(pending, { keep: false, text: '' }, undefined)).toBe('Reading…');
    expect(tileNote(read('7'), { keep: true, text: '7' }, undefined)).toBeNull();
  });
});
