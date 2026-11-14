// The owner's one capture of a window, which the screens that preview one share: whichever shows holds it.

import { useCallback, useEffect, useState, useSyncExternalStore } from 'react';

import { pickWindow, stopCapture } from './api.ts';
import { background } from './repeat.ts';
import { ownTicket } from './ticket.ts';

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
 * Says that the component holds the capture while it shows: it picks the window with `pick`, and its capture stops
 * when it goes, with the capture's screenshots and recording, so no other screen has to stop it for it. `stop` stops
 * its capture early, such as before another window is picked.
 */
export function useCapture(): {
  readonly pick: (window: number) => Promise<void>;
  readonly stop: () => void;
} {
  // The screen stops the capture it started by the number its pick answered (`ownTicket`), so the stop it sends as it
  // goes never stops the next screen's.
  const [own] = useState(() =>
    ownTicket((started) => {
      background(stopCapture(started));
    }),
  );
  useEffect(() => {
    const release = capture.hold();
    own.enter();
    return () => {
      release();
      own.leave();
    };
  }, [own]);
  const pick = useCallback(
    async (window: number): Promise<void> => {
      own.started(await pickWindow(window));
    },
    [own],
  );
  return { pick, stop: own.stop };
}

/** Whether a screen holds the capture now. */
export function useCaptureHeld(): boolean {
  return useSyncExternalStore(capture.subscribe, capture.held);
}
