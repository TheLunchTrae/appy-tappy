import { expect, test } from 'bun:test';

import type { Live } from '../src/lib/bindings.ts';
import { type LiveWatch, type LiveWatchEvent, NOT_WATCHING, RETRY_MS, liveWatchStep } from '../src/lib/liveWatch.ts';

const READ: Live = { kind: 'read', reading: { kind: 'number', value: 87 }, holds: null };

/** The watch after `events`, and everything the editor was told to show on the way. */
function after(events: readonly LiveWatchEvent[]) {
  let state: LiveWatch = NOT_WATCHING;
  const shown: unknown[] = [];
  for (const event of events) {
    const next = liveWatchStep(state, event);
    ({ state } = next);
    if (next.shows !== undefined) {
      shown.push(next.shows);
    }
  }
  return { state, shown };
}

test('a watch another capture replaced starts again at once and shows nothing', () => {
  const { state, shown } = after([
    { kind: 'started', watch: 1 },
    { kind: 'read', live: READ, now: 100 },
    { kind: 'refused', message: 'No value is watched.', now: 200 },
  ]);
  expect(state.watch).toBeNull();
  expect(state.retryAt).toBe(0);
  expect(shown).toEqual([READ]);
});

test('a reading after a replaced watch forgets the refusal', () => {
  const { state, shown } = after([
    { kind: 'started', watch: 1 },
    { kind: 'refused', message: 'No value is watched.', now: 200 },
    { kind: 'started', watch: 2 },
    { kind: 'read', live: READ, now: 300 },
    { kind: 'refused', message: 'No value is watched.', now: 400 },
  ]);
  expect(state).toEqual({ watch: null, retryAt: 0, refused: 'No value is watched.' });
  expect(shown).toEqual([READ]);
});

test('a second refusal in a row is a failure, which shows and waits', () => {
  const { state, shown } = after([
    { kind: 'started', watch: 1 },
    { kind: 'refused', message: 'No value is watched.', now: 200 },
    { kind: 'started', watch: 2 },
    { kind: 'refused', message: 'The capture stopped.', now: 250 },
  ]);
  expect(state).toEqual({ watch: null, retryAt: 250 + RETRY_MS, refused: 'The capture stopped.' });
  expect(shown).toEqual([{ kind: 'error', message: 'The capture stopped.' }]);
});

test('a lost watch and one that cannot start wait before the next', () => {
  const lost: Live = { kind: 'lost', message: 'The window closed.' };
  const closed = after([
    { kind: 'started', watch: 1 },
    { kind: 'read', live: lost, now: 500 },
  ]);
  expect(closed.state).toEqual({ watch: null, retryAt: 500 + RETRY_MS, refused: null });
  expect(closed.shown).toEqual([lost]);
  const unstarted = after([{ kind: 'unstarted', message: 'Progress is not saved.', now: 700 }]);
  expect(unstarted.state.retryAt).toBe(700 + RETRY_MS);
  expect(unstarted.shown).toEqual([{ kind: 'error', message: 'Progress is not saved.' }]);
});
