import { describe, expect, test } from 'bun:test';

import type { Picture } from '../src/lib/pixels.ts';
import { arrive, arrived, loadEnded, loadsAgain, type Strip, type Thumb } from '../src/lib/useScreenshots.ts';

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

describe('a screenshot taken or recorded is listed only once it arrives', () => {
  test('with both its thumbnail and its own pixels, and keeps them', async () => {
    expect(await arriving(answering([thumb, full]))).toEqual({
      listed: { index: 3, thumb: { kind: 'loaded', picture: thumb }, recorded: true, own: full },
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

describe("a draft's screenshot stays listed while its pixels load", () => {
  const loading: Thumb = { kind: 'loading' };
  const failed: Thumb = { kind: 'failed' };
  const loaded: Thumb = { kind: 'loaded', picture: thumb };
  const shot = (index: number, shown: Thumb) => ({ index, time: 'Draft', thumb: shown, recorded: false });
  const strip: Strip = { shots: [shot(0, loading), shot(2, loaded)], full: new Map([[2, full]]), error: null };
  const arrivedPixels = { kind: 'arrived', pixels: { thumb, picture: full } } as const;
  const stopped = { kind: 'failed', reason: 'the owner stopped' } as const;

  test('shows its thumbnail and keeps its own pixels once they arrive', () => {
    const shown = loadEnded({ ...strip, error: 'an older failure' }, 0, arrivedPixels);
    expect(shown.shots).toEqual([shot(0, loaded), shot(2, loaded)]);
    expect([...shown.full]).toEqual([
      [2, full],
      [0, full],
    ]);
    expect(shown.error).toBeNull();
  });

  test('is marked when its pixels fail to load, keeping its place, and says why', () => {
    // The draft refers to it by its index, which stays.
    expect(loadEnded(strip, 0, stopped)).toEqual({
      shots: [shot(0, failed), shot(2, loaded)],
      full: strip.full,
      error: 'the owner stopped',
    });
  });

  test('drops a load that ends after the screenshot was removed', () => {
    const left: Strip = { ...strip, shots: [shot(2, loaded)] };
    expect(loadEnded(left, 0, arrivedPixels)).toBe(left);
    expect(loadEnded(left, 0, stopped)).toBe(left);
  });

  test('loads again when it is shown after its pixels failed, once until that load ends', () => {
    const shots = [shot(0, failed), shot(2, loaded), shot(3, loading)];
    expect(loadsAgain(shots, new Set(), 0)).toBe(true);
    expect(loadsAgain(shots, new Set(), 2)).toBe(false);
    expect(loadsAgain(shots, new Set(), 3)).toBe(false);
    expect(loadsAgain(shots, new Set(), 5)).toBe(false);
    // A second show before the strip shows the first load starts no second load.
    expect(loadsAgain(shots, new Set([0]), 0)).toBe(false);
  });
});
