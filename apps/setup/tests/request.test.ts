import { describe, expect, test } from 'bun:test';

import { ask, newest } from '../src/lib/request.ts';

/** A request whose answer the test sends when it chooses. */
function pending<Answer>() {
  const settled = Promise.withResolvers<Answer>();
  let sent = 0;
  return {
    request: async (): Promise<Answer> => {
      sent += 1;
      return settled.promise;
    },
    sent: () => sent,
    ...settled,
  };
}

/** Lets the callbacks of settled promises and of timers up to `ms` run. */
async function after(ms = 0): Promise<void> {
  await new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

describe('ask', () => {
  test('hands over the answer of a request nobody cancelled', async () => {
    const reply = pending<string>();
    const heard: string[] = [];
    ask(
      reply.request,
      (answer) => heard.push(answer),
      () => heard.push('failed'),
    );
    expect(reply.sent()).toBe(1);
    reply.resolve('fits');
    await after();
    expect(heard).toEqual(['fits']);
  });

  test('drops the answer and the failure of a cancelled request', async () => {
    for (const settle of ['resolve', 'reject'] as const) {
      const reply = pending<string>();
      const heard: string[] = [];
      const cancel = ask(
        reply.request,
        (answer) => heard.push(answer),
        () => heard.push('failed'),
      );
      cancel();
      reply[settle]('late');
      await after();
      expect(heard).toEqual([]);
    }
  });

  test('hands a failure to its own callback', async () => {
    const reply = pending<string>();
    const heard: unknown[] = [];
    ask(
      reply.request,
      () => heard.push('answered'),
      (error) => heard.push(error),
    );
    reply.reject('no window');
    await after();
    expect(heard).toEqual(['no window']);
  });

  test("hands what the answer's callback throws to the failure's", async () => {
    const reply = pending<string>();
    const heard: unknown[] = [];
    ask(
      reply.request,
      () => {
        throw new Error('does not fit');
      },
      (error) => heard.push(error),
    );
    reply.resolve('fits');
    await after();
    expect(heard).toEqual([new Error('does not fit')]);
  });

  test('drops the answer of a delayed request cancelled after it was sent', async () => {
    const reply = pending<string>();
    const heard: string[] = [];
    const cancel = ask(
      reply.request,
      (answer) => heard.push(answer),
      () => heard.push('failed'),
      10,
    );
    await after(40);
    expect(reply.sent()).toBe(1);
    cancel();
    reply.resolve('late');
    await after();
    expect(heard).toEqual([]);
  });

  test('waits out its delay, and a cancel meanwhile sends nothing', async () => {
    const first = pending<string>();
    const cancel = ask(
      first.request,
      () => undefined,
      () => undefined,
      20,
    );
    expect(first.sent()).toBe(0);
    cancel();
    const second = pending<string>();
    const heard: string[] = [];
    ask(
      second.request,
      (answer) => heard.push(answer),
      () => undefined,
      20,
    );
    await after(60);
    expect(first.sent()).toBe(0);
    expect(second.sent()).toBe(1);
    second.resolve('once');
    await after();
    expect(heard).toEqual(['once']);
  });
});

describe('newest', () => {
  test('a later ask makes the earlier one stale', () => {
    const asks = newest();
    const first = asks.begin();
    expect(first()).toBe(true);
    const second = asks.begin();
    expect(first()).toBe(false);
    expect(second()).toBe(true);
  });

  test('a drop makes every ask stale and leaves the next one current', () => {
    const asks = newest();
    const before = asks.begin();
    // A read that joined counts with the ask it joined, such as a box drawn by hand over a selection still asked.
    const joined = asks.join();
    asks.drop();
    expect(before()).toBe(false);
    expect(joined()).toBe(false);
    expect(asks.begin()()).toBe(true);
  });

  test('a read that joins the newest ask is stale once another ask starts, and starts none itself', () => {
    const asks = newest();
    const command = asks.begin();
    const poll = asks.join();
    expect(command()).toBe(true);
    expect(poll()).toBe(true);
    asks.begin();
    expect(poll()).toBe(false);
  });
});
