import { describe, expect, test } from 'bun:test';

import { RUNTIME_STOPPED, runtimeAfter, runtimeWarning } from '../src/lib/runtime.ts';

describe('the background part', () => {
  test('runs as the newest read says, and is not known after a read that failed', () => {
    expect(runtimeAfter({ status: 'fulfilled', value: true })).toBe(true);
    expect(runtimeAfter({ status: 'fulfilled', value: false })).toBe(false);
    expect(runtimeAfter({ status: 'fulfilled', value: null })).toBeNull();
    expect(runtimeAfter({ status: 'rejected', reason: new Error('AppyTappy Setup is closing.') })).toBeNull();
  });

  test('is warned about only once it is known not to run', () => {
    expect(runtimeWarning(false)).toBe(RUNTIME_STOPPED);
    expect(runtimeWarning(true)).toBeNull();
    expect(runtimeWarning(null)).toBeNull();
  });
});
