import { expect, test } from 'bun:test';

import type { CharactersSpec, SampleDef } from '../src/lib/bindings.ts';
import type { ReferenceSaving } from '../src/lib/check.ts';
import { unlearnedArrived, unlearnedAsk, unlearnedKey } from '../src/lib/useUnlearned.ts';

const percent = { app: 'example', id: 'percent' };
const red = { red: 255, green: 0, blue: 0 };
const region = { x: 0, y: 0, width: 10, height: 10 };
const samples: readonly SampleDef[] = [
  { image: 'ref-percent-1.png', text: '87%' },
  { image: 'ref-percent-2.png', text: '12%' },
];
const characters: CharactersSpec = {
  region,
  screenshot: 0,
  reference: percent,
  ink: red,
  tolerance: 25,
  labels: [{ screenshot: 0, region, text: '5' }],
  recorded: [],
  dropped: ['ref-percent-2.png'],
};
const taught: ReferenceSaving = { kind: 'taught', characters };

test('a saved reference asks about its pictures with the settings and the way it is saved', () => {
  expect(unlearnedAsk(null)).toBeNull();
  expect(unlearnedAsk({ kind: 'taught', characters: { ...characters, reference: null } })).toBeNull();
  expect(unlearnedAsk(taught)).toEqual({
    reference: percent,
    ink: red,
    tolerance: 25,
    dropped: ['ref-percent-2.png'],
    saving: 'taught',
  });
  const { ink: _ink, ...byContrast } = characters;
  expect(unlearnedAsk({ kind: 'taught', characters: byContrast })?.ink).toBeNull();
  expect(unlearnedAsk({ kind: 'settings', reference: percent, ink: null, tolerance: 40, dropped: [] })).toEqual({
    reference: percent,
    ink: null,
    tolerance: 40,
    dropped: [],
    saving: 'settings',
  });
});

test('an answer shows only while the editor asks under the key it was asked under', () => {
  const key = (saving: ReferenceSaving, saved = samples): string | null => unlearnedKey(unlearnedAsk(saving), saved);
  const asked = key(taught) ?? '';
  const unlearned = [{ image: 'ref-percent-1.png', text: '87%', problem: 'No pixel stands out.' }];
  const answer = { key: asked, samples: unlearned };
  expect(unlearnedKey(null, samples)).toBeNull();
  expect(unlearnedArrived(answer, asked)).toEqual(unlearned);
  // The boxes it adds are learned after its saved pictures, so they ask nothing new.
  expect(key({ kind: 'taught', characters: { ...characters, labels: [] } })).toBe(asked);
  // An answer about another tolerance, other pictures left out, or other saved pictures is dropped.
  const stale = [
    key({ kind: 'taught', characters: { ...characters, tolerance: 30 } }),
    key({ kind: 'taught', characters: { ...characters, dropped: [] } }),
    key({ kind: 'settings', reference: percent, ink: red, tolerance: 25, dropped: ['ref-percent-2.png'] }),
    key(taught, samples.slice(0, 1)),
    null,
  ];
  for (const wanted of stale) {
    expect(wanted).not.toBe(asked);
    expect(unlearnedArrived(answer, wanted)).toEqual([]);
  }
});
