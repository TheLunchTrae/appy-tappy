// The screenshot shortcut, as a global-shortcut accelerator such as `Control+Shift+S`.

/** The keys of a key press that a shortcut reads. */
export interface KeyPress {
  readonly code: string;
  readonly ctrlKey: boolean;
  readonly altKey: boolean;
  readonly shiftKey: boolean;
  readonly metaKey: boolean;
}

export const DEFAULT_SHORTCUT = 'F9';

const NAMED_KEYS: Readonly<Record<string, string>> = {
  Space: 'Space',
  Insert: 'Insert',
  Delete: 'Delete',
  Home: 'Home',
  End: 'End',
  PageUp: 'PageUp',
  PageDown: 'PageDown',
  ArrowUp: 'Up',
  ArrowDown: 'Down',
  ArrowLeft: 'Left',
  ArrowRight: 'Right',
  PrintScreen: 'PrintScreen',
  Pause: 'Pause',
};

/** The accelerator key for `code`, and whether it may stand alone without stealing typing. */
function key(code: string): { readonly name: string; readonly alone: boolean } | null {
  const function_ = /^F([1-9]|1\d|2[0-4])$/u.exec(code);
  if (function_ !== null) {
    return { name: code, alone: true };
  }
  const letter = /^Key([A-Z])$/u.exec(code)?.[1];
  if (letter !== undefined) {
    return { name: letter, alone: false };
  }
  const digit = /^Digit(\d)$/u.exec(code)?.[1];
  if (digit !== undefined) {
    return { name: digit, alone: false };
  }
  const named = NAMED_KEYS[code];
  if (named !== undefined) {
    return { name: named, alone: code === 'PrintScreen' || code === 'Pause' };
  }
  return null;
}

/**
 * The accelerator for `press`, or null when it cannot be a shortcut: a
 * modifier alone, an unsupported key, or a typing key with no modifier,
 * which would stop that key working in every other app.
 */
export function accelerator(press: KeyPress): string | null {
  const found = key(press.code);
  if (found === null) {
    return null;
  }
  const modifiers = [
    press.ctrlKey ? 'Control' : null,
    press.altKey ? 'Alt' : null,
    press.shiftKey ? 'Shift' : null,
    press.metaKey ? 'Super' : null,
  ].filter((modifier) => modifier !== null);
  if (modifiers.length === 0 && !found.alone) {
    return null;
  }
  return [...modifiers, found.name].join('+');
}

/** The accelerator as the keycap labels show it, such as `Ctrl+Shift+S`. */
export function shortcutLabel(shortcut: string): string {
  return shortcut.replace('Control', 'Ctrl').replace('Super', 'Win');
}

/** The keys a key press action can press, by their `code`, as the automations file spells them. */
const ACTION_KEYS: Readonly<Record<string, string>> = {
  Space: 'Space',
  Enter: 'Enter',
  Tab: 'Tab',
  Escape: 'Escape',
  Backspace: 'Backspace',
  Insert: 'Insert',
  Delete: 'Delete',
  Home: 'Home',
  End: 'End',
  PageUp: 'PageUp',
  PageDown: 'PageDown',
  ArrowUp: 'Up',
  ArrowDown: 'Down',
  ArrowLeft: 'Left',
  ArrowRight: 'Right',
};

/**
 * The keys `press` presses, as a key press action spells them, such as `Ctrl+Shift+M`, or null for a
 * modifier alone or a key that an action cannot press.
 */
export function actionKeys(press: KeyPress): string | null {
  const letter = /^Key([A-Z])$/u.exec(press.code)?.[1];
  const digit = /^Digit(\d)$/u.exec(press.code)?.[1];
  const function_ = /^F([1-9]|1\d|2[0-4])$/u.test(press.code) ? press.code : undefined;
  const found = letter ?? digit ?? function_ ?? ACTION_KEYS[press.code];
  if (found === undefined) {
    return null;
  }
  return [
    press.ctrlKey ? 'Ctrl' : null,
    press.altKey ? 'Alt' : null,
    press.shiftKey ? 'Shift' : null,
    press.metaKey ? 'Win' : null,
    found,
  ]
    .filter((part) => part !== null)
    .join('+');
}
