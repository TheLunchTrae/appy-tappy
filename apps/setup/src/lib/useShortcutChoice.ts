// Choosing a new screenshot shortcut from the next keys pressed on a button.

import { type KeyboardEvent, useState } from 'react';

import { errorText } from './api.ts';
import { useSettings } from './settings.ts';
import { accelerator, shortcutLabel } from './shortcut.ts';

/** The screenshot shortcut, and the user's choice of a new one, which is saved in the settings. */
export function useShortcutChoice(): {
  /** The shortcut as the settings hold it, such as `F9` or `Control+Shift+S`. */
  readonly shortcut: string;
  /** The shortcut as the button shows it, such as F9. */
  readonly label: string;
  /** Whether the next keys pressed on the button become the shortcut. */
  readonly choosing: boolean;
  /** Why the new shortcut could not be saved, or null. */
  readonly saveError: string | null;
  /** Starts choosing, or stops without a new shortcut. */
  readonly toggle: () => void;
  /** Stops choosing without a new shortcut, such as when focus leaves the button. */
  readonly cancel: () => void;
  /** The button's key handler: while choosing, Escape stops and a shortcut's keys become the shortcut. */
  readonly onKeyDown: (event: KeyboardEvent<HTMLButtonElement>) => void;
} {
  const { settings, change } = useSettings();
  const shortcut = settings.screenshotShortcut;
  const [choosing, setChoosing] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
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
      change((current) => ({ ...current, screenshotShortcut: next })).then(
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
    shortcut,
    label: shortcutLabel(shortcut),
    choosing,
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
