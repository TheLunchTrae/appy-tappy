// Asking the Rust side for something whose answer may arrive after the screen moved on. Every such ask goes through
// one of these, so an answer nobody waits for anymore is dropped the same way everywhere.

import { useCallback, useEffect, useRef, useState } from 'react';

/**
 * Asks `request`, at once or once `delayMs` passed, and hands what it answers to `answered`, or why it failed to
 * `failed`, which also hears what `answered` throws. The function it returns cancels the ask: a request not yet sent
 * is never sent, and the answer to one that was is dropped. It is what an effect returns, so an ask ends when what it
 * asked about changes or its component goes; the delay makes a run of changes, such as a dragged slider, ask once.
 */
export function ask<Answer>(
  request: () => Promise<Answer>,
  answered: (answer: Answer) => void,
  failed: (error: unknown) => void,
  delayMs = 0,
): () => void {
  let wanted = true;
  const send = (): void => {
    request()
      .then((answer) => {
        if (wanted) {
          answered(answer);
        }
      })
      // Hears what `answered` throws too, so a screen's error shows a failure to use the answer.
      .catch((error: unknown) => {
        if (wanted) {
          failed(error);
        }
      });
  };
  if (delayMs <= 0) {
    send();
    return () => {
      wanted = false;
    };
  }
  const timer = setTimeout(send, delayMs);
  return () => {
    wanted = false;
    clearTimeout(timer);
  };
}

/** Tells the newest of a run of asks from the ones before it. */
export interface Newest {
  /** Starts an ask, which makes those before it stale, and answers its check: whether it is still the newest. */
  readonly begin: () => () => boolean;
  /**
   * Answers a check that holds until the next ask starts, without starting one: for a read that goes along with
   * the newest ask, such as a poll that any command overtakes.
   */
  readonly join: () => () => boolean;
  /** Makes every ask so far stale. */
  readonly drop: () => void;
}

export function newest(): Newest {
  let count = 0;
  const join = (): (() => boolean) => {
    const mine = count;
    return () => count === mine;
  };
  return {
    begin: () => {
      count += 1;
      return join();
    },
    join,
    drop: () => {
      count += 1;
    },
  };
}

/**
 * Asks where only the newest counts, for asks an event starts rather than an effect. Each `begin` makes the asks
 * started before it stale, as do `drop` and unmounting, and gives the new ask its check: whether it is still the
 * newest, to ask between steps and before showing what it answered.
 */
export function useNewest(): Newest {
  const [asks] = useState(newest);
  useEffect(() => asks.drop, [asks]);
  return asks;
}

/** Whether the component still shows, to ask after an await before acting for it. */
export function useMounted(): () => boolean {
  const mounted = useRef(false);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  return useCallback(() => mounted.current, []);
}
