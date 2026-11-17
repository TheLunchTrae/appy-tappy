// The global shortcut that takes a screenshot, and choosing a new one.

import { useEffect, useRef, useState } from 'react';

import { type ShortcutEvent, register, unregister } from '@tauri-apps/plugin-global-shortcut';

import { background } from './repeat.ts';
import { ask } from './request.ts';
import { shortcutLabel } from './shortcut.ts';
import { useShortcutChoice } from './useShortcutChoice.ts';

// The native registration belongs to the app, including when one capture
// screen has unmounted and another is registering the same keys.
let shortcutQueue = Promise.resolve();

function inOrder(change: () => Promise<void>): Promise<void> {
  const next = shortcutQueue.then(change);
  shortcutQueue = next.catch(() => undefined);
  return next;
}

/**
 * The global shortcut that takes a screenshot with `onPress`, and the user's choice of a new one, which is saved in
 * the settings. It is on while the component that uses it shows, except while the user is choosing and while
 * `onPress` is null because there is nothing to take a screenshot of, so other programs keep the keys meanwhile.
 */
export function useScreenshotShortcut(onPress: (() => Promise<unknown>) | null): Omit<
  ReturnType<typeof useShortcutChoice>,
  'shortcut'
> & {
  /** Why the shortcut could not be set up, or null. Saving a new one does not hide it. */
  readonly error: string | null;
} {
  const { shortcut, ...choice } = useShortcutChoice();
  const { choosing } = choice;
  const [error, setError] = useState<string | null>(null);
  const latest = useRef(onPress);
  useEffect(() => {
    latest.current = onPress;
  });

  const takes = onPress !== null;
  useEffect(() => {
    if (choosing || !takes) {
      return undefined;
    }
    let active = true;
    let registered = false;
    const pressed = (event: ShortcutEvent): void => {
      if (active && event.state === 'Pressed' && latest.current !== null) {
        background(latest.current());
      }
    };
    const cancel = ask(
      () =>
        inOrder(async () => {
          if (active) {
            await register(shortcut, pressed);
            registered = true;
          }
        }),
      () => {
        setError(null);
      },
      () => {
        setError(`${shortcutLabel(shortcut)} could not be set up. Another app may use it; choose another.`);
      },
    );
    return () => {
      active = false;
      cancel();
      background(
        inOrder(async () => {
          if (registered) {
            await unregister(shortcut);
          }
        }),
      );
    };
  }, [shortcut, choosing, takes]);

  return { ...choice, error };
}
