// A file a command takes: the page sends it as the request's raw bytes, and what it says about the file as JSON in
// a header. The Rust side reads both in `commands/upload.rs`.

/**
 * `about` as JSON for a header, which holds only ASCII: every other character is written as its `\u` escapes, which
 * JSON reads back as the character.
 */
export function aboutHeader(about: unknown): string {
  return JSON.stringify(about).replace(
    /[\u007f-\uffff]/g,
    (unit) => `\\u${unit.charCodeAt(0).toString(16).padStart(4, '0')}`,
  );
}
