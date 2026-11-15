import { expect, test } from 'bun:test';

import type { CharactersSpec } from '../src/lib/bindings.ts';
import { lookalikeKey, lookalikeWarning, neededPairs, pairsArrived } from '../src/lib/useLookalikes.ts';

const pairs = [
  ['8', '6'],
  ['0', '8'],
  ['O', '0'],
] as const;

test('a value is warned about the pairs with a character it reads, and every pair when it needs no list', () => {
  expect(neededPairs(pairs, ['0', '1', '6', '8'])).toEqual(pairs);
  // A shown O it does not need reads as the 0 it does.
  expect(neededPairs(pairs, ['0'])).toEqual([
    ['0', '8'],
    ['O', '0'],
  ]);
  expect(neededPairs(pairs, ['1', '%'])).toEqual([]);
  expect(neededPairs(pairs, [])).toEqual(pairs);
});

test('the warning names the pairs and the fix for how the reference finds its characters', () => {
  expect(lookalikeWarning([], null, 40)).toBeNull();
  expect(lookalikeWarning([['8', '6']], null, 40)).toBe(
    '8 and 6 look alike in what it learned, so it may read one as the other. Text with a thick outline often does this. Pick a text color below, so only the inside of the characters counts.',
  );
  const white = { red: 255, green: 255, blue: 255 };
  expect(lookalikeWarning(pairs.slice(0, 2), white, 40)).toBe(
    'Some characters look alike in what it learned, so it may read one of each pair as the other: 8 and 6; 0 and 8. Try a lower tolerance, so less of the edge around the characters counts.',
  );
  // No tolerance is lower than none.
  expect(lookalikeWarning([['8', '6']], white, 0)).toBe(
    '8 and 6 look alike in what it learned, so it may read one as the other. Pick the text color again from the middle of a character, so less of its edge counts.',
  );
});

test('an answer shows only while the reference asks under the key it was asked under', () => {
  const spec: CharactersSpec = {
    region: { x: 0, y: 0, width: 10, height: 10 },
    screenshot: 0,
    reference: null,
    tolerance: 40,
    labels: [{ screenshot: 0, region: { x: 0, y: 0, width: 10, height: 10 }, text: '86' }],
    recorded: [],
  };
  const asked = lookalikeKey(spec) ?? '';
  const answer = { key: asked, pairs: [['8', '6']] as const };
  expect(lookalikeKey(null)).toBeNull();
  expect(lookalikeKey({ ...spec })).toBe(asked);
  expect(pairsArrived(answer, asked)).toEqual([['8', '6']]);
  const relabeled = lookalikeKey({ ...spec, labels: [{ ...spec.labels[0]!, text: '88' }] });
  expect(relabeled).not.toBe(asked);
  expect(pairsArrived(answer, relabeled)).toEqual([]);
  expect(pairsArrived(answer, lookalikeKey({ ...spec, ink: { red: 255, green: 0, blue: 0 } }))).toEqual([]);
  expect(pairsArrived(answer, null)).toEqual([]);
});
