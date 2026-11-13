import { describe, expect, test } from 'bun:test';

import { EMPTY_DRAFT, buildCharacters } from '../src/lib/check.ts';
import { afterRead, type LiveRead, type ReadAnswer } from '../src/lib/liveCheck.ts';
import { trialOf } from '../src/lib/useLiveCheck.ts';

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

describe('trialOf', () => {
  const region = { x: 1, y: 2, width: 30, height: 12 };
  const draft = {
    ...EMPTY_DRAFT,
    region,
    drawnOn: 0,
    ink: { red: 255, green: 0, blue: 0 },
    boxes: [{ id: 1, screenshot: 0, region, text: '87%' }],
  };
  const set = {
    set: async (): Promise<void> => {
      await Promise.resolve();
    },
  };
  const keyOf = (edited: typeof draft, kind: 'characters' | 'check' = 'characters') => {
    const trial = trialOf(kind, buildCharacters(edited, null), set);
    return trial.ok ? trial.key : null;
  };

  test('keys a trial by what its spec holds, so an edit that leaves the spec alone tries nothing again', () => {
    const key = keyOf(draft);
    expect(key).not.toBeNull();
    // An empty box added, the empty box moved, and the same text typed again all teach the same characters.
    const empty = { id: 2, screenshot: 0, region: { ...region, x: 50 }, text: '' };
    expect(keyOf({ ...draft, boxes: [...draft.boxes, empty] })).toBe(key);
    expect(keyOf({ ...draft, boxes: [...draft.boxes, { ...empty, region }] })).toBe(key);
    expect(keyOf({ ...draft, boxes: [{ id: 1, screenshot: 0, region: { ...region }, text: '87% ' }] })).toBe(key);
  });

  test('gives another key to another spec, and to the same spec tried another way', () => {
    const key = keyOf(draft);
    expect(keyOf({ ...draft, boxes: [{ id: 1, screenshot: 0, region, text: '88%' }] })).not.toBe(key);
    expect(keyOf({ ...draft, tolerance: draft.tolerance + 1 })).not.toBe(key);
    expect(keyOf(draft, 'check')).not.toBe(key);
  });

  test('passes on what the trial still needs', () => {
    expect(trialOf('characters', buildCharacters(EMPTY_DRAFT, null), set)).toEqual({
      ok: false,
      next: 'Draw a box around the characters on a screenshot.',
    });
  });
});
