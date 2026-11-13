import { expect, test } from 'bun:test';

import { ownTicket } from '../src/lib/ticket.ts';

test('a component stops the work it started, by its number, once', () => {
  const stopped: number[] = [];
  const own = ownTicket((ticket) => stopped.push(ticket));
  own.enter();
  // Nothing was started, so there is nothing of the component's to stop.
  own.stop();
  expect(stopped).toEqual([]);
  own.started(3);
  expect(own.held()).toBe(3);
  own.stop();
  own.stop();
  expect(stopped).toEqual([3]);
  expect(own.held()).toBeNull();
  // Two starts whose answers arrive out of order leave the newer work as the component's, and going on with work
  // answers its own number again.
  own.started(5);
  own.started(4);
  own.started(5);
  own.leave();
  expect(stopped).toEqual([3, 5]);
  // A start that answers after the component went stops at once.
  own.started(6);
  expect(stopped).toEqual([3, 5, 6]);
  own.enter();
  own.started(7);
  expect(stopped).toEqual([3, 5, 6]);
});
