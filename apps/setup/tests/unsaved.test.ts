import { describe, expect, test } from 'bun:test';

import { EMPTY_DRAFT } from '../src/lib/check.ts';
import { held, onLeave, sameContent } from '../src/lib/unsaved.ts';

describe('sameContent', () => {
  test('compares what objects hold, not which objects they are', () => {
    expect(sameContent({ ...EMPTY_DRAFT, labels: new Map(), states: [] }, EMPTY_DRAFT)).toBe(true);
    expect(
      sameContent(
        { name: 'A', window: { title: 'T', mustMatch: { title: true } } },
        { name: 'A', window: { title: 'T', mustMatch: { title: true } } },
      ),
    ).toBe(true);
  });

  test('finds a change at any depth', () => {
    expect(sameContent(EMPTY_DRAFT, { ...EMPTY_DRAFT, labels: new Map([[0, '87%']]) })).toBe(false);
    expect(sameContent(new Map([[0, '87%']]), new Map([[1, '87%']]))).toBe(false);
    expect(sameContent(EMPTY_DRAFT, { ...EMPTY_DRAFT, region: { x: 0, y: 0, width: 4, height: 2 } })).toBe(false);
    expect(sameContent([{ key: 0 }], [{ key: 0 }, { key: 1 }])).toBe(false);
    expect(sameContent({ title: true }, { title: false })).toBe(false);
    expect(sameContent({ a: 1 }, { a: 1, b: undefined })).toBe(false);
  });

  test('tells kinds apart', () => {
    expect(sameContent([], {})).toBe(false);
    expect(sameContent(new Map(), {})).toBe(false);
    expect(sameContent(null, {})).toBe(false);
    expect(sameContent('1', 1)).toBe(false);
  });
});

describe('held', () => {
  test('holds work once what the flow shows differs from its start', () => {
    const start = { name: '', draft: EMPTY_DRAFT, screenshots: 0 };
    expect(held(false, start, { ...start, draft: { ...EMPTY_DRAFT } })).toBe('nothing');
    expect(held(false, start, { ...start, screenshots: 1 })).toBe('unsaved');
    expect(held(false, start, { ...start, name: 'Progress' })).toBe('unsaved');
  });

  test('a save under way comes first', () => {
    expect(held(true, 'a', 'a')).toBe('saving');
    expect(held(true, 'a', 'b')).toBe('saving');
  });
});

test('leaving a flow asks first only for unsaved work, and waits for a save', () => {
  expect(onLeave('nothing')).toBe('leave');
  expect(onLeave('unsaved')).toBe('ask');
  expect(onLeave('saving')).toBe('wait');
});
