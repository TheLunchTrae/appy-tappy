// The global shortcut that takes a screenshot, and choosing a new one.

import { useEffect, useRef, useState } from 'react';

import { type ShortcutEvent, register, unregister } from '@tauri-apps/plugin-global-shortcut';

import { background } from './repeat.ts';
import { shortcutLabel } from './shortcut.ts';
import { useShortcutChoice } from './useShortcutChoice.ts';

/**
 * The global shortcut that takes a screenshot with `onPress`, and the user's choice of a new one, which is saved in
 * the settings. It is on while the component that uses it shows, except while the user is choosing.
 */
export function useScreenshotShortcut(onPress: () => Promise<void>): Omit<
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
  useEffect(() => {
    if (choosing) {
      return undefined;
    }
    let current = true;
    const pressed = (event: ShortcutEvent): void => {
      if (event.state === 'Pressed') {
        background(latest.current());
      }
    };
    queue.current = queue.current
      .then(async () => register(shortcut, pressed))
      .then(() => {
        if (current) {
          setError(null);
        }
      })
      .catch(() => {
        if (current) {
          setError(`${shortcutLabel(shortcut)} could not be set up. Another app may use it; choose another.`);
        }
      });
    return () => {
      current = false;
      queue.current = queue.current
        .then(async () => unregister(shortcut))
        .catch(() => {
          // A shortcut that never registered has nothing to remove.
        });
    };
  }, [shortcut, choosing]);

  return { ...choice, error };
}
