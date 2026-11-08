import { useEffect, useRef } from 'react';

/** Lets `task` finish on its own; its errors are already shown to the user, so any left are only logged. */
export function background(task: Promise<unknown>): void {
  task.catch((error: unknown) => {
    console.error(error);
  });
}

/**
 * Runs `task` while `active`, waiting `pauseMs` after each run finishes, so a
 * slow answer never piles up requests behind it. The latest `task` is used on
 * each run.
 */
export function useRepeat(task: () => Promise<void>, pauseMs: number, active: boolean): void {
  const latest = useRef(task);
  useEffect(() => {
    latest.current = task;
  });
  useEffect(() => {
    if (!active) {
      return undefined;
    }
    let stopped = false;
    // A function, because the check after the await must see a stop made meanwhile.
    const isStopped = (): boolean => stopped;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const run = async (): Promise<void> => {
      if (stopped) {
        return;
      }
      await latest.current();
      if (!isStopped()) {
        timer = setTimeout(() => {
          background(run());
        }, pauseMs);
      }
    };
    background(run());
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  }, [active, pauseMs]);
}
