// The owner's one capture of a window, which the screens that preview one share: whichever shows holds it.

import { useCallback, useEffect, useSyncExternalStore } from 'react';

import { stopCapture } from './api.ts';
import { background } from './repeat.ts';

/** Counts the screens that hold the capture, and tells its listeners when one starts or stops holding it. */
export function holders(): {
  readonly hold: () => () => void;
  readonly held: () => boolean;
  readonly subscribe: (listener: () => void) => () => void;
} {
  let count = 0;
  const listeners = new Set<() => void>();
  const changed = (by: number): void => {
    count += by;
    for (const listener of listeners) {
      listener();
    }
  };
  return {
    hold: () => {
      changed(1);
      let holding = true;
      return () => {
        if (holding) {
          holding = false;
          changed(-1);
        }
      };
    },
    held: () => count > 0,
    subscribe: (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}

const capture = holders();

/**
 * Says that the component holds the capture while it shows: it picks the window, and the capture stops when it
 * goes, with the capture's screenshots and recording, so no other screen has to stop it for it. The function it
 * returns stops the capture early, such as before another window is picked.
 */
export function useCapture(): () => void {
  useEffect(() => {
    const release = capture.hold();
    return () => {
      release();
      background(stopCapture());
    };
  }, []);
  return useCallback(() => {
    background(stopCapture());
  }, []);
}

/** Whether a screen holds the capture now. */
export function useCaptureHeld(): boolean {
  return useSyncExternalStore(capture.subscribe, capture.held);
}
