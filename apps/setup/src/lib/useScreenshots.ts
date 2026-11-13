// The screenshots the capture screen took or recorded this session, and the one it shows.

import { useCallback, useEffect, useRef, useState } from 'react';

import { errorText, removeScreenshots, screenshot, takeScreenshot } from './api.ts';
import type { DraftShot } from './bindings.ts';
import { withShot } from './looks.ts';
import type { Picture } from './pixels.ts';
import type { Size } from './region.ts';
import { background } from './repeat.ts';

/**
 * A listed screenshot's thumbnail: on its way, there, or failed to load. A screenshot taken or recorded this session
 * is listed only once it is there; one restored from a draft is listed from the start, since the draft refers to it.
 */
export type Thumb =
  | { readonly kind: 'loading' }
  | { readonly kind: 'failed' }
  | { readonly kind: 'loaded'; readonly picture: Picture };

const LOADING: Thumb = { kind: 'loading' };
const FAILED: Thumb = { kind: 'failed' };

/** A screenshot kept this session. */
interface Shot {
  readonly index: number;
  readonly time: string;
  readonly thumb: Thumb;
  /** Whether a recording kept it, rather than the user taking it. */
  readonly recorded: boolean;
}

/**
 * `shots` with the thumbnail of the screenshot `index` as `thumb`; unchanged when it is not listed, such as once it
 * was removed.
 */
export function withThumb<Listed extends { readonly index: number; readonly thumb: Thumb }>(
  shots: readonly Listed[],
  index: number,
  thumb: Thumb,
): readonly Listed[] {
  return shots.some((shot) => shot.index === index)
    ? shots.map((shot) => (shot.index === index ? { ...shot, thumb } : shot))
    : shots;
}

/** Whether showing the screenshot `index` of `shots` loads it again: its pixels failed to load. */
export function loadsAgain(
  shots: readonly { readonly index: number; readonly thumb: Thumb }[],
  index: number,
): boolean {
  return shots.find((shot) => shot.index === index)?.thumb.kind === 'failed';
}

const THUMB_WIDTH = 224;
/** Wide enough for any window, so a screenshot arrives at its own pixels. */
const FULL_WIDTH = 16_384;

/** A screenshot taken: its index, and its window size. */
export interface Taken {
  readonly index: number;
  readonly size: Size;
}

/** A screenshot's pixels: small for its thumbnail, and at its own size. */
export interface Pixels {
  readonly thumb: Picture;
  readonly picture: Picture;
}

/** Fetches the screenshot `index` small for its thumbnail and at its own pixels; null for one that is gone. */
async function fetchShot(index: number): Promise<readonly [Picture | null, Picture | null]> {
  return Promise.all([screenshot(index, THUMB_WIDTH), screenshot(index, FULL_WIDTH)]);
}

/**
 * The pixels of the screenshot `index`, as `fetch` gets them. Rejects when either does not arrive, so a screenshot
 * is listed only once it has them.
 */
export async function arrive(
  index: number,
  fetch: (index: number) => Promise<readonly [Picture | null, Picture | null]> = fetchShot,
): Promise<Pixels> {
  const [thumb, picture] = await fetch(index);
  if (thumb === null || picture === null) {
    throw new Error("The screenshot's pixels did not arrive.");
  }
  return { thumb, picture };
}

/**
 * The screenshot `index` to list, with its pixels, once `fetch` gets both its thumbnail and its own pixels. When
 * either does not arrive, it calls `release` once and rejects, so a screenshot taken or recorded is listed only once
 * it has them.
 */
export async function arrived(
  index: number,
  recorded: boolean,
  release: (index: number) => void,
  fetch: (index: number) => Promise<readonly [Picture | null, Picture | null]> = fetchShot,
): Promise<{ readonly shot: Shot; readonly picture: Picture }> {
  const time = new Date().toLocaleTimeString();
  try {
    const { thumb, picture } = await arrive(index, fetch);
    return { shot: { index, time, thumb: { kind: 'loaded', picture: thumb }, recorded }, picture };
  } catch (failure: unknown) {
    release(index);
    throw failure;
  }
}

/**
 * The screenshots kept this session, and the one shown, or null while the live view shows. Those in `restored` came
 * back from a draft, so they are listed from the start, and one whose pixels fail to load loads again when it is
 * shown.
 */
