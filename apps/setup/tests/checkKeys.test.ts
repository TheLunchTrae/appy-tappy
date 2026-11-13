import { expect, test } from 'bun:test';

import { type KeyFocus, type StepKeys, checkKey } from '../src/lib/checkKeys.ts';

const PLAIN = { altKey: false, ctrlKey: false, metaKey: false, repeat: false, defaultPrevented: false };
/** A step for the character 7 that shows every button. */
const STEP: StepKeys = { back: true, skip: true, character: '7' };

function pressed(key: string, focus: KeyFocus = 'step', step: StepKeys = STEP, held: Partial<typeof PLAIN> = {}) {
  return checkKey({ ...PLAIN, ...held, key }, focus, step)?.kind ?? null;
}

test('Enter presses the default button unless a field or a control has focus', () => {
  expect(pressed('Enter')).toBe('primary');
  expect(pressed('Enter', 'field')).toBeNull();
  expect(pressed('Enter', 'control')).toBeNull();
  // The last step has no character and still has its Done.
  expect(pressed('Enter', 'step', { back: false, skip: false, character: null })).toBe('primary');
});

test('Alt with B or S presses Back or Skip from anywhere, when the step shows that button', () => {
  expect(pressed('b', 'step', STEP, { altKey: true })).toBe('back');
  expect(pressed('S', 'field', STEP, { altKey: true })).toBe('skip');
  expect(pressed('s', 'control', STEP, { altKey: true })).toBe('skip');
  expect(pressed('b', 'step', { ...STEP, back: false }, { altKey: true })).toBeNull();
  expect(pressed('s', 'step', { ...STEP, skip: false }, { altKey: true })).toBeNull();
  // Alt never types a character or presses the default button.
  expect(pressed('1', 'step', STEP, { altKey: true })).toBeNull();
  expect(pressed('Enter', 'step', STEP, { altKey: true })).toBeNull();
});

test('a typed character goes to the shape unless a field has focus', () => {
  expect(pressed('1')).toBe('retype');
  expect(pressed('b', 'control')).toBe('retype');
  expect(pressed('é')).toBe('retype');
  expect(pressed('1', 'field')).toBeNull();
});

test('a key that would retype nothing is left alone', () => {
  // The step's own character changes nothing.
  expect(pressed('7')).toBeNull();
  // White space is no character, so Space still presses a button that has focus.
  expect(pressed(' ', 'control')).toBeNull();
  for (const key of ['Tab', 'Escape', 'ArrowLeft', 'F5', 'Dead', 'Shift']) {
    expect(pressed(key)).toBeNull();
  }
  // A step with no shape to retype: the last one, the one for text that fits no step, or one whose shapes are
  // all left out.
  expect(pressed('1', 'step', { ...STEP, character: null })).toBeNull();
});

test("a shortcut, a repeated key, and a key something else took are not the check's", () => {
  expect(pressed('s', 'step', STEP, { ctrlKey: true })).toBeNull();
  expect(pressed('1', 'step', STEP, { metaKey: true })).toBeNull();
  // AltGr reports Ctrl and Alt together and types a character, so it presses no button.
  expect(pressed('s', 'field', STEP, { ctrlKey: true, altKey: true })).toBeNull();
  expect(pressed('@', 'step', STEP, { ctrlKey: true, altKey: true })).toBe('retype');
  expect(pressed('Enter', 'step', STEP, { repeat: true })).toBeNull();
  expect(pressed('Enter', 'step', STEP, { defaultPrevented: true })).toBeNull();
  expect(pressed('b', 'step', STEP, { altKey: true, defaultPrevented: true })).toBeNull();
});
