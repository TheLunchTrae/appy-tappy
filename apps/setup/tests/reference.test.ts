import { expect, test } from 'bun:test';

import type { AppFile } from '../src/lib/bindings.ts';
import {
  andList,
  knownCharacters,
  labelCharacters,
  readers,
  readersNeed,
  referenceCharacters,
  readersLose,
  unreadBy,
} from '../src/lib/reference.ts';

const white = { red: 255, green: 255, blue: 255 };

test('knownCharacters lists each character once, digits first, without whitespace', () => {
  expect(knownCharacters(['87.5%', ' 12 ', '-3'])).toEqual(['1', '2', '3', '5', '7', '8', '%', '-', '.']);
  expect(knownCharacters([])).toEqual([]);
  // By code point: a character outside the Basic Multilingual Plane comes after a fullwidth one.
  expect(knownCharacters(['\u{1d7d9}z\uff11'])).toEqual(['z', '\uff11', '\u{1d7d9}']);
});

test('labelCharacters keeps every character of a label but white space, in order', () => {
  expect(labelCharacters(' 8 7\t%\u3000')).toEqual(['8', '7', '%']);
});

test("referenceCharacters reads a reference's samples", () => {
  expect(
    referenceCharacters({
      id: 'percent',
      name: 'Percent font',
      origin: { kind: 'byHand' } as const,
      guesses: false,
      samples: [
        { image: 'ref-percent-1.png', text: '10%' },
        { image: 'ref-percent-2.png', text: '9' },
      ],
    }),
  ).toEqual(['0', '1', '9', '%']);
});

test('referenceCharacters leaves out what a reference learned but does not read', () => {
  expect(
    referenceCharacters({
      id: 'percent',
      name: 'Percent font',
      origin: { kind: 'byHand' } as const,
      guesses: false,
      samples: [{ image: 'ref-percent-1.png', text: 'Lv 10%' }],
      characters: '0123456789 %',
    }),
  ).toEqual(['0', '1', '%']);
});

test('readers names the number values that read with a reference, in a check or a fix', () => {
  const screen = { windowSize: { width: 10, height: 10 }, region: { x: 0, y: 0, width: 1, height: 1 }, fixes: [] };
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
    sizeChanges: 'often',
    antiCheat: 'unknown',
    references: [],
    values: [
      {
        id: 'a',
        name: 'Progress',
        origin: { kind: 'byHand' } as const,
        screen: { ...screen, check: { kind: 'number', reference: 'percent', whole: true } },
      },
      {
        id: 'b',
        name: 'Lives',
        origin: { kind: 'byHand' } as const,
        screen: {
          ...screen,
          check: { kind: 'number', reference: 'digits', whole: false },
          fixes: [{ windowSize: { width: 20, height: 20 }, region: screen.region, images: [], reference: 'percent' }],
        },
      },
      {
        id: 'c',
        name: 'Icon',
        origin: { kind: 'byHand' } as const,
        screen: { ...screen, check: { kind: 'color', color: white, tolerance: 0 } },
      },
      {
        id: 'd',
        name: 'Best run',
        origin: { kind: 'byHand' } as const,
        screen: { ...screen, check: { kind: 'number', reference: 'percent', whole: false } },
      },
    ],
  };
  expect(readers(file, 'percent')).toEqual(['Progress', 'Lives', 'Best run']);
  expect(readers(file, 'digits')).toEqual(['Lives']);
  expect(readers(file, 'none')).toEqual([]);
  // Every reader of `percent` reads every character it knows, so none is left out.
  expect(readersNeed(file, 'percent')).toEqual([]);
  const needing = (characters: readonly string[]): AppFile => ({
    ...file,
    values: file.values.map((value, index) =>
      value.screen.check.kind === 'number'
        ? {
            ...value,
            screen: { ...value.screen, check: { ...value.screen.check, characters: characters[index] ?? '' } },
          }
        : value,
    ),
  });
  // A fix reads the characters its value needs, so Lives adds its 1.
  expect(readersNeed(needing(['8%', '1', '', '%']), 'percent')).toEqual(['1', '8', '%']);
  // Best run reads every character the reference knows.
  expect(readersNeed(needing(['8%', '1', '', '']), 'percent')).toEqual([]);
  expect(readersNeed(needing(['8%', '1', '', '%']), 'none')).toEqual([]);
  // A reference that reads only some characters needs no others.
  const reading = (characters: string, file: AppFile): AppFile => ({
    ...file,
    references: [
      { id: 'percent', name: 'Percent', origin: { kind: 'byHand' }, guesses: false, samples: [], characters },
    ],
  });
  expect(readersNeed(reading('0123456789', file), 'percent')).toEqual([
    '0',
    '1',
    '2',
    '3',
    '4',
    '5',
    '6',
    '7',
    '8',
    '9',
  ]);
  expect(readersNeed(reading('18', needing(['8%', '1', '', '%'])), 'percent')).toEqual(['1', '8']);
  expect(readersNeed(reading('%', { ...file, values: [] }), 'percent')).toEqual(['%']);
  // Values that need none of what it reads count for nothing.
  expect(readersNeed(reading('5', needing(['8%', '1', '%', '%'])), 'percent')).toEqual(['5']);
});

