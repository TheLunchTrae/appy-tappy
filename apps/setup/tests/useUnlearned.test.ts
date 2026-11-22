import { expect, test } from 'bun:test';

import type { CharactersSpec, SampleDef } from '../src/lib/bindings.ts';
import type { ReferenceSaving } from '../src/lib/check.ts';
import { cropArrived, unlearnedArrived, unlearnedAsk, unlearnedKey } from '../src/lib/useUnlearned.ts';

const percent = { app: 'example', id: 'percent' };
const region = { x: 0, y: 0, width: 10, height: 10 };
const samples: readonly SampleDef[] = [
  { image: 'ref-percent-1.png', text: '87%' },
  { image: 'ref-percent-2.png', text: '12%' },
];
const characters: CharactersSpec = {
  region,
  screenshot: 0,
  reference: percent,
  labels: [{ screenshot: 0, region, text: '5' }],
  recorded: [],
  dropped: ['ref-percent-2.png'],
};
const taught: ReferenceSaving = { kind: 'taught', characters };

test('a saved reference asks about its pictures without the ones it leaves out', () => {
  expect(unlearnedAsk(null)).toBeNull();
  expect(unlearnedAsk({ kind: 'taught', characters: { ...characters, reference: null } })).toBeNull();
  expect(unlearnedAsk(taught)).toEqual({ reference: percent, dropped: ['ref-percent-2.png'] });
  expect(unlearnedAsk({ kind: 'settings', reference: percent, dropped: [] })).toEqual({
    reference: percent,
    dropped: [],
  });
});

test('an answer shows only while the editor asks under the key it was asked under', () => {
  const key = (saving: ReferenceSaving, saved = samples): string | null => unlearnedKey(unlearnedAsk(saving), saved);
  const asked = key(taught) ?? '';
  const unlearned = [{ image: 'ref-percent-1.png', text: '87%', problem: 'No pixel stands out.', missing: false }];
  const answer = { key: asked, samples: unlearned };
  expect(unlearnedKey(null, samples)).toBeNull();
  expect(unlearnedArrived(answer, asked)).toEqual(unlearned);
  // The boxes it adds are learned after its saved pictures, so they ask nothing new.
  expect(key({ kind: 'taught', characters: { ...characters, labels: [] } })).toBe(asked);
  // An answer about other pictures left out or other saved pictures is dropped.
  const stale = [
    key({ kind: 'taught', characters: { ...characters, dropped: [] } }),
    key(taught, samples.slice(0, 1)),
    null,
  ];
  for (const wanted of stale) {
    expect(wanted).not.toBe(asked);
    expect(unlearnedArrived(answer, wanted)).toEqual([]);
  }
});

test('a picture fetched for settings since changed is dropped', () => {
  const picture = (width: number) => ({ width, height: 1, data: new Uint8ClampedArray(width * 4) });
  const first = picture(1);
  const second = picture(2);
  const none = { key: '', pictures: new Map<string, typeof first>() };
  const old = cropArrived(none, 'old', 'old', 'ref-percent-1.png', first);
  expect(old).toEqual({ key: 'old', pictures: new Map([['ref-percent-1.png', first]]) });
  // The editor asks under a new key, whose first picture replaces the old key's.
  const now = cropArrived(old, 'new', 'new', 'ref-percent-2.png', second);
  expect(now).toEqual({ key: 'new', pictures: new Map([['ref-percent-2.png', second]]) });
  // A picture of the old key that arrives late leaves the new key's pictures as they are.
  expect(cropArrived(now, 'old', 'new', 'ref-percent-1.png', first)).toBe(now);
  expect(cropArrived(now, 'old', null, 'ref-percent-1.png', first)).toBe(now);
  // The pictures of one key gather.
  expect(cropArrived(now, 'new', 'new', 'ref-percent-3.png', first).pictures.size).toBe(2);
});
