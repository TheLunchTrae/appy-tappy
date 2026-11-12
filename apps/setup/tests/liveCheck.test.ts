import { describe, expect, test } from 'bun:test';

import { afterRead, type LiveRead, type ReadAnswer } from '../src/lib/liveCheck.ts';

const shown: LiveRead = { reading: { kind: 'number', value: 87 }, problem: null };
const failing: LiveRead = { reading: null, problem: 'Not read on the live window now.' };
const read: ReadAnswer = { ok: true, reading: { kind: 'number', value: 12 } };
const failed: ReadAnswer = { ok: false, problem: 'No pixel in the region has the color.' };

describe('afterRead', () => {
  test('takes an answer asked under the check in force', () => {
    expect(afterRead(failing, 2, 2, read)).toEqual({ reading: { kind: 'number', value: 12 }, problem: null });
    expect(afterRead(shown, 2, 2, failed)).toEqual({ reading: null, problem: 'No pixel in the region has the color.' });
    // Before a frame arrived there is nothing to show, and no problem.
    expect(afterRead(failing, 2, 2, { ok: true, reading: null })).toEqual({ reading: null, problem: null });
  });

  test('drops an answer asked under an older check, reading or problem', () => {
    // A check, a saved value's edit, and the same edit tried again each take the next number.
    expect(afterRead(shown, 1, 2, read)).toBe(shown);
    expect(afterRead(shown, 1, 2, failed)).toBe(shown);
    expect(afterRead(failing, 1, 2, read)).toBe(failing);
  });

  test('drops an answer once the check changed and no check is in force', () => {
    expect(afterRead(shown, 1, null, read)).toBe(shown);
    expect(afterRead(shown, 1, null, failed)).toBe(shown);
  });

  test('drops an answer asked between a change of the check and the Rust side taking the new one', () => {
    // The Rust side still read it with the check before, though the new check is in force by the time it answers.
    expect(afterRead(shown, null, 2, read)).toBe(shown);
    expect(afterRead(shown, null, 2, failed)).toBe(shown);
    expect(afterRead(shown, null, null, read)).toBe(shown);
  });
});
