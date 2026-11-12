import { describe, expect, test } from 'bun:test';

import { resume } from '../src/lib/flow.ts';

describe('a flow opened for another', () => {
  const opener: { readonly kind: 'value'; readonly app: string | null } = { kind: 'value', app: null };

  test('hands the application it saved to the flow that opened it', () => {
    expect(resume(opener, 'example')).toEqual({ kind: 'value', app: 'example' });
  });

  test('returns to the flow as it was when it is canceled', () => {
    expect(resume({ ...opener, app: 'player' }, null)).toEqual({ kind: 'value', app: 'player' });
  });

  test('returns to the page when no flow opened it', () => {
    expect(resume(null, 'example')).toBeNull();
  });
});
