import { describe, expect, test } from 'bun:test';

import { fileSaver, type PickedFile, saveZip, type ZipSaver } from '../src/lib/zipFile.ts';

/**
 * How a fake file's writes go: as asked, or failing the write or the removal. A removal fails by rejecting, or by
 * throwing before it returns a promise, as a WebView without `remove` does.
 */
interface Faults {
  readonly write?: boolean;
  readonly remove?: 'rejects' | 'throws';
}

/**
 * A folder of files by name, and a picker that makes the file it picks, empty, as the save picker does. Each file
 * holds the text of the zip written to it.
 */
function folder(faults: Faults = {}): {
  readonly files: Map<string, string>;
  readonly asked: string[];
  readonly pick: (name: string) => Promise<ZipSaver | null>;
} {
  const files = new Map<string, string>();
  const asked: string[] = [];
  const handle = (name: string): PickedFile => ({
    createWritable: async () => {
      let written = '';
      return Promise.resolve({
        write: async (zip: Blob) => {
          if (faults.write === true) {
            throw new Error('The disk is full.');
          }
          written = await zip.text();
        },
        abort: async () => Promise.resolve(),
        close: async () => {
          files.set(name, written);
          await Promise.resolve();
        },
      });
    },
    remove:
      faults.remove === 'throws'
        ? () => {
            throw new TypeError('file.remove is not a function');
          }
        : async () => {
            if (faults.remove === 'rejects') {
              throw new Error('The file is in use.');
            }
            files.delete(name);
            await Promise.resolve();
          },
  });
  const pick = async (name: string): Promise<ZipSaver | null> => {
    asked.push(name);
    files.set(name, '');
    return Promise.resolve(fileSaver(handle(name)));
  };
  return { files, asked, pick };
}

async function failure(promise: Promise<unknown>): Promise<unknown> {
  try {
    await promise;
  } catch (error: unknown) {
    return error;
  }
  return null;
}

const zip = async (): Promise<Blob> => Promise.resolve(new Blob(['zip']));
const tooLarge = async (): Promise<Blob> => Promise.reject(new Error('The bundle is too large.'));

describe('saving a zip', () => {
  test('asks where, then writes it there', async () => {
    const { files, asked, pick } = folder();
    expect(await saveZip('app.zip', zip, pick)).toBe(true);
    expect(asked).toEqual(['app.zip']);
    expect([...files]).toEqual([['app.zip', 'zip']]);
  });

  test('makes nothing when the user closes the picker', async () => {
    let made = false;
    const make = async (): Promise<Blob> => {
      made = true;
      return zip();
    };
    expect(await saveZip('app.zip', make, async () => Promise.resolve(null))).toBe(false);
    expect(made).toBe(false);
  });

  test('leaves no file when the zip fails to be made, such as a bundle', async () => {
    const { files, pick } = folder();
    expect(await failure(saveZip('bundle.zip', tooLarge, pick))).toEqual(new Error('The bundle is too large.'));
    // The picker made the file when it was picked, and the failure removes it.
    expect([...files]).toEqual([]);
  });

  test('leaves no file when the zip fails to be written', async () => {
    const { files, pick } = folder({ write: true });
    expect(await failure(saveZip('bundle.zip', zip, pick))).toEqual(new Error('The disk is full.'));
    expect([...files]).toEqual([]);
  });

  test.each(['rejects', 'throws'] as const)('says why it failed even when the removal %s', async (remove) => {
    const { pick } = folder({ remove });
    expect(await failure(saveZip('bundle.zip', tooLarge, pick))).toEqual(new Error('The bundle is too large.'));
  });
});
