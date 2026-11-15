import { expect, test } from 'bun:test';

import type { CharactersSpec } from '../src/lib/bindings.ts';
import { fitArrived, fitKey, fitSettles } from '../src/lib/useContrastTolerance.ts';

const spec: CharactersSpec = {
  region: { x: 0, y: 0, width: 10, height: 10 },
  screenshot: 0,
  reference: null,
  tolerance: 40,
  labels: [{ screenshot: 0, region: { x: 0, y: 0, width: 10, height: 10 }, text: '87%' }],
  recorded: [],
  dropped: [],
};

test('a reference asks for a fit by contrast alone, and anew when a sample or the tolerance changes', () => {
  expect(fitKey(null)).toBeNull();
  expect(fitKey({ ...spec, ink: { red: 255, green: 0, blue: 0 } })).toBeNull();
  expect(fitKey(spec)).toBe(fitKey({ ...spec }));
  expect(fitKey({ ...spec, tolerance: 60 })).not.toBe(fitKey(spec));
  expect(fitKey({ ...spec, labels: [] })).not.toBe(fitKey(spec));
});

test('a fit sets the tolerance only while the reference still asks for that fit', () => {
  const asked = fitKey(spec) ?? '';
  expect(fitArrived(asked, asked, 40, 60)).toBe(60);
  // The tolerance asked about reads every sample.
  expect(fitArrived(asked, asked, 40, 40)).toBeNull();
  // A sample changed since, which asked again under its own name.
  expect(fitArrived(asked, fitKey({ ...spec, labels: [] }), 40, 60)).toBeNull();
  // The reference took a text color since.
  expect(fitArrived(asked, null, 40, 60)).toBeNull();
});

test('a fit settles once it leaves the tolerance of the spec that still asks for it as it is', () => {
  const asked = fitKey(spec) ?? '';
  expect(fitSettles(asked, asked, 40, 40)).toBe(true);
  // The fit moves the tolerance, which asks again under the new one.
  expect(fitSettles(asked, asked, 40, 60)).toBe(false);
  // A sample changed since.
  expect(fitSettles(asked, fitKey({ ...spec, labels: [] }), 40, 40)).toBe(false);
});
