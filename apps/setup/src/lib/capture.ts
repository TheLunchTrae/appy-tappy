// The owner's one capture of a window, which the screens that preview one share: whichever shows holds it.

import { useEffect, useState, useSyncExternalStore } from 'react';

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
 * The screens' captures of a window, which each screen starts with `pick` and stops by the number its pick answered
 * (`ownTicket`), so the stop it sends as it goes never stops the next screen's. Picks go to the owner one at a time,
 * in the order the screens asked, and a pick whose screen went before its turn is not sent. The owner takes what the
 * page sends in no set order, so a pick that a closed screen sent could otherwise take the capture after the next
 * screen's pick, and the closed screen's stop would then leave the next screen with none.
 */
export function screenCaptures(
  pick: (window: number) => Promise<number>,
  stop: (capture: number) => void,
): () => {
  /** The screen shows. */
  readonly enter: () => void;
  /** Captures `window` for the screen once the picks before it answered, unless the screen went by then. */
  readonly pick: (window: number) => Promise<void>;
  /** Stops the screen's capture, if it has one. */
  readonly stop: () => void;
  /** The screen goes, and its capture with it. */
  readonly leave: () => void;
} {
  // The newest pick, which the next one waits for, whether it worked or not.
  let picking: Promise<void> = Promise.resolve();
  return () => {
    const own = ownTicket(stop);
    return {
      enter: own.enter,
      pick: async (window) => {
        const turn = picking.then(async () => {
          if (own.shows()) {
            own.started(await pick(window));
          }
        });
        picking = turn.catch(() => undefined);
        await turn;
      },
      stop: own.stop,
      leave: own.leave,
    };
  };
}

const screens = screenCaptures(pickWindow, (started) => {
  background(stopCapture(started));
});

/**
 * Says that the component holds the capture while it shows: it picks the window with `pick`, and its capture stops
 * when it goes, with the capture's screenshots and recording, so no other screen has to stop it for it. `stop` stops
 * its capture early, such as before another window is picked.
 */
export function useCapture(): {
  readonly pick: (window: number) => Promise<void>;
  readonly stop: () => void;
} {
  const [own] = useState(screens);
  useEffect(() => {
    const release = capture.hold();
    own.enter();
    return () => {
      release();
      own.leave();
    };
  }, [own]);
  return { pick: own.pick, stop: own.stop };
}

/** Whether a screen holds the capture now. */
export function useCaptureHeld(): boolean {
  return useSyncExternalStore(capture.subscribe, capture.held);
}
