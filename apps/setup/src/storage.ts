// Settings kept in this browser view between runs. They are conveniences, so
// a store that cannot be read or written falls back to the default.

import { DEFAULT_SHORTCUT } from './shortcut.ts';

const SHORTCUT_KEY = 'appytappy.screenshotShortcut';

export function loadShortcut(): string {
  try {
    return localStorage.getItem(SHORTCUT_KEY) ?? DEFAULT_SHORTCUT;
  } catch {
    return DEFAULT_SHORTCUT;
  }
}

export function saveShortcut(shortcut: string): void {
  try {
    localStorage.setItem(SHORTCUT_KEY, shortcut);
  } catch {
    // The shortcut then lasts until the setup app closes.
  }
}
