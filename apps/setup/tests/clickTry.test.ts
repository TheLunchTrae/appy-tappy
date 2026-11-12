import { describe, expect, test } from 'bun:test';

import { IDLE, type Try, type TryAnswer, type TryEvent, tryStep } from '../src/lib/clickTry.ts';

const found: TryAnswer = { kind: 'found', at: { x: 4, y: 2, likeness: 97 } };
const sent: TryAnswer = { kind: 'sent', message: 'Sent.', elsewhere: false };

function after(...events: readonly TryEvent[]): Try {
  return events.reduce(tryStep, IDLE);
}

describe('trying a click target', () => {
  test('a try is under way until its own answer or failure comes', () => {
    expect(after({ kind: 'find', attempt: 1 })).toEqual({ kind: 'finding', attempt: 1 });
    expect(after({ kind: 'click', attempt: 2 })).toEqual({ kind: 'clicking', attempt: 2 });
    expect(after({ kind: 'find', attempt: 1 }, { kind: 'answered', attempt: 1, answer: found })).toEqual({
      kind: 'answered',
      answer: found,
    });
    expect(after({ kind: 'click', attempt: 1 }, { kind: 'failed', attempt: 1, message: 'No window.' })).toEqual({
      kind: 'failed',
      message: 'No window.',
    });
  });

  test('an answer to a find that the user changed the target under is dropped', () => {
    for (const what of ['place', 'press'] as const) {
      const changed = after({ kind: 'find', attempt: 1 }, { kind: 'changed', what });
      expect(changed).toEqual(IDLE);
      expect(tryStep(changed, { kind: 'answered', attempt: 1, answer: found })).toEqual(IDLE);
      expect(tryStep(changed, { kind: 'failed', attempt: 1, message: 'Late.' })).toEqual(IDLE);
    }
  });

  test('a click that is being sent keeps its try through a change, so its answer always shows', () => {
    for (const what of ['place', 'press'] as const) {
      const sending = after({ kind: 'click', attempt: 1 }, { kind: 'changed', what });
      expect(sending).toEqual({ kind: 'clicking', attempt: 1 });
      expect(tryStep(sending, { kind: 'answered', attempt: 1, answer: sent })).toEqual({
        kind: 'answered',
        answer: sent,
      });
      expect(tryStep(sending, { kind: 'failed', attempt: 1, message: 'No window.' })).toEqual({
        kind: 'failed',
        message: 'No window.',
      });
    }
  });

  test('an answer to an earlier try does not end a later one', () => {
    const later = after(
      { kind: 'find', attempt: 1 },
      { kind: 'changed', what: 'place' },
      { kind: 'click', attempt: 2 },
    );
    expect(tryStep(later, { kind: 'answered', attempt: 1, answer: found })).toEqual({ kind: 'clicking', attempt: 2 });
    expect(tryStep(later, { kind: 'failed', attempt: 1, message: 'Late.' })).toEqual({ kind: 'clicking', attempt: 2 });
    expect(tryStep(later, { kind: 'answered', attempt: 2, answer: sent })).toEqual({ kind: 'answered', answer: sent });
  });

  test('where it would land stays through a change of button, and nothing stays through a change of place', () => {
    const landed = after({ kind: 'find', attempt: 1 }, { kind: 'answered', attempt: 1, answer: found });
    expect(tryStep(landed, { kind: 'changed', what: 'press' })).toEqual(landed);
    expect(tryStep(landed, { kind: 'changed', what: 'place' })).toEqual(IDLE);
    const clicked = after({ kind: 'click', attempt: 1 }, { kind: 'answered', attempt: 1, answer: sent });
    expect(tryStep(clicked, { kind: 'changed', what: 'press' })).toEqual(IDLE);
    const failed = after({ kind: 'click', attempt: 1 }, { kind: 'failed', attempt: 1, message: 'No.' });
    expect(tryStep(failed, { kind: 'changed', what: 'press' })).toEqual(IDLE);
  });
});
