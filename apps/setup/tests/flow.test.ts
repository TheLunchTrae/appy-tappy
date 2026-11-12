import { describe, expect, test } from 'bun:test';

import { afterApplication, openerWith } from '../src/lib/flow.ts';

interface ValueFlow {
  readonly kind: 'value';
  readonly app: string | null;
}

describe('the application flow opened for another flow', () => {
  const flow: ValueFlow = { kind: 'value', app: null };

  test('hands the application it saved to the flow that opened it', () => {
    expect(afterApplication(openerWith(flow, 'player'), { kind: 'saved', id: 'example' }, true)).toEqual({
      kind: 'flow',
      flow: { kind: 'value', app: 'example' },
    });
  });

  test('returns to the flow with what its picker had chosen when it is canceled', () => {
    expect(afterApplication(openerWith(flow, 'player'), { kind: 'canceled' }, true)).toEqual({
      kind: 'flow',
      flow: { kind: 'value', app: 'player' },
    });
    // Nothing chosen stays nothing chosen, whatever the flow started with.
    expect(
      afterApplication(openerWith<ValueFlow>({ ...flow, app: 'example' }, null), { kind: 'canceled' }, true),
    ).toEqual({
      kind: 'flow',
      flow: { kind: 'value', app: null },
    });
  });

  test('changes nothing when its save finishes after the user left it', () => {
    expect(afterApplication(flow, { kind: 'saved', id: 'example' }, false)).toEqual({ kind: 'nothing' });
    expect(afterApplication(null, { kind: 'saved', id: 'example' }, false)).toEqual({ kind: 'nothing' });
  });
});

describe('the application flow opened from a page', () => {
  test('shows the Applications page once it saved, and the page under it when canceled', () => {
    expect(afterApplication<ValueFlow>(null, { kind: 'saved', id: 'example' }, true)).toEqual({
      kind: 'applications',
    });
    expect(afterApplication<ValueFlow>(null, { kind: 'canceled' }, true)).toEqual({ kind: 'page' });
  });
});
