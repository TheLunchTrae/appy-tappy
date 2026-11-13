import { expect, test } from 'bun:test';

import { holders, ownCapture } from '../src/lib/capture.ts';

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

test('a screen stops the capture it started, by its number, once', () => {
  const stopped: number[] = [];
  const own = ownCapture((capture) => stopped.push(capture));
  own.enter();
  // Nothing was started, so there is nothing of the screen's to stop.
  own.stop();
  expect(stopped).toEqual([]);
  own.started(3);
  own.stop();
  own.stop();
  expect(stopped).toEqual([3]);
  // Two picks whose answers arrive out of order leave the newer capture as the screen's.
  own.started(5);
  own.started(4);
  own.leave();
  expect(stopped).toEqual([3, 5]);
  // A pick that answers after the screen went stops at once.
  own.started(6);
  expect(stopped).toEqual([3, 5, 6]);
  own.enter();
  own.started(7);
  expect(stopped).toEqual([3, 5, 6]);
});
