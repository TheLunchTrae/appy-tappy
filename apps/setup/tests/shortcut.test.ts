import { describe, expect, test } from 'bun:test';

import { accelerator, actionKeys, shortcutLabel, spellKeys } from '../src/shortcut.ts';

const none = { ctrlKey: false, altKey: false, shiftKey: false, metaKey: false };

describe('accelerator', () => {
  test('takes function keys alone', () => {
    expect(accelerator({ ...none, code: 'F9' })).toBe('F9');
    expect(accelerator({ ...none, code: 'F24' })).toBe('F24');
    expect(accelerator({ ...none, code: 'F25' })).toBeNull();
  });

  test('needs a modifier for typing keys', () => {
    expect(accelerator({ ...none, code: 'KeyS' })).toBeNull();
    expect(accelerator({ ...none, code: 'Digit4' })).toBeNull();
    expect(accelerator({ ...none, code: 'Space' })).toBeNull();
    expect(accelerator({ ...none, ctrlKey: true, shiftKey: true, code: 'KeyS' })).toBe('Control+Shift+S');
    expect(accelerator({ ...none, altKey: true, code: 'Digit4' })).toBe('Alt+4');
    expect(accelerator({ ...none, metaKey: true, code: 'ArrowUp' })).toBe('Super+Up');
  });

  test('ignores modifiers alone and unknown keys', () => {
    expect(accelerator({ ...none, ctrlKey: true, code: 'ControlLeft' })).toBeNull();
    expect(accelerator({ ...none, code: 'IntlBackslash' })).toBeNull();
  });

  test('takes Print Screen alone', () => {
    expect(accelerator({ ...none, code: 'PrintScreen' })).toBe('PrintScreen');
  });
});

test('shortcutLabel uses keycap names', () => {
  expect(shortcutLabel('Control+Shift+S')).toBe('Ctrl+Shift+S');
  expect(shortcutLabel('Super+F9')).toBe('Win+F9');
});

const typed = { ...none, key: '' };

describe('actionKeys', () => {
  test('spells modifiers in order, then the key', () => {
    expect(actionKeys({ ...typed, code: 'KeyM' })).toBe('M');
    expect(actionKeys({ ...typed, code: 'Digit5' })).toBe('5');
    expect(actionKeys({ ...typed, code: 'F24' })).toBe('F24');
    expect(actionKeys({ ctrlKey: true, altKey: true, shiftKey: true, metaKey: true, key: 'm', code: 'KeyM' })).toBe(
      'Ctrl+Alt+Shift+Win+M',
    );
    expect(actionKeys({ ...typed, shiftKey: true, code: 'ArrowLeft' })).toBe('Shift+Left');
  });

  test('takes the keys an action can press, Tab and Escape included', () => {
    expect(actionKeys({ ...typed, code: 'Tab' })).toBe('Tab');
    expect(actionKeys({ ...typed, code: 'Escape' })).toBe('Escape');
    expect(actionKeys({ ...typed, code: 'Enter' })).toBe('Enter');
    expect(actionKeys({ ...typed, code: 'Backspace' })).toBe('Backspace');
    expect(actionKeys({ ...typed, code: 'PageDown' })).toBe('PageDown');
  });

  test('ignores modifiers alone and keys an action cannot press', () => {
    expect(actionKeys({ ...typed, ctrlKey: true, code: 'ControlLeft' })).toBeNull();
    expect(actionKeys({ ...typed, code: 'F25' })).toBeNull();
    expect(actionKeys({ ...typed, code: 'PrintScreen' })).toBeNull();
    expect(actionKeys({ ...typed, code: 'Numpad1' })).toBeNull();
  });
});

test('actionKeys takes the letter the layout types, not the one in that place in English', () => {
  // The key labeled A on a French keyboard sits where Q does in English.
  expect(actionKeys({ ...typed, ctrlKey: true, key: 'a', code: 'KeyQ' })).toBe('Ctrl+A');
  expect(actionKeys({ ...typed, shiftKey: true, key: 'Z', code: 'KeyY' })).toBe('Shift+Z');
  // A layout that types no Latin letter keeps the letter of the key's place.
  expect(actionKeys({ ...typed, key: 'ф', code: 'KeyA' })).toBe('A');
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
    ]) {
      expect(spellKeys(text)).toBeNull();
    }
  });
});
