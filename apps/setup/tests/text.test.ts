import { describe, expect, test } from 'bun:test';

import { trimmed } from '../src/lib/text.ts';

describe('trimmed', () => {
  test('takes off white space as Rust does', () => {
    expect(trimmed(' \t Ctrl+M　\n')).toBe('Ctrl+M');
    expect(trimmed('﻿C:\\Timer.exe﻿')).toBe('﻿C:\\Timer.exe﻿');
    expect(trimmed('   ')).toBe('');
    expect(trimmed('a 😀 ')).toBe('a 😀');
  });

  test('reads a long run of spaces inside the text quickly', () => {
    const text = `a${' '.repeat(200_000)}b`;
    const start = performance.now();
    expect(trimmed(text)).toBe(text);
    expect(performance.now() - start).toBeLessThan(1000);
  });
});
