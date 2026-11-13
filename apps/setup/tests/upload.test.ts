import { describe, expect, test } from 'bun:test';

import { aboutHeader } from '../src/lib/upload.ts';

describe('aboutHeader', () => {
  test('writes JSON in ASCII that reads back as it was', () => {
    // The Rust side's test in `commands/upload.rs` reads this same header.
    expect(aboutHeader('café 🎉.zip')).toBe(String.raw`"caf\u00e9 \ud83c\udf89.zip"`);
    const spec = { name: 'Ünïcode\u007f "quoted"\n', antiCheat: 'unknown', from: '\u202ename.zip' };
    const header = aboutHeader(spec);
    expect(header).toMatch(/^[\u0020-\u007e]*$/);
    expect(JSON.parse(header)).toEqual(spec);
  });
});
