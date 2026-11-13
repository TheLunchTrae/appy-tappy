import { describe, expect, test } from 'bun:test';

import { type AntiCheatDef, LIMITS, type SharedView } from '../src/lib/bindings.ts';
import {
  type ImportEvent,
  type ImportState,
  NO_IMPORT,
  classToConfirm,
  exportName,
  importBusy,
  nextImport,
  originText,
  sizeProblem,
} from '../src/lib/sharing.ts';

function view(antiCheat: AntiCheatDef): SharedView {
  return {
    name: 'Example App 2',
    file: {
      format: 1,
      name: 'Example App',
      window: {
        title: 'Example App',
        class: 'ExampleWindow',
        executable: 'Example.exe',
        mustMatch: { title: false, class: false },
        fallback: 'none',
      },
      sizeChanges: 'often',
      antiCheat,
      origin: { kind: 'byHand' },
      values: [],
      references: [],
    },
  };
}

const bytes = [80, 75];
const reading: ImportState = { kind: 'reading', file: 'friend.zip' };
const refused: ImportState = { kind: 'refused', file: 'friend.zip', problem: 'Not a zip file.' };
const asking: ImportState = {
  kind: 'asking',
  file: 'friend.zip',
  bytes,
  view: view('none'),
  name: 'Example App 2',
  antiCheat: 'unknown',
  problem: null,
};
const importing: ImportState = {
  kind: 'importing',
  file: 'friend.zip',
  bytes,
  view: view('none'),
  name: 'Example App 2',
  antiCheat: 'unknown',
};

describe('nextImport', () => {
  const cases: readonly (readonly [string, ImportState, ImportEvent, ImportState])[] = [
    ['a picked file is read', NO_IMPORT, { kind: 'picked', file: 'friend.zip', problem: null }, reading],
    [
      'a picked file that is too large is refused unread',
      NO_IMPORT,
      { kind: 'picked', file: 'friend.zip', problem: 'Not a zip file.' },
      refused,
    ],
    [
      'a read file asks, with the name on offer and never None to start on',
      reading,
      { kind: 'read', bytes, view: view('none') },
      asking,
    ],
    [
      'a protected class in the file is the one to confirm',
      reading,
      { kind: 'read', bytes, view: view('kernelLevel') },
      { ...asking, view: view('kernelLevel'), antiCheat: 'kernelLevel' },
    ],
    ['a file that cannot be read says why', reading, { kind: 'failed', problem: 'Not a zip file.' }, refused],
    ['the name changes', asking, { kind: 'named', name: 'Mine' }, { ...asking, name: 'Mine' }],
    ['the class changes', asking, { kind: 'classed', antiCheat: 'none' }, { ...asking, antiCheat: 'none' }],
    ['import saves', asking, { kind: 'import' }, importing],
    ['import clears the last failure', { ...asking, problem: 'Pick another name.' }, { kind: 'import' }, importing],
    ['a saved import closes', importing, { kind: 'imported' }, NO_IMPORT],
    [
      'a failed import asks again with why',
      importing,
      { kind: 'failed', problem: 'Pick another name.' },
      { ...asking, problem: 'Pick another name.' },
    ],
    ['cancel closes the question', asking, { kind: 'close' }, NO_IMPORT],
    ['close closes a refusal', refused, { kind: 'close' }, NO_IMPORT],
    ['close waits for the read', reading, { kind: 'close' }, reading],
    ['close waits for the import', importing, { kind: 'close' }, importing],
    ['a file picked while one is open does nothing', asking, { kind: 'picked', file: 'b.zip', problem: null }, asking],
    ['a read that was not asked for does nothing', NO_IMPORT, { kind: 'read', bytes, view: view('none') }, NO_IMPORT],
    ['a late read leaves the question alone', asking, { kind: 'read', bytes: [], view: view('userMode') }, asking],
    ['a late failure leaves a closed dialog closed', NO_IMPORT, { kind: 'failed', problem: 'x' }, NO_IMPORT],
    ['a failure with nothing under way does nothing', asking, { kind: 'failed', problem: 'x' }, asking],
    ['the name is locked while it saves', importing, { kind: 'named', name: 'Mine' }, importing],
    ['the class is locked while it saves', importing, { kind: 'classed', antiCheat: 'none' }, importing],
    ['import without the question does nothing', reading, { kind: 'import' }, reading],
    ['an import that was not asked for does nothing', asking, { kind: 'imported' }, asking],
  ];
  for (const [name, state, event, expected] of cases) {
    test(name, () => {
      expect(nextImport(state, event)).toEqual(expected);
    });
  }

  test('it is busy only while it reads or saves', () => {
    expect([NO_IMPORT, reading, refused, asking, importing].map(importBusy)).toEqual([false, true, false, false, true]);
  });
});

describe('classToConfirm', () => {
  test('starts on what the file says, except None', () => {
    expect((['unknown', 'userMode', 'kernelLevel', 'none'] as const).map(classToConfirm)).toEqual([
      'unknown',
      'userMode',
      'kernelLevel',
      'unknown',
    ]);
  });
});

describe('exportName', () => {
  test('is made of the letters and digits of the name', () => {
    expect(exportName('Example App: Save 3')).toBe('appytappy-example-app-save-3.zip');
    expect(exportName(' ../?')).toBe('appytappy-application.zip');
  });
});

describe('originText', () => {
  test('says how a definition was made', () => {
    expect(originText({ kind: 'byHand' })).toBe('Made by hand');
    expect(originText({ kind: 'recording' })).toBe('Made with a recording');
    expect(originText({ kind: 'imported', file: 'friend.zip' })).toBe('Imported from friend.zip');
  });
});

describe('sizeProblem', () => {
  test('refuses only a file larger than an import reads', () => {
    expect(sizeProblem('friend.zip', LIMITS.maxSharedBytes)).toBeNull();
    expect(sizeProblem('friend.zip', LIMITS.maxSharedBytes + 1)).toBe(
      'AppyTappy cannot import friend.zip: it holds more than 16 MB.',
    );
  });
});
