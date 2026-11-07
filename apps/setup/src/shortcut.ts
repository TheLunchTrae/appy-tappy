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

/** A key press as a recording reads it: the key's place, and the character it types. */
export interface TypedKeyPress extends KeyPress {
  readonly key: string;
}

/**
 * The keys `press` presses, as a key press action spells them, such as `Ctrl+Shift+M`, or null for a
 * modifier alone or a key that an action cannot press. A letter is the one the keyboard layout types,
 * not the one in that place on an English keyboard, because the runtime presses the letter.
 */
export function actionKeys(press: TypedKeyPress): string | null {
  const letter = /^[a-z]$/iu.test(press.key) ? press.key.toUpperCase() : /^Key([A-Z])$/u.exec(press.code)?.[1];
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

const MODIFIERS = ['Ctrl', 'Alt', 'Shift', 'Win'] as const;

/** The names of the keys a key press action can press besides letters, digits and function keys. */
const NAMED_ACTION_KEYS: readonly string[] = Object.values(ACTION_KEYS);

/** One key's name as an action spells it, or null when an action cannot press it. */
function keyName(name: string): string | null {
  if (/^[a-z0-9]$/iu.test(name)) {
    return name.toUpperCase();
  }
  const function_ = /^f([1-9]\d*)$/iu.exec(name)?.[1];
  if (function_ !== undefined) {
    return Number(function_) <= 24 ? `F${function_}` : null;
  }
  return NAMED_ACTION_KEYS.find((named) => named.toLowerCase() === name.toLowerCase()) ?? null;
}

/**
 * `text` in the one spelling a key press action is saved in, such as `Ctrl+Shift+M`, or null when it
 * is not any of Ctrl, Alt, Shift and Win then one key, joined by `+`. It reads the keys as the
 * runtime does: ignoring case and the spaces around each part.
 */
export function spellKeys(text: string): string | null {
  const held = new Set<string>();
  let key: string | null = null;
  for (const part of text.split('+').map((name) => name.trim())) {
    const modifier = MODIFIERS.find((name) => name.toLowerCase() === part.toLowerCase());
    if (key !== null || (modifier !== undefined && held.has(modifier))) {
      // The key comes last, and each modifier once.
      return null;
    }
    if (modifier === undefined) {
      key = keyName(part);
      if (key === null) {
        return null;
      }
    } else {
      held.add(modifier);
    }
  }
  return key === null ? null : [...MODIFIERS.filter((name) => held.has(name)), key].join('+');
}
