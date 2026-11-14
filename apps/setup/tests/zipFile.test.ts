import { describe, expect, test } from 'bun:test';

import { saveZip, type ZipSaver } from '../src/lib/zipFile.ts';

/** A picker that records each name it was asked with and each zip written where it picked. */
function picker(picks: boolean): {
  readonly asked: string[];
  readonly written: Blob[];
  readonly pick: (name: string) => Promise<ZipSaver | null>;
} {
  const asked: string[] = [];
  const written: Blob[] = [];
  const pick = async (name: string): Promise<ZipSaver | null> => {
    asked.push(name);
    const write: ZipSaver = async (zip) => {
      written.push(zip);
      await Promise.resolve();
    };
    return Promise.resolve(picks ? write : null);
  };
  return { asked, written, pick };
}

describe('saving a zip that is made first', () => {
  test('writes it where the user picks', async () => {
    const zip = new Blob(['zip']);
    const { asked, written, pick } = picker(true);
    expect(await saveZip('app.zip', async () => Promise.resolve(zip), pick)).toBe(true);
    expect(asked).toEqual(['app.zip']);
    expect(written).toEqual([zip]);
  });

  test('says so when the user closes the picker', async () => {
    const { written, pick } = picker(false);
    expect(await saveZip('app.zip', async () => Promise.resolve(new Blob(['zip'])), pick)).toBe(false);
    expect(written).toEqual([]);
  });

  test('asks nothing and makes no file when the zip fails to be made', async () => {
    const { asked, pick } = picker(true);
    const tooLarge = async (): Promise<Blob> => Promise.reject(new Error('The export is too large.'));
    let failure: unknown = null;
    try {
      await saveZip('app.zip', tooLarge, pick);
    } catch (error: unknown) {
      failure = error;
    }
    expect(failure).toEqual(new Error('The export is too large.'));
    // The picker makes the file it picks, so a picker never opened leaves none behind.
    expect(asked).toEqual([]);
  });
});
