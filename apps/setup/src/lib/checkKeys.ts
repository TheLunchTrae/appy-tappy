import { labelCharacters } from './reference.ts';

/**
 * What has focus when a key is pressed in the character check: a `field` that takes typing, a `control` that Enter
 * presses, such as a button, or the `step` itself, which is its heading or nothing at all.
 */
export type KeyFocus = 'field' | 'control' | 'step';

/** What the step under the key offers. */
export interface StepKeys {
  readonly back: boolean;
  readonly skip: boolean;
  /** The step's character while one of its shapes can be retyped, and null on a step with none. */
  readonly character: string | null;
}

/** What a key press answers a step with. */
export type KeyAnswer =
  /** The step's default button: Yes, Next, or Done. */
  | { readonly kind: 'primary' }
  | { readonly kind: 'back' }
  | { readonly kind: 'skip' }
  /** The key goes to the "It is" field of the step's first shape that is not checked. */
  | { readonly kind: 'retype' };

type Key = Pick<KeyboardEvent, 'key' | 'altKey' | 'ctrlKey' | 'metaKey' | 'repeat' | 'defaultPrevented'>;

/**
 * What a key answers a step of the character check with, the way a Windows dialog would: Enter presses the default
 * button, Alt with a button's letter presses that button, and a character typed outside a field goes to the field
 * the step is about. Null for a key that is not the check's, so it does what it does anywhere else:
 *
 * - one that something with focus took and marked with preventDefault, or one a held key repeats, so holding Enter
 *   answers one step;
 * - Enter while a field or a control has focus, which keeps it;
 * - a character typed in a field, typed with Ctrl or the Windows key, which is a shortcut, or the same as the
 *   step's, which would change nothing;
 * - Alt with a letter whose button the step does not show.
 *
 * Ctrl and Alt together are AltGr, which types a character on many keyboards, so they count as neither.
 */
export function checkKey(event: Key, focus: KeyFocus, step: StepKeys): KeyAnswer | null {
  const altGr = event.ctrlKey && event.altKey;
  if (event.defaultPrevented || event.repeat || event.metaKey || (event.ctrlKey && !altGr)) {
    return null;
  }
  if (event.altKey && !altGr) {
    const letter = event.key.toLowerCase();
    if (letter === 'b' && step.back) {
      return { kind: 'back' };
    }
    return letter === 's' && step.skip ? { kind: 'skip' } : null;
  }
  if (event.key === 'Enter') {
    return focus === 'step' ? { kind: 'primary' } : null;
  }
  if (focus === 'field' || step.character === null || event.key === step.character) {
    return null;
  }
  // A key that types names its character, and any other has a longer name, such as Tab.
  const [typed, ...more] = labelCharacters(event.key);
  return typed !== undefined && more.length === 0 && typed === event.key ? { kind: 'retype' } : null;
}
