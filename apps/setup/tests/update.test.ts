import { describe, expect, test } from 'bun:test';

import { NO_UPDATE, type UpdateEvent, type UpdateState, nextUpdate } from '../src/lib/update.ts';

const offered: UpdateState = { kind: 'offered', version: '0.2.0', problem: null };
const downloading: UpdateState = { kind: 'downloading', version: '0.2.0' };
const ready: UpdateState = { kind: 'ready', version: '0.2.0' };
const asking: UpdateState = { kind: 'asking', version: '0.2.0' };
const installing: UpdateState = { kind: 'installing', version: '0.2.0' };

describe('nextUpdate', () => {
  const cases: readonly (readonly [string, UpdateState, UpdateEvent, UpdateState])[] = [
    ['a newer release offers it', NO_UPDATE, { kind: 'checked', version: '0.2.0' }, offered],
    ['no newer release shows nothing', NO_UPDATE, { kind: 'checked', version: null }, NO_UPDATE],
    ['update downloads', offered, { kind: 'update' }, downloading],
    ['a finished download waits to install', downloading, { kind: 'downloaded', version: '0.2.0' }, ready],
    [
      'a newer release downloaded since the offer waits under its own version',
      downloading,
      { kind: 'downloaded', version: '0.3.0' },
      { kind: 'ready', version: '0.3.0' },
    ],
    ['install asks first', ready, { kind: 'install' }, asking],
    ['keep goes back to the downloaded release', asking, { kind: 'keep' }, ready],
    ['confirm installs', asking, { kind: 'confirm' }, installing],
    [
      'a failed download offers it again with why',
      downloading,
      { kind: 'failed', problem: 'No network.' },
      { kind: 'offered', version: '0.2.0', problem: 'No network.' },
    ],
    [
      'a failed install offers it again with why',
      installing,
      { kind: 'failed', problem: 'The signature does not match.' },
      { kind: 'offered', version: '0.2.0', problem: 'The signature does not match.' },
    ],
    ['update during the download does nothing', downloading, { kind: 'update' }, downloading],
    ['a late check leaves the download alone', downloading, { kind: 'checked', version: '0.3.0' }, downloading],
    ['install before the download finishes does nothing', downloading, { kind: 'install' }, downloading],
    ['confirm without the question does nothing', ready, { kind: 'confirm' }, ready],
    ['keep without the question does nothing', ready, { kind: 'keep' }, ready],
    ['a failure while it waits to install does nothing', ready, { kind: 'failed', problem: 'x' }, ready],
    ['a download that was not asked for does nothing', offered, { kind: 'downloaded', version: '0.2.0' }, offered],
  ];
  for (const [name, state, event, expected] of cases) {
    test(name, () => {
      expect(nextUpdate(state, event)).toEqual(expected);
    });
  }
});
