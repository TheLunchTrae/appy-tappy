// The screenshots the capture screen took or recorded this session, and the one it shows.

import { useCallback, useEffect, useRef, useState } from 'react';

import { errorText, removeScreenshots, screenshot, takeScreenshot } from './api.ts';
import { withShot } from './looks.ts';
import type { Picture } from './pixels.ts';
import type { Size } from './region.ts';
import { background } from './repeat.ts';

/** A screenshot kept this session. */
interface Shot {
  readonly index: number;
  readonly time: string;
  readonly thumb: Picture | null;
  /** Whether a recording kept it, rather than the user taking it. */
  readonly recorded: boolean;
}

const THUMB_WIDTH = 224;
/** Wide enough for any window, so a screenshot arrives at its own pixels. */
const FULL_WIDTH = 16_384;

/** A screenshot taken: its index, and its window size, or null when its pixels did not arrive. */
export interface Taken {
  readonly index: number;
  readonly size: Size | null;
}

/** A screenshot a draft kept, which is the capture's again: its index, and whether a recording had kept it. */
export interface RestoredShot {
  readonly index: number;
  readonly recorded: boolean;
}

/** Fetches the screenshot `index` small for its thumbnail and at its own pixels. */
async function fetchShot(index: number): Promise<readonly [Picture | null, Picture | null]> {
  return Promise.all([screenshot(index, THUMB_WIDTH), screenshot(index, FULL_WIDTH)]);
}

/**
 * The screenshots kept this session, and the one shown, or null while the live view shows. Those in `restored` came
 * back from a draft, so they are listed from the start.
 */
export function useScreenshots(restored: readonly RestoredShot[] = []): {
  /** In the order they were kept. */
  readonly shots: readonly Shot[];
  /** Each screenshot at its own pixels, once they arrive. */
  readonly full: ReadonlyMap<number, Picture>;
  readonly selected: number | null;
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
    restored.map(({ index, recorded }) => ({ index, time: 'Draft', thumb: null, recorded })),
  );
  const [full, setFull] = useState<ReadonlyMap<number, Picture>>(new Map());
  const [selected, setSelected] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  // The screenshots removed, so pixels that arrive late for one are dropped.
  const removed = useRef<Set<number>>(new Set());

  // Takes the pixels of the listed screenshot `index`, unless it was removed meanwhile.
  const keep = useCallback((index: number, thumb: Picture | null, picture: Picture | null): void => {
    if (removed.current.has(index)) {
      return;
    }
    setShots((current) => current.map((shot) => (shot.index === index ? { ...shot, thumb } : shot)));
    if (picture !== null) {
      setFull((current) => new Map(current).set(index, picture));
    }
  }, []);
  // Lists the screenshot `index` and fetches its pixels.
  const list = useCallback(
    async (index: number, recorded: boolean): Promise<Picture | null> => {
      const time = new Date().toLocaleTimeString();
      setShots((current) => withShot(current, { index, time, thumb: null, recorded }));
      const [thumb, picture] = await fetchShot(index);
      keep(index, thumb, picture);
      return picture;
    },
    [keep],
  );

  // A draft's screenshots are this session's again; their pixels load once.
  const [first] = useState(restored);
  useEffect(() => {
    const load = async (): Promise<void> => {
      try {
        await Promise.all(
          first.map(async ({ index }) => {
            const [thumb, picture] = await fetchShot(index);
            keep(index, thumb, picture);
          }),
        );
      } catch (failure: unknown) {
        setError(errorText(failure));
      }
    };
    background(load());
  }, [first, keep]);

  const take = useCallback(
    async (how: 'show' | 'hold' = 'show'): Promise<Taken | null> => {
      try {
        const index = await takeScreenshot();
        if (how === 'show') {
          setSelected(index);
        }
        setError(null);
        const picture = await list(index, false);
        return { index, size: picture === null ? null : { width: picture.width, height: picture.height } };
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
      setSelected((current) => (current !== null && indexes.includes(current) ? null : current));
      setError(null);
    } catch (error: unknown) {
      setError(errorText(error));
    }
  }, []);

  const position = (index: number): number => index + 1;
  return { shots, full, selected, setSelected, error, take, adopt, remove, position };
}
