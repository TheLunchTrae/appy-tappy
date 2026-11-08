import { describe, expect, test } from 'bun:test';

import type { AppFile, BuildDef } from '../src/lib/api.ts';
import { buildChange } from '../src/lib/build.ts';

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

describe('buildChange', () => {
  test('is null while the checked build runs, nothing was checked, or no build is known', () => {
    expect(buildChange(file, { ...checked })).toBeNull();
    expect(buildChange(file, null)).toBeNull();
    const { checked: _, ...unchecked } = file;
    expect(buildChange(unchecked, { ...checked, size: 2048 })).toBeNull();
  });

  test('names both versions when they differ, and otherwise that the file does', () => {
    expect(buildChange(file, { ...checked, version: '1.5.0.0' })).toBe('1.4.2.0 then, 1.5.0.0 now');
    expect(buildChange(file, { ...checked, size: 2048 })).toBe('still 1.4.2.0, but another file');
    expect(buildChange(file, { ...checked, modified: 1_762_500_001 })).toBe('still 1.4.2.0, but another file');
    const { version: _, ...unversioned } = checked;
    expect(buildChange(file, unversioned)).toBe('1.4.2.0 then, no version now');
    const bare = { ...file, checked: { build: unversioned, at: 1_762_600_000 } };
    expect(buildChange(bare, { ...unversioned, size: 2048 })).toBe('another file, with no version either time');
  });
});
