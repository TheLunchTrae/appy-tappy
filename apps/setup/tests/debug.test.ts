import { describe, expect, test } from 'bun:test';

import { bundleName, durationText, elapsedText, sizeText } from '../src/lib/debug.ts';

describe('durationText', () => {
  test('keeps two decimals below a millisecond, one below 100 ms, then whole ms, then seconds', () => {
    expect(durationText(50_000)).toBe('0.05 ms');
    expect(durationText(9_400_000)).toBe('9.4 ms');
    expect(durationText(120_400_000)).toBe('120 ms');
    expect(durationText(12_345_000_000)).toBe('12.3 s');
  });
});

describe('elapsedText', () => {
  test('counts minutes, and hours once there are any', () => {
    expect(elapsedText(7_204_000_000)).toBe('0:07.204');
    expect(elapsedText(67_204_999_999)).toBe('1:07.204');
    expect(elapsedText(3_723_004_000_000)).toBe('1:02:03.004');
  });
});

describe('sizeText', () => {
  test('uses bytes, KB or MB', () => {
    expect(sizeText(800)).toBe('800 bytes');
    expect(sizeText(12 * 1024)).toBe('12 KB');
    expect(sizeText(4.2 * 1024 * 1024)).toBe('4.2 MB');
  });
});

describe('bundleName', () => {
  test('names the bundle for the local day and time', () => {
    expect(bundleName(new Date(2026, 10, 2, 9, 5))).toBe('appytappy-debug-2026-11-02-0905.zip');
  });
});
