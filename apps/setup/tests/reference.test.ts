import { expect, test } from 'bun:test';

import type { AppFile } from '../src/lib/api.ts';
import { andList, knownCharacters, readers, referenceCharacters } from '../src/lib/reference.ts';

const white = { red: 255, green: 255, blue: 255 };

test('knownCharacters lists each character once, digits first, without whitespace', () => {
  expect(knownCharacters(['87.5%', ' 12 ', '-3'])).toEqual(['1', '2', '3', '5', '7', '8', '%', '-', '.']);
  expect(knownCharacters([])).toEqual([]);
});

test("referenceCharacters reads a reference's samples", () => {
  expect(
    referenceCharacters({
      id: 'percent',
      name: 'Percent font',
      ink: white,
      tolerance: 40,
      samples: [
        { image: 'ref-percent-1.png', text: '10%' },
        { image: 'ref-percent-2.png', text: '9' },
      ],
    }),
  ).toEqual(['0', '1', '9', '%']);
});

test('readers names the number values that read with a reference', () => {
  const screen = { windowSize: { width: 10, height: 10 }, region: { x: 0, y: 0, width: 1, height: 1 } };
  const file: AppFile = {
    format: 2,
    name: 'Example App',
    window: {
      title: '',
      class: '',
      executable: '',
      mustMatch: { title: true, class: false },
      fallback: 'none',
    },
    antiCheat: 'unknown',
    references: [],
    values: [
      {
        id: 'a',
        name: 'Progress',
        screen: { ...screen, check: { kind: 'number', reference: 'percent', whole: true } },
      },
      { id: 'b', name: 'Lives', screen: { ...screen, check: { kind: 'number', reference: 'digits', whole: false } } },
      { id: 'c', name: 'Icon', screen: { ...screen, check: { kind: 'color', color: white, tolerance: 0 } } },
      {
        id: 'd',
        name: 'Best run',
        screen: { ...screen, check: { kind: 'number', reference: 'percent', whole: false } },
      },
    ],
  };
  expect(readers(file, 'percent')).toEqual(['Progress', 'Best run']);
  expect(readers(file, 'none')).toEqual([]);
});

test('andList joins names as a sentence does', () => {
  expect(andList([])).toBe('');
  expect(andList(['A'])).toBe('A');
  expect(andList(['A', 'B'])).toBe('A and B');
  expect(andList(['A', 'B', 'C'])).toBe('A, B and C');
});
