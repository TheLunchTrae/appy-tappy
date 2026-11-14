import { describe, expect, test } from 'bun:test';

import { spellKeys, trimmed } from '../src/lib/text.ts';

describe('trimmed', () => {
  test('takes off white space as Rust does', () => {
    expect(trimmed(' \t Ctrl+M　\n')).toBe('Ctrl+M');
    expect(trimmed('﻿C:\\Timer.exe﻿')).toBe('﻿C:\\Timer.exe﻿');
    expect(trimmed('   ')).toBe('');
    expect(trimmed('a 😀 ')).toBe('a 😀');
  });

  test('keeps a long run of spaces inside the text', () => {
    const text = `a${' '.repeat(200_000)}b`;
    expect(trimmed(text)).toBe(text);
  });
});

describe('spellKeys', () => {
  test('spells keys as the file keeps them, reading them as the runtime does', () => {
    expect(spellKeys('ctrl + shift + m')).toBe('Ctrl+Shift+M');
    expect(spellKeys('Shift+Ctrl+M')).toBe('Ctrl+Shift+M');
    expect(spellKeys('win+alt+f24')).toBe('Alt+Win+F24');
    expect(spellKeys('5')).toBe('5');
    expect(spellKeys('pagedown')).toBe('PageDown');
    expect(spellKeys(' Escape ')).toBe('Escape');
  });

  test('refuses what an action cannot press', () => {
    for (const text of [
      '',
      'Ctrl',
      'Ctrl+',
      'Ctrl++M',
      'M+Ctrl',
      'Ctrl+Ctrl+M',
      'M+N',
      'F0',
      'F25',
      'F01',
      'é',
      'Numpad1',
      'ArrowUp',
      // The Kelvin sign, which a case-insensitive match would read as K.
      '\u212A',
    ]) {
      expect(spellKeys(text)).toBeNull();
    }
  });
});
