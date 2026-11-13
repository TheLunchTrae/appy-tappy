import { describe, expect, test } from 'bun:test';

import type { Picture } from '../src/lib/pixels.ts';
import { arrive } from '../src/lib/useScreenshots.ts';

const picture = (width: number): Picture => ({ width, height: 1, data: new Uint8ClampedArray(width * 4) });
const thumb = picture(1);
const full = picture(4);

/** A fetch that answers `pictures`. */
const answering =
  (pictures: readonly [Picture | null, Picture | null]) =>
  async (): Promise<readonly [Picture | null, Picture | null]> =>
    Promise.resolve(pictures);

/** What `arrive` failed with, or null when it did not fail. */
async function failure(arriving: Promise<unknown>): Promise<string | null> {
  try {
    await arriving;
    return null;
  } catch (error: unknown) {
    return error instanceof Error ? error.message : String(error);
  }
}

describe('a screenshot is listed once its pixels arrive', () => {
  test('with both its thumbnail and its own pixels', async () => {
    expect(await arrive(3, answering([thumb, full]))).toEqual({ thumb, picture: full });
  });

  test('and not when either does not arrive or the fetch fails', async () => {
    const missing = "The screenshot's pixels did not arrive.";
    expect(await failure(arrive(3, answering([null, full])))).toBe(missing);
    expect(await failure(arrive(3, answering([thumb, null])))).toBe(missing);
    const stopped = async (): Promise<never> => Promise.reject(new Error('the owner stopped'));
    expect(await failure(arrive(3, stopped))).toBe('the owner stopped');
  });
});
