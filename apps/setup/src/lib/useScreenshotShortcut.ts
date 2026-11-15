// The global shortcut that takes a screenshot, and choosing a new one.

import { useEffect, useRef, useState } from 'react';

import { type ShortcutEvent, register, unregister } from '@tauri-apps/plugin-global-shortcut';

import { background } from './repeat.ts';
import { ask } from './request.ts';
import { shortcutLabel } from './shortcut.ts';
import { useShortcutChoice } from './useShortcutChoice.ts';

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

  // Registering and unregistering run one after another, in the order the
  // effect asked for them, so a quick unregister cannot overtake a register.
  const queue = useRef<Promise<void>>(Promise.resolve());
  const takes = onPress !== null;
  useEffect(() => {
    if (choosing || !takes) {
      return undefined;
    }
    const pressed = (event: ShortcutEvent): void => {
      if (event.state === 'Pressed' && latest.current !== null) {
        background(latest.current());
      }
    };
    const cancel = ask(
      async () => {
        const registered = queue.current.then(async () => register(shortcut, pressed));
        // The queue goes on whether or not the shortcut registered.
        queue.current = registered.catch(() => undefined);
        return registered;
      },
      () => {
        setError(null);
      },
      () => {
        setError(`${shortcutLabel(shortcut)} could not be set up. Another app may use it; choose another.`);
      },
    );
    return () => {
      cancel();
      queue.current = queue.current
        .then(async () => unregister(shortcut))
        .catch(() => {
          // A shortcut that never registered has nothing to remove.
        });
    };
  }, [shortcut, choosing, takes]);

  return { ...choice, error };
}
