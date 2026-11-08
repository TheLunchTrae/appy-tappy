import { describe, expect, test } from 'bun:test';

import { NO_UPDATE, type UpdateEvent, type UpdateState, nextUpdate } from '../src/lib/update.ts';

const offered: UpdateState = { kind: 'offered', version: '0.2.0', problem: null };
const asking: UpdateState = { kind: 'asking', version: '0.2.0' };
const installing: UpdateState = { kind: 'installing', version: '0.2.0' };

describe('nextUpdate', () => {
  const cases: readonly (readonly [string, UpdateState, UpdateEvent, UpdateState])[] = [
    ['a newer release offers it', NO_UPDATE, { kind: 'checked', version: '0.2.0' }, offered],
    ['no newer release shows nothing', NO_UPDATE, { kind: 'checked', version: null }, NO_UPDATE],
    ['update asks first', offered, { kind: 'update' }, asking],
    ['keep goes back to the offer', asking, { kind: 'keep' }, offered],
    ['confirm downloads', asking, { kind: 'confirm' }, installing],
    [
      'a failed download offers it again with why',
      installing,
      { kind: 'failed', problem: 'No network.' },
      { kind: 'offered', version: '0.2.0', problem: 'No network.' },
    ],
    ['update during the download does nothing', installing, { kind: 'update' }, installing],
    ['a late check leaves the download alone', installing, { kind: 'checked', version: '0.3.0' }, installing],
    ['confirm without the question does nothing', offered, { kind: 'confirm' }, offered],
    ['a failure without a download does nothing', offered, { kind: 'failed', problem: 'x' }, offered],
  ];
  for (const [name, state, event, expected] of cases) {
    test(name, () => {
      expect(nextUpdate(state, event)).toEqual(expected);
    });
  }
});
