import { expect, test } from 'bun:test';

import { type ReferenceSave, SAVE_AFTER_REVIEW, referenceSave } from '../src/lib/teaching.ts';

const READY: ReferenceSave = {
  named: true,
  built: true,
  tried: true,
  busy: false,
  overLimit: false,
  reviewing: false,
};

test('a reference saves once it is named, built and tried', () => {
  expect(referenceSave(READY)).toEqual({ can: true, waits: null });
  for (const missing of [{ named: false }, { built: false }, { tried: false }, { busy: true }, { overLimit: true }]) {
    expect(referenceSave({ ...READY, ...missing })).toEqual({ can: false, waits: null });
  }
});

test('saving waits while review is open, and says so', () => {
  // Review's edits reach the reference only with Keep, and saving drops the recording they are edits of.
  expect(referenceSave({ ...READY, reviewing: true })).toEqual({ can: false, waits: SAVE_AFTER_REVIEW });
});