test('andList joins names as a sentence does', () => {
  expect(andList([])).toBe('');
  expect(andList(['A'])).toBe('A');
  expect(andList(['A', 'B'])).toBe('A and B');
  expect(andList(['A', 'B', 'C'])).toBe('A, B and C');
});

test('unreadBy names what a value needs that its reference does not read', () => {
  const reference = {
    id: 'percent',
    name: 'Percent',
    origin: { kind: 'byHand' } as const,
    guesses: false,
    samples: [],
  };
  expect(unreadBy({ ...reference, characters: '0123456789' }, ['1', '%', 'O'])).toEqual(['%', 'O']);
  expect(unreadBy(reference, ['1', '%'])).toEqual([]);
});

test('readersLose names what the values that read with a reference would stop reading', () => {
  const value = (name: string, characters?: string) => ({
    id: name,
    name,
    origin: { kind: 'byHand' } as const,
    screen: {
      windowSize: { width: 10, height: 10 },
      region: { x: 0, y: 0, width: 1, height: 1 },
      fixes: [],
      check: {
        kind: 'number' as const,
        reference: 'percent',
        whole: false,
        ...(characters === undefined ? {} : { characters }),
      },
    },
  });
  const file: AppFile = {
    format: 2,
    name: 'Player',
    window: { title: '', class: '', executable: '', mustMatch: { title: true, class: false }, fallback: 'none' },
    sizeChanges: 'often',
    antiCheat: 'unknown',
    references: [],
    values: [value('Progress', '12%'), value('Best', '1'), value('Any')],
  };
  expect(readersLose(file, 'percent', ['1', '2'])).toBe(
    'Progress needs %, which it would no longer read. Turn it on, or change what the value needs.',
  );
  expect(readersLose(file, 'percent', ['%'])).toBe(
    'Progress needs 1 2 and Best needs 1, which it would no longer read. Turn them on, or change what the values need.',
  );
  expect(readersLose(file, 'percent', ['1', '2', '%'])).toBeNull();
  expect(readersLose(file, 'percent', [])).toBeNull();
});

test('referenceCharacters and unreadBy take a list that names no character as every character', () => {
  const reference = { id: 'percent', name: 'Percent', origin: { kind: 'byHand' } as const, characters: ' ' };
  expect(referenceCharacters({ ...reference, guesses: false, samples: [{ image: 'a.png', text: '1%' }] })).toEqual([
    '1',
    '%',
  ]);
  expect(unreadBy({ ...reference, guesses: false, samples: [] }, ['%'])).toEqual([]);
});
