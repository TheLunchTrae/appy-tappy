import { expect, test } from 'bun:test';

import { keepsReading, learn } from '../src/lib/useRecorder.ts';

/** A view of a recording that records while `recording` says so. */
interface View {
  readonly recording: boolean;
}

const records = (view: View): boolean => view.recording;

test('a recording whose first read failed is read until a read answers, then while it records', () => {
  const nothing = learn<View>({ view: null, unread: false }, { kind: 'forgot' });
  expect(keepsReading(nothing, records)).toBe(false);
  // The start answered and the read after it failed, so nothing but the start was learned.
  const started = learn(nothing, { kind: 'started' });
  expect(keepsReading(started, records)).toBe(true);
  // A read that answers ends the wait, whether it shows the recording or none.
  const recording = learn(started, { kind: 'read', view: { recording: true } });
  expect(keepsReading(recording, records)).toBe(true);
  expect(keepsReading(learn(recording, { kind: 'read', view: { recording: false } }), records)).toBe(false);
  expect(keepsReading(learn(started, { kind: 'read', view: null }), records)).toBe(false);
  // Going on with a stopped recording reads it again too, and a discard forgets it.
  const more = learn(learn(recording, { kind: 'read', view: { recording: false } }), { kind: 'started' });
  expect(keepsReading(more, records)).toBe(true);
  expect(learn(more, { kind: 'forgot' })).toEqual({ view: null, unread: false });
});