export function useScreenshots(restored: readonly DraftShot[] = []): {
  /** In the order they were kept. */
  readonly shots: readonly Shot[];
  /** Each screenshot at its own pixels, once they arrive. */
  readonly full: ReadonlyMap<number, Picture>;
  readonly selected: number | null;
  /** Shows the screenshot `index`, or the live view for null, loading a restored one again whose pixels failed. */
  readonly setSelected: (index: number | null) => void;
  readonly error: string | null;
  /**
   * Takes a screenshot of the newest frame and lists it, and with `show` shows it at once; null when it failed. With
   * `hold`, what is shown stays, such as the live view under a drag.
   */
  readonly take: (how?: 'show' | 'hold') => Promise<Taken | null>;
  /** Lists the screenshot `index`, which a recording kept, without showing it. */
  readonly adopt: (index: number) => void;
  /** Removes the screenshots `indexes` and releases their pixels; the live view shows if one of them was shown. */
  readonly remove: (indexes: readonly number[]) => Promise<void>;
  /** The screenshot's number as the strip shows it, which it keeps when another is removed. */
  readonly position: (index: number) => number;
} {
  const [shots, setShots] = useState<readonly Shot[]>(() =>
    restored.map(({ index, recorded }) => ({ index, time: 'Draft', thumb: LOADING, recorded })),
  );
  const [full, setFull] = useState<ReadonlyMap<number, Picture>>(new Map());
  const [selected, setShown] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  // The screenshots removed, so pixels that arrive late for one are dropped.
  const removed = useRef<Set<number>>(new Set());
  // The screenshots as last rendered, so showing one can tell whether its pixels failed to load.
  const listed = useRef(shots);
  useEffect(() => {
    listed.current = shots;
  }, [shots]);

  // Loads the pixels of the restored screenshot `index`, which stays listed: the draft refers to it.
  const load = useCallback(async (index: number): Promise<void> => {
    setShots((current) => withThumb(current, index, LOADING));
    try {
      const { thumb, picture } = await arrive(index);
      if (!removed.current.has(index)) {
        setShots((current) => withThumb(current, index, { kind: 'loaded', picture: thumb }));
        setFull((current) => new Map(current).set(index, picture));
        setError(null);
      }
    } catch (failure: unknown) {
      if (!removed.current.has(index)) {
        setShots((current) => withThumb(current, index, FAILED));
        setError(errorText(failure));
      }
    }
  }, []);
  // Fetches the pixels of the screenshot `index` and then lists it. One whose pixels do not arrive is released and
  // never listed, so the strip shows no blank tile for it.
  const list = useCallback(async (index: number, recorded: boolean): Promise<Picture> => {
    const { shot, picture } = await arrived(index, recorded, (failed) => {
      removed.current.add(failed);
      background(removeScreenshots([failed]));
    });
    if (!removed.current.has(index)) {
      setShots((current) => withShot(current, shot));
      setFull((current) => new Map(current).set(index, picture));
    }
    return picture;
  }, []);

  // A draft's screenshots are this session's again; their pixels load once, and again when a failed one is shown.
  const [first] = useState(restored);
  useEffect(() => {
    for (const { index } of first) {
      background(load(index));
    }
  }, [first, load]);

  const setSelected = useCallback(
    (index: number | null): void => {
      setShown(index);
      if (index !== null && loadsAgain(listed.current, index)) {
        background(load(index));
      }
    },
    [load],
  );

  const take = useCallback(
    async (how: 'show' | 'hold' = 'show'): Promise<Taken | null> => {
      try {
        const index = await takeScreenshot();
        const picture = await list(index, false);
        if (how === 'show') {
          setShown(index);
        }
        setError(null);
        return { index, size: { width: picture.width, height: picture.height } };
      } catch (error: unknown) {
        setError(errorText(error));
        return null;
      }
    },
    [list],
  );

  const adopt = useCallback(
    (index: number): void => {
      list(index, true).catch((error: unknown) => {
        setError(errorText(error));
      });
    },
    [list],
  );

  const remove = useCallback(async (indexes: readonly number[]): Promise<void> => {
    try {
      await removeScreenshots(indexes);
      for (const index of indexes) {
        removed.current.add(index);
      }
      setShots((current) => current.filter((shot) => !indexes.includes(shot.index)));
      setFull((current) => new Map([...current].filter(([index]) => !indexes.includes(index))));
      setShown((current) => (current !== null && indexes.includes(current) ? null : current));
      setError(null);
    } catch (error: unknown) {
      setError(errorText(error));
    }
  }, []);

  const position = (index: number): number => index + 1;
  return { shots, full, selected, setSelected, error, take, adopt, remove, position };
}
