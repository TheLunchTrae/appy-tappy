import { describe, expect, test } from 'bun:test';

import { RUNTIME_STOPPED, runtimeWarning } from '../src/lib/runtime.ts';

describe('the background part', () => {
  test('is warned about only once it is known not to run', () => {
    expect(runtimeWarning(false)).toBe(RUNTIME_STOPPED);
    expect(runtimeWarning(true)).toBeNull();
    expect(runtimeWarning(null)).toBeNull();
  });
});
