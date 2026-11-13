// The owner's one capture of a window, which the screens that preview one share: whichever shows holds it.

import { useCallback, useEffect, useState, useSyncExternalStore } from 'react';

import { pickWindow, stopCapture } from './api.ts';
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
 * The capture a screen started, by the number the owner gave it, which the screen stops by that number: the owner
 * drops a stop of a capture that another took the place of, so a screen that goes never stops the next screen's.
 */
export function ownCapture(stop: (capture: number) => void): {
  /** The screen shows. */
  readonly enter: () => void;
  /** Hears that the screen's pick started `capture`, which stops at once when the screen went meanwhile. */
  readonly started: (capture: number) => void;
  /** Stops the screen's capture, if it has one. */
  readonly stop: () => void;
  /** The screen goes, and its capture with it. */
  readonly leave: () => void;
} {
  let held: number | null = null;
  let shows = false;
  const end = (): void => {
    if (held !== null) {
      stop(held);
      held = null;
    }
  };
  return {
    enter: () => {
      shows = true;
    },
    started: (capture) => {
      // The owner stopped the screen's last capture when it started this one. The newest has the highest number,
      // whichever pick answers last.
      held = Math.max(held ?? capture, capture);
      if (!shows) {
        end();
      }
    },
    stop: end,
    leave: () => {
      shows = false;
      end();
    },
  };
}

/**
 * Says that the component holds the capture while it shows: it picks the window with `pick`, and its capture stops
 * when it goes, with the capture's screenshots and recording, so no other screen has to stop it for it. `stop` stops
 * its capture early, such as before another window is picked.
 */
export function useCapture(): {
  readonly pick: (window: number) => Promise<void>;
  readonly stop: () => void;
} {
  const [own] = useState(() =>
    ownCapture((started) => {
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
