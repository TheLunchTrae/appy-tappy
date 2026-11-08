// Text read the way the Rust side reads it, so the page's checks agree with the save's.

/** `text` without the white space around it, as Rust's `str::trim` takes it off: a byte order mark is not white space. */
export function trimmed(text: string): string {
  return text.replaceAll(/^\p{White_Space}+|\p{White_Space}+$/gu, '');
}

/**
 * `text` with only A to Z in lower case, as Rust compares ASCII letters. Unicode case folding would also read the
 * Kelvin sign as `k` and the long s as `s`.
 */
export function asciiLower(text: string): string {
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
