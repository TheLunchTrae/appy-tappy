// The global shortcut that takes a screenshot, and choosing a new one.

import { type KeyboardEvent, useEffect, useRef, useState } from 'react';

import { type ShortcutEvent, register, unregister } from '@tauri-apps/plugin-global-shortcut';

import { errorText } from './api.ts';
import { background } from './repeat.ts';
import { useSettings } from './settings.ts';
import { accelerator, shortcutLabel } from './shortcut.ts';

/**
 * The global shortcut that takes a screenshot with `onPress`, and the user's choice of a new one, which is saved in
 * the settings. It is on while the component that uses it shows, except while the user is choosing.
 */
export function useScreenshotShortcut(onPress: () => Promise<void>): {
  /** The shortcut as the button shows it, such as F9. */
  readonly label: string;
  /** Whether the next keys pressed on the button become the shortcut. */
  readonly choosing: boolean;
  /** Why the shortcut could not be set up, or null. */
  readonly error: string | null;
  /** Why the new shortcut could not be saved, or null. Setting the old one up again must not hide it. */
  readonly saveError: string | null;
  /** Starts choosing, or stops without a new shortcut. */
  readonly toggle: () => void;
  /** Stops choosing without a new shortcut, such as when focus leaves the button. */
  readonly cancel: () => void;
  /** The button's key handler: while choosing, Escape stops and a shortcut's keys become the shortcut. */
  readonly onKeyDown: (event: KeyboardEvent<HTMLButtonElement>) => void;
} {
  const settings = useSettings();
  const shortcut = settings.settings.screenshotShortcut;
  const [choosing, setChoosing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
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

  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>): void => {
    if (!choosing) {
      return;
    }
    event.preventDefault();
    if (event.code === 'Escape') {
      setChoosing(false);
      return;
    }
    const next = accelerator(event);
    if (next !== null) {
      setChoosing(false);
      settings
        .change((current) => ({ ...current, screenshotShortcut: next }))
        .then(
          (problem) => {
            setSaveError(problem);
          },
          (failure: unknown) => {
            setSaveError(errorText(failure));
          },
        );
    }
  };
  return {
    label: shortcutLabel(shortcut),
    choosing,
    error,
    saveError,
    toggle: () => {
      setChoosing((current) => !current);
    },
    cancel: () => {
      setChoosing(false);
    },
    onKeyDown,
  };
}
