import { expect, test } from 'bun:test';

import type { AppFile } from '../src/lib/bindings.ts';
import { readAppFile } from '../src/lib/useAppFile.ts';

const file: AppFile = {
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
  antiCheat: 'unknown',
  values: [],
  references: [],
};

/** A read that fails with `reason`, as a command that cannot read the file does. */
async function failing(reason: unknown): Promise<AppFile | null> {
  const read = Promise.withResolvers<AppFile | null>();
  read.reject(reason);
  return read.promise;
}

test('a file that reads shows with no error, and one no longer saved shows neither', async () => {
  expect(await readAppFile(async () => Promise.resolve(file))).toEqual({ file, error: null });
  expect(await readAppFile(async () => Promise.resolve(null))).toEqual({ file: null, error: null });
});

test('an unreadable application file shows its error and no file', async () => {
  const broken = "Example App's app.json cannot be read: expected value at line 1 column 1.";
  expect(await readAppFile(async () => failing(broken))).toEqual({ file: null, error: broken });
  expect(await readAppFile(async () => failing(new Error('lost')))).toEqual({
    file: null,
    error: 'Something went wrong in AppyTappy Setup.',
  });
});
