import { expect, test } from 'bun:test';

import type { CharactersSpec } from '../src/lib/bindings.ts';
import {
  lookalikeWarning,
  neededPairs,
  riskKey,
  riskMark,
  risksAnswered,
  risksArrived,
  risksFailed,
} from '../src/lib/useReadingRisks.ts';

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

test('the warning names the pairs and says to check the text typed for them', () => {
  expect(lookalikeWarning([])).toBeNull();
  expect(lookalikeWarning([['8', '6']])).toBe(
    "8 and 6 have the same shape in what it learned, so it can't tell one from the other. Check that the text typed for each picture is what it shows. A value that needs both won't read them.",
  );
  expect(lookalikeWarning(pairs.slice(0, 2))).toBe(
    "Some characters have the same shape in what it learned, so it can't tell one of each pair from the other: 8 and 6; 0 and 8. Check that the text typed for each picture is what it shows. A value that needs both won't read them.",
  );
});

test('an answer shows only while the reference asks under the key it was asked under', () => {
  const spec: CharactersSpec = {
    region: { x: 0, y: 0, width: 10, height: 10 },
    screenshot: 0,
    reference: null,
    labels: [{ screenshot: 0, region: { x: 0, y: 0, width: 10, height: 10 }, text: '86' }],
    recorded: [],
    dropped: [],
  };
  const asked = riskKey(spec) ?? '';
  const risks = { lookalikes: [['8', '6']], problem: null } as const;
  const answer = { key: asked, risks };
  const none = { lookalikes: [], problem: null };
  expect(riskKey(null)).toBeNull();
  expect(riskKey({ ...spec })).toBe(asked);
  expect(risksArrived(answer, asked)).toEqual(risks);
  const relabeled = riskKey({ ...spec, labels: [{ ...spec.labels[0]!, text: '88' }] });
  expect(relabeled).not.toBe(asked);
  expect(risksArrived(answer, relabeled)).toEqual(none);
  expect(risksArrived(answer, null)).toEqual(none);
});

test('a check that failed says so under the key it was asked under, unlike a check that found nothing', () => {
  const failed = risksFailed('That screenshot is gone. Take a new one and try again.');
  expect(failed).toEqual({
    lookalikes: [],
    problem: "Can't check for characters that look alike. That screenshot is gone. Take a new one and try again.",
  });
  expect(risksArrived({ key: 'asked', risks: failed }, 'asked')).toEqual(failed);
  // A change to what it asks about drops the failure with the answer, and the next answer replaces it.
  expect(risksArrived({ key: 'asked', risks: failed }, 'changed').problem).toBeNull();
});

test('a reference that cannot be taught is answered no risks and no problem, since its check says why', () => {
  const none = { lookalikes: [], problem: null };
  expect(risksAnswered(null)).toEqual(none);
  expect(risksAnswered({ lookalikes: [['8', '6']] })).toEqual({ lookalikes: [['8', '6']], problem: null });
});

test('a saved reference is marked for the pairs its values cannot tell apart', () => {
  expect(riskMark({ lookalikes: [] }, [])).toBeNull();
  expect(riskMark({ lookalikes: [['8', '6']] }, [])).toBe(
    '8 and 6 have the same shape in what it learned. Edit it to see what to do.',
  );
  // No value that reads with it needs either character.
  expect(riskMark({ lookalikes: [['8', '6']] }, ['1', '2'])).toBeNull();
  expect(riskMark({ lookalikes: pairs }, ['0'])).toBe(
    'Some characters have the same shape in what it learned. Edit it to see what to do.',
  );
});
