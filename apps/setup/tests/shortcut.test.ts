import { describe, expect, test } from 'bun:test';

import { accelerator, shortcutLabel } from '../src/shortcut.ts';

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
