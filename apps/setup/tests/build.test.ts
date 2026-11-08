import { describe, expect, test } from 'bun:test';

import type { AppFile, BuildDef } from '../src/lib/api.ts';
import { buildState, checkedText } from '../src/lib/build.ts';

const checked: BuildDef = { version: '1.4.2.0', size: 1024, modified: 1_762_500_000 };

const file: AppFile = {
  format: 1,
  name: 'Example App',
  window: {
    title: 'Example App',
    class: 'ExampleWindow',
    executable: 'Example.exe',
    mustMatch: { title: false, class: false },
    fallback: 'none',
  },
  antiCheat: 'unknown',
  values: [],
  references: [],
  checked: { build: checked, at: 1_762_600_000 },
};

const { checked: _, ...unchecked } = file;
const { version: __, ...unversioned } = checked;

describe('buildState', () => {
  test('is null while the checked build runs or no build is known', () => {
    expect(buildState(file, { ...checked })).toBeNull();
    expect(buildState(file, null)).toBeNull();
    expect(buildState(unchecked, null)).toBeNull();
  });

  test('says when a build runs but none was recorded', () => {
    expect(buildState(unchecked, checked)).toEqual({ kind: 'unrecorded' });
  });

  test('names both versions when they differ, and otherwise that the file does', () => {
    const change = (running: BuildDef, of: AppFile = file) => buildState(of, running);
    expect(change({ ...checked, version: '1.5.0.0' })).toEqual({
      kind: 'changed',
      change: '1.4.2.0 then, 1.5.0.0 now',
    });
    expect(change({ ...checked, size: 2048 })).toEqual({ kind: 'changed', change: 'still 1.4.2.0, but another file' });
    expect(change({ ...checked, modified: 1_762_500_001 })).toEqual({
      kind: 'changed',
      change: 'still 1.4.2.0, but another file',
    });
    expect(change(unversioned)).toEqual({ kind: 'changed', change: '1.4.2.0 then, no version now' });
    const bare = { ...file, checked: { build: unversioned, at: 1_762_600_000 } };
    expect(change({ ...checked }, bare)).toEqual({ kind: 'changed', change: 'no version then, 1.4.2.0 now' });
    expect(change({ ...unversioned, size: 2048 }, bare)).toEqual({
      kind: 'changed',
      change: 'another file, with no version either time',
    });
  });
});

describe('checkedText', () => {
  test('names the checked version and day, or only the day for a build without a version', () => {
    const day = (at: Date) => at.toISOString().slice(0, 10);
    expect(checkedText(file, day)).toBe('checked against 1.4.2.0 on 2025-11-08');
    const bare = { ...file, checked: { build: unversioned, at: 1_762_600_000 } };
    expect(checkedText(bare, day)).toBe('checked on 2025-11-08');
    expect(checkedText(unchecked, day)).toBeNull();
  });
});
