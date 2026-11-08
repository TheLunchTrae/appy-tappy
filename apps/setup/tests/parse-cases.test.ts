import { describe, expect, test } from 'bun:test';

import { appName, openTarget, soundFile } from '../src/lib/automation.ts';
import { PARSE_CASES } from '../src/lib/bindings.ts';
import { spellKeys } from '../src/lib/shortcut.ts';

// The bindings test writes what the Rust parsers make of each sample. The page reads them itself, for instant
// feedback, and these tests keep its reading the same as the save's.

/** The page's reading of each parser's samples, by the parser's name in `PARSE_CASES`. */
const PAGE_READS = new Map<string, (text: string) => string | null>(
  Object.entries({
    openTarget,
    soundFile,
    keys: spellKeys,
    appName,
  } satisfies Record<keyof typeof PARSE_CASES, (text: string) => string | null>),
);

describe('the page reads every sample as the Rust side does', () => {
  for (const [parser, cases] of Object.entries(PARSE_CASES)) {
    test(parser, () => {
      const read = PAGE_READS.get(parser);
      expect(read).toBeDefined();
      for (const { text, read: rust } of cases) {
        expect({ text, read: read?.(text) }).toEqual({ text, read: rust });
      }
    });
  }

  test('has a reading for every parser', () => {
    expect([...PAGE_READS.keys()].toSorted()).toEqual(Object.keys(PARSE_CASES).toSorted());
  });
});
