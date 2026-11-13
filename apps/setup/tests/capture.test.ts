import { expect, test } from 'bun:test';

import { holders, screenCaptures } from '../src/lib/capture.ts';

test('the capture is held while any screen holds it, and each change is told once', () => {
  const capture = holders();
  let told = 0;
  const unsubscribe = capture.subscribe(() => {
    told += 1;
  });
  expect(capture.held()).toBe(false);
  const flow = capture.hold();
  const target = capture.hold();
  expect(capture.held()).toBe(true);
  flow();
  // Letting go twice counts once, so the other screen still holds it.
  flow();
  expect(capture.held()).toBe(true);
  target();
  expect(capture.held()).toBe(false);
  expect(told).toBe(4);
  unsubscribe();
  capture.hold();
  expect(told).toBe(4);
});

/** A pick the test answers by hand, with the window it was for. */
interface Asked {
  readonly window: number;
  readonly answer: (capture: number) => void;
  readonly fail: (error: string) => void;
}

/**
 * Screens whose picks wait for the test to answer them, against an owner with one capture slot that a pick takes and
 * a stop of the capture in it empties.
 */
function owner(): {
  readonly asked: Asked[];
  readonly slot: () => number | null;
  readonly screen: ReturnType<typeof screenCaptures>;
} {
  const asked: Asked[] = [];
  let slot: number | null = null;
  const screen = screenCaptures(
    async (window) =>
      new Promise<number>((resolve, reject) => {
        asked.push({
          window,
          answer: (capture) => {
            slot = capture;
            resolve(capture);
          },
          fail: (error) => {
            reject(new Error(error));
          },
        });
      }),
    (capture) => {
      if (slot === capture) {
        slot = null;
      }
    },
  );
  return { asked, slot: () => slot, screen };
}

/** Lets the picks that were answered run on. */
async function settle(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

test('a pick that a closed screen sent never takes the next screen capture', async () => {
  const { asked, slot, screen } = owner();
  const closed = screen();
  closed.enter();
  const closedPick = closed.pick(1);
  await settle();
  closed.leave();
  const next = screen();
  next.enter();
  const nextPick = next.pick(2);
  await settle();
  // The next screen's pick waits for the closed screen's to answer, so the owner takes them in that order.
  expect(asked.map((pick) => pick.window)).toEqual([1]);
  asked[0]?.answer(10);
  await closedPick;
  // The closed screen's capture stopped as soon as its pick answered.
  expect(slot()).toBeNull();
  await settle();
  expect(asked.map((pick) => pick.window)).toEqual([1, 2]);
  asked[1]?.answer(11);
  await nextPick;
  expect(slot()).toBe(11);
});

test('a pick whose screen went before its turn is not sent, and a failed pick holds up none after it', async () => {
  const { asked, slot, screen } = owner();
  const first = screen();
  first.enter();
  const failed = first.pick(1);
  await settle();
  const gone = screen();
  gone.enter();
  const never = gone.pick(2);
  gone.leave();
  const last = screen();
  last.enter();
  const picked = last.pick(3);
  asked[0]?.fail('No window.');
  const failure = await failed.then(
    () => null,
    (error: unknown) => error,
  );
  expect(failure).toEqual(new Error('No window.'));
  await never;
  await settle();
  expect(asked.map((pick) => pick.window)).toEqual([1, 3]);
  asked[1]?.answer(12);
  await picked;
  expect(slot()).toBe(12);
});
