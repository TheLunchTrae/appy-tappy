import { expect, test } from 'bun:test';

import { supersedes } from '../src/lib/reads.ts';

test('the answer of a read of the saved lists is taken only when no newer one was', () => {
  expect(supersedes(1, 0)).toBe(true);
  expect(supersedes(5, 3)).toBe(true);
  // A periodic read that started before a save and answers after the read the save started.
  expect(supersedes(3, 4)).toBe(false);
  // The same answer is not taken twice.
  expect(supersedes(4, 4)).toBe(false);
});
