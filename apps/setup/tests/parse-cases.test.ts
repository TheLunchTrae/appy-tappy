import { describe, expect, test } from 'bun:test';

import { appName, openTarget, soundFile } from '../src/lib/automation.ts';
import { PARSE_CASES } from '../src/lib/bindings.ts';
import { placeSpot, spotAt } from '../src/lib/click.ts';
import { expectedSet, tooMany } from '../src/lib/expected.ts';
import { labelCharacters } from '../src/lib/reference.ts';
import { type KeyPress, accelerator, spellKeys } from '../src/lib/shortcut.ts';

// The bindings test writes what the Rust side makes of each sample. The page reads them itself, for instant
// feedback, and these tests keep its reading the same as the save's.

const { spotPlacement, ...TEXT_CASES } = PARSE_CASES;

/** The key press a browser reports for the keys `shortcut` names as an accelerator, such as `Control+Shift+S`. */
function pressFor(shortcut: string): KeyPress {
  const parts = shortcut.split('+');
  const key = parts.at(-1) ?? '';
  const arrow = ['Up', 'Down', 'Left', 'Right'].includes(key);
  const code = /^[A-Z]$/u.test(key) ? `Key${key}` : /^\d$/u.test(key) ? `Digit${key}` : arrow ? `Arrow${key}` : key;
  return {
    code,
    ctrlKey: parts.includes('Control'),
    altKey: parts.includes('Alt'),
    shiftKey: parts.includes('Shift'),
    metaKey: parts.includes('Super'),
  };
}

/** The page's reading of each parser's samples, by the parser's name in `PARSE_CASES`. */
const PAGE_READS = new Map<string, (text: string) => string | null>(
  Object.entries({
    openTarget,
    soundFile,
    keys: spellKeys,
    appName,
    // The recorder records a shortcut the save takes from its key press, and never one the save refuses.
    screenshotShortcut: (text) => (accelerator(pressFor(text)) === text ? text : null),
    labelCharacters: (text) => labelCharacters(text).join(''),
    // The save refuses a list longer than a reference saves, and lists the rest once each, in order.
    expectedCharacters: (text) => {
      const expected = expectedSet(text);
      return tooMany(expected) ? null : expected.join('');
    },
  } satisfies Record<keyof typeof TEXT_CASES, (text: string) => string | null>),
);

describe('the page reads every sample as the Rust side does', () => {
  for (const [parser, cases] of Object.entries(TEXT_CASES)) {
    test(parser, () => {
      const read = PAGE_READS.get(parser);
      expect(read).toBeDefined();
      for (const { text, read: rust } of cases) {
        expect({ text, read: read?.(text) }).toEqual({ text, read: rust });
      }
    });
  }

  test('has a reading for every parser', () => {
    expect([...PAGE_READS.keys()].toSorted()).toEqual(Object.keys(TEXT_CASES).toSorted());
  });
});

test('spots land where the Rust side places them, and the spot picked at a point lands there', () => {
  for (const { at, window, place } of spotPlacement) {
    expect({ at, window, place: placeSpot({ kind: 'spot', ...at }, window) }).toEqual({ at, window, place });
    if (place !== null) {
      expect(placeSpot(spotAt(place, window), window)).toEqual(place);
    }
  }
});
