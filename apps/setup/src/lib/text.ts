// Text read the way the Rust side reads it, so the page's checks agree with the save's: any text's white space,
// case and length, and what a typed path, link, key press or app name comes to. `PARSE_CASES` holds what the Rust
// side makes of samples of those, and `tests/parse-cases.test.ts` keeps the page's reading of them the same.

import { KEY_NAMES, LIMITS } from './bindings.ts';

const LEADING_SPACE = /^\p{White_Space}+/u;
const SPACE = /^\p{White_Space}$/u;

/** `text` without the white space around it, as Rust's `str::trim` takes it off: a byte order mark is not white space. */
export function trimmed(text: string): string {
  const rest = text.replace(LEADING_SPACE, '');
  // Walked back by hand, because a pattern anchored at the end tries again at each space of a long run inside the
  // text. Every White_Space character is one UTF-16 code unit, so a unit at a time is a character at a time.
  let end = rest.length;
  while (end > 0 && SPACE.test(rest.charAt(end - 1))) {
    end -= 1;
  }
  return rest.slice(0, end);
}

/**
 * `text` with only A to Z in lower case, as Rust compares ASCII letters. Unicode case folding would also read the
 * Kelvin sign as `k` and the long s as `s`.
 */
function asciiLower(text: string): string {
  return text.replaceAll(/[A-Z]/gu, (letter) => letter.toLowerCase());
}

/** The length of `text` in UTF-8 bytes, as the saved files' limits count it. */
export function utf8Bytes(text: string): number {
  return new TextEncoder().encode(text).length;
}

/** Whether `text` holds a control character, such as a tab or a line break. */
export function hasControl(text: string): boolean {
  return /\p{Cc}/u.test(text);
}

/**
 * A plain decimal number, such as `90`, `-3`, `12.5` or `1,234`: as a number
 * check reads one, `.` is the decimal point and `,` only separates groups of
 * three digits.
 */
const DECIMAL = /^-?(?:\d{1,3}(?:,\d{3})+(?:\.\d*)?|\d+(?:\.\d*)?|\.\d+)$/u;

/** A number typed into a field, or null when it is not a plain decimal number or too large to hold. */
export function typedNumber(text: string): number | null {
  const typed = text.trim();
  const number = DECIMAL.test(typed) ? Number(typed.replaceAll(',', '')) : Number.NaN;
  return Number.isFinite(number) ? number : null;
}

/**
 * A path or link as typed, without spaces around it or the quotes that File Explorer's Copy as path puts around
 * a path.
 */
export function typedPath(text: string): string {
  const path = trimmed(text);
  return path.length >= 2 && path.startsWith('"') && path.endsWith('"') ? trimmed(path.slice(1, -1)) : path;
}

/**
 * Whether `text` is a full Windows path on a drive, such as `C:\Tools\Timer.exe`. A relative one would depend on a
 * folder the user never sees, and a network path, such as `\\server\share\file`, is refused, as the runtime
 * refuses it.
 */
function isFullPath(text: string): boolean {
  return /^[A-Za-z]:[\\/]/u.test(text);
}

/** Whether `text` fits in an open or sound action: not empty, not too long, and no control characters. */
function fitsTarget(text: string): boolean {
  return text !== '' && !hasControl(text) && utf8Bytes(text) <= LIMITS.maxTargetBytes;
}

/** Whether `text` is a web link: `http` or `https` only, as the runtime opens no other kind. */
function isWebLink(text: string): boolean {
  return /^https?:\/\/\P{White_Space}+$/u.test(asciiLower(text));
}

/**
 * What an open action keeps of `text`, as the Rust side reads it: a full path or a web link without the spaces
 * around it, or null when it is neither.
 */
export function openTarget(text: string): string | null {
  const target = trimmed(text);
  return fitsTarget(target) && (isFullPath(target) || isWebLink(target)) ? target : null;
}

/** What a sound action keeps of `text`, as the Rust side reads it: a WAV file's full path, or null. */
export function soundFile(text: string): string | null {
  const file = trimmed(text);
  return fitsTarget(file) && isFullPath(file) && asciiLower(file).endsWith('.wav') ? file : null;
}

/** `text` as a media or volume action's app name, as the Rust side reads it, or null when it can't be one. */
export function appName(text: string): string | null {
  return text !== '' && !hasControl(text) && utf8Bytes(text) <= LIMITS.maxActionAppBytes ? text : null;
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
