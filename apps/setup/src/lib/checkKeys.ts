import { labelCharacters } from './reference.ts';

/**
 * What has focus when a key is pressed in the character check: a `field` that takes typing, a field that is
 * `typing`, which holds a character the user typed and has not answered with yet, a `control` that Enter presses,
 * which is a button or a link, or the `step` itself, which is its heading, a tick box, or nothing at all.
 */
export type KeyFocus = 'field' | 'typing' | 'control' | 'step';

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
  /** The key goes to the "It is" field of the step's first shape that is not checked, or its first once all are. */
  | { readonly kind: 'retype' }
  /** The character typed in the field that has focus becomes its shape's answer, and the step stays. */
  | { readonly kind: 'commit' };

type Key = Pick<
  KeyboardEvent,
  'key' | 'code' | 'altKey' | 'ctrlKey' | 'metaKey' | 'repeat' | 'defaultPrevented' | 'getModifierState'
>;

/** The letter Alt is held with: the one the key types or, on a keyboard that types no Latin letter, the one its place has. */
function accessLetter(event: Pick<Key, 'key' | 'code'>): string {
  if (/^[a-z]$/iu.test(event.key)) {
    return event.key.toLowerCase();
  }
  return /^Key[A-Z]$/u.test(event.code) ? event.code.slice(3).toLowerCase() : '';
}

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
 * Alt with B or S while a field holds a character typed and not answered with yet makes that character the answer
 * and stays at the step. The button's click does the same, since pressing it takes focus from the field; going on
 * at once would drop what was typed. AltGr types a character on many keyboards, so it counts as neither Ctrl nor Alt.
 */
export function checkKey(event: Key, focus: KeyFocus, step: StepKeys): KeyAnswer | null {
  const altGr = event.getModifierState('AltGraph');
  if (event.defaultPrevented || event.repeat || event.metaKey || (event.ctrlKey && !altGr)) {
    return null;
  }
  if (event.altKey && !altGr) {
    const letter = accessLetter(event);
    const answer = letter === 'b' && step.back ? 'back' : letter === 's' && step.skip ? 'skip' : null;
    return answer === null ? null : { kind: focus === 'typing' ? 'commit' : answer };
  }
  if (event.key === 'Enter') {
    return focus === 'step' ? { kind: 'primary' } : null;
  }
  if (focus === 'field' || focus === 'typing' || step.character === null || event.key === step.character) {
    return null;
  }
  // A key that types names its character, and any other has a longer name, such as Tab.
  const [typed, ...more] = labelCharacters(event.key);
  return typed !== undefined && more.length === 0 && typed === event.key ? { kind: 'retype' } : null;
}
