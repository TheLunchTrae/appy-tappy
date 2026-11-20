import { expect, test } from 'bun:test';

import {
  type ReferenceSave,
  SAVE_AFTER_REVIEW,
  SAVE_AFTER_UNLEARNED,
  checkBackLabel,
  draftHolds,
  referenceSave,
} from '../src/lib/teaching.ts';

const READY: ReferenceSave = {
  named: true,
  built: true,
  tried: true,
  busy: false,
  reviewing: false,
  unlearned: 0,
};

test('a reference saves once it is named, built and tried', () => {
  expect(referenceSave(READY)).toEqual({ can: true, waits: null });
  // The reading box says what the characters need and why a try failed, and a save under way ends by itself.
  for (const missing of [{ built: false }, { tried: false }, { busy: true }]) {
    expect(referenceSave({ ...READY, ...missing })).toEqual({ can: false, waits: null });
  }
});

test('saving waits for a name, and says so', () => {
  expect(referenceSave({ ...READY, named: false })).toEqual({ can: false, waits: 'Name the reference.' });
  expect(referenceSave({ ...READY, named: false, unlearned: 1 })).toEqual({
    can: false,
    waits: SAVE_AFTER_UNLEARNED,
  });
});

test('saving waits while review is open, and says so', () => {
  // Review's edits reach the reference only with Keep, and saving drops the recording they are edits of.
  expect(referenceSave({ ...READY, reviewing: true })).toEqual({ can: false, waits: SAVE_AFTER_REVIEW });
});

test("saving waits while the reference keeps a saved picture it can't learn, and says so", () => {
  // The save would be refused, and settings alone are saved without a try that would have said so.
  expect(referenceSave({ ...READY, unlearned: 2 })).toEqual({ can: false, waits: SAVE_AFTER_UNLEARNED });
  expect(referenceSave({ ...READY, unlearned: 1, reviewing: true })).toEqual({
    can: false,
    waits: SAVE_AFTER_REVIEW,
  });
});

test("the character check's Back label follows the reviewed recording", () => {
  // Opened from a recording's review, the check goes back to it; otherwise to the screenshot.
  expect(checkBackLabel(true)).toBe('Back to review');
  expect(checkBackLabel(false)).toBe('Back to the screenshot');
});

test('a reference with a draft of changes is changed only by finishing the draft', () => {
  const digits = { id: 'digits', name: 'Digits' };
  expect(draftHolds(digits, new Set(['digits']))).toBe(
    'Digits has a draft of changes. Finish it on the References page to change it.',
  );
  expect(draftHolds(digits, new Set(['letters']))).toBeNull();
});
