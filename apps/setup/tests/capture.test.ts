import { expect, test } from 'bun:test';

import { holders } from '../src/lib/capture.ts';

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
