// The screenshot shortcut, as a global-shortcut accelerator such as `Control+Shift+S`, and the keys of a key press
// action.

import { KEY_NAMES, SHORTCUT_KEYS } from './bindings.ts';
import { asciiLower, trimmed } from './text.ts';

/** The keys of a key press that a shortcut reads. */
export interface KeyPress {
  readonly code: string;
  readonly ctrlKey: boolean;
  readonly altKey: boolean;
  readonly shiftKey: boolean;
  readonly metaKey: boolean;
}

/** A screenshot shortcut's named key. */
type ShortcutKey = (typeof SHORTCUT_KEYS.named)[number];

/** The browser's `code` for each named key a shortcut can end with, and its name in a sentence. */
const SHORTCUT_KEY_CODES: Readonly<Record<ShortcutKey, { readonly code: string; readonly words: string }>> = {
  Space: { code: 'Space', words: 'Space' },
  Up: { code: 'ArrowUp', words: 'an arrow' },
  Down: { code: 'ArrowDown', words: 'an arrow' },
  Left: { code: 'ArrowLeft', words: 'an arrow' },
  Right: { code: 'ArrowRight', words: 'an arrow' },
  Insert: { code: 'Insert', words: 'Insert' },
  Delete: { code: 'Delete', words: 'Delete' },
  Home: { code: 'Home', words: 'Home' },
  End: { code: 'End', words: 'End' },
  PageUp: { code: 'PageUp', words: 'Page Up' },
  PageDown: { code: 'PageDown', words: 'Page Down' },
  PrintScreen: { code: 'PrintScreen', words: 'Print Screen' },
  Pause: { code: 'Pause', words: 'Pause' },
};

/** Which key press flag holds each modifier a shortcut can hold down, and its keycap name. */
const SHORTCUT_MODIFIERS: Readonly<
  Record<
    (typeof SHORTCUT_KEYS.modifiers)[number],
    { readonly flag: keyof Omit<KeyPress, 'code'>; readonly keycap: string }
  >
> = {
  Control: { flag: 'ctrlKey', keycap: 'Ctrl' },
  Alt: { flag: 'altKey', keycap: 'Alt' },
  Shift: { flag: 'shiftKey', keycap: 'Shift' },
  Super: { flag: 'metaKey', keycap: 'Win' },
};

/** Whether the shortcut key `name` may stand alone without stealing typing. */
function alone(name: ShortcutKey): boolean {
  return (SHORTCUT_KEYS.alone as readonly ShortcutKey[]).includes(name);
}

/** The accelerator key for `code`, and whether it may stand alone without stealing typing. */
function key(code: string): { readonly name: string; readonly alone: boolean } | null {
  const number = /^F([1-9]\d*)$/u.exec(code)?.[1];
  if (number !== undefined) {
    return Number(number) <= SHORTCUT_KEYS.maxFunctionKey ? { name: code, alone: true } : null;
  }
  const letter = /^Key([A-Z])$/u.exec(code)?.[1];
  if (letter !== undefined) {
    return { name: letter, alone: false };
  }
  const digit = /^Digit(\d)$/u.exec(code)?.[1];
  if (digit !== undefined) {
    return { name: digit, alone: false };
  }
  const named = SHORTCUT_KEYS.named.find((name) => SHORTCUT_KEY_CODES[name].code === code);
  return named === undefined ? null : { name: named, alone: alone(named) };
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
  const modifiers = SHORTCUT_KEYS.modifiers.filter((modifier) => press[SHORTCUT_MODIFIERS[modifier].flag]);
  if (modifiers.length === 0 && !found.alone) {
    return null;
  }
  return [...modifiers, found.name].join('+');
}

/** `items` joined as a sentence lists them, such as `a, b, or c`. */
function listed(items: readonly string[], type: 'conjunction' | 'disjunction'): string {
  return new Intl.ListFormat('en', { type }).format(items);
}

/** Which shortcuts the screenshot shortcut can be, in words. */
export function shortcutKeysInWords(): string {
  const words = (names: readonly ShortcutKey[]): string[] => [
    ...new Set(names.map((name) => SHORTCUT_KEY_CODES[name].words)),
  ];
  const typing = SHORTCUT_KEYS.named.filter((name) => !alone(name));
  const modifiers = SHORTCUT_KEYS.modifiers.map((modifier) => SHORTCUT_MODIFIERS[modifier].keycap);
  return `${listed([`F1 to F${SHORTCUT_KEYS.maxFunctionKey}`, ...words(SHORTCUT_KEYS.alone)], 'conjunction')} work alone. ${listed(['A letter', 'a digit', ...words(typing)], 'disjunction')} needs ${listed(modifiers, 'disjunction')} with it, so it keeps working everywhere else. Other keys can't be the shortcut.`;
}

/** The accelerator as the keycap labels show it, such as `Ctrl+Shift+S`. */
export function shortcutLabel(shortcut: string): string {
  return shortcut.replace('Control', 'Ctrl').replace('Super', 'Win');
}

/** The named keys a key press action can press, by their `code`, as the automations file spells them. */
const ACTION_KEYS: Readonly<Record<string, (typeof KEY_NAMES.named)[number]>> = {
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
  const letter = /^[A-Za-z]$/u.test(press.key) ? press.key.toUpperCase() : /^Key([A-Z])$/u.exec(press.code)?.[1];
  const digit = /^Digit(\d)$/u.exec(press.code)?.[1];
  const number = /^F([1-9]\d*)$/u.exec(press.code)?.[1];
  const function_ = number !== undefined && Number(number) <= KEY_NAMES.maxFunctionKey ? press.code : undefined;
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

const MODIFIERS = KEY_NAMES.modifiers;

/** One key's name as an action spells it, or null when an action cannot press it. */
function keyName(name: string): string | null {
  if (/^[A-Za-z0-9]$/u.test(name)) {
    return name.toUpperCase();
  }
  const function_ = /^[Ff]([1-9]\d*)$/u.exec(name)?.[1];
  if (function_ !== undefined) {
    return Number(function_) <= KEY_NAMES.maxFunctionKey ? `F${function_}` : null;
  }
  return KEY_NAMES.named.find((named) => asciiLower(named) === asciiLower(name)) ?? null;
}

/**
 * `text` in the one spelling a key press action is saved in, such as `Ctrl+Shift+M`, or null when it
 * is not any of Ctrl, Alt, Shift and Win then one key, joined by `+`. It reads the keys as the
 * runtime does: ignoring case and the spaces around each part.
 */
export function spellKeys(text: string): string | null {
  const held = new Set<string>();
  let key: string | null = null;
  for (const part of text.split('+').map(trimmed)) {
    const modifier = MODIFIERS.find((name) => asciiLower(name) === asciiLower(part));
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
