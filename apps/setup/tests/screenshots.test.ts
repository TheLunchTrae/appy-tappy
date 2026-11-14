import { describe, expect, test } from 'bun:test';

import type { Picture } from '../src/lib/pixels.ts';
import { arrived } from '../src/lib/useScreenshots.ts';

const picture = (width: number): Picture => ({ width, height: 1, data: new Uint8ClampedArray(width * 4) });
const thumb = picture(1);
const full = picture(4);

/** A fetch that answers `pictures`. */
const answering =
  (pictures: readonly [Picture | null, Picture | null]) =>
  async (): Promise<readonly [Picture | null, Picture | null]> =>
    Promise.resolve(pictures);

/** What a screenshot that arrived with `fetch` lists, or the failure, and the screenshots it released. */
async function arriving(
  fetch: (index: number) => Promise<readonly [Picture | null, Picture | null]>,
): Promise<{ readonly listed: unknown; readonly released: readonly number[] }> {
  const released: number[] = [];
  const release = (index: number): void => {
    released.push(index);
  };
  try {
    const { shot, picture: own } = await arrived(3, true, release, fetch);
    return { listed: { index: shot.index, thumb: shot.thumb, recorded: shot.recorded, own }, released };
  } catch (error: unknown) {
    return { listed: error instanceof Error ? error.message : String(error), released };
  }
}

describe('a screenshot is listed once its pixels arrive', () => {
  test('with both its thumbnail and its own pixels, and keeps them', async () => {
    expect(await arriving(answering([thumb, full]))).toEqual({
      listed: { index: 3, thumb, recorded: true, own: full },
      released: [],
    });
  });

  test('and is released once and never listed when either does not arrive or the fetch fails', async () => {
    const missing = "The screenshot's pixels did not arrive.";
    expect(await arriving(answering([null, full]))).toEqual({ listed: missing, released: [3] });
    expect(await arriving(answering([thumb, null]))).toEqual({ listed: missing, released: [3] });
    const stopped = async (): Promise<never> => Promise.reject(new Error('the owner stopped'));
    expect(await arriving(stopped)).toEqual({ listed: 'the owner stopped', released: [3] });
  });
});
