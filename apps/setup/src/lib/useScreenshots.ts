// The screenshots the capture screen took or recorded this session, and the one it shows.

import { useCallback, useRef, useState } from 'react';

import { errorText, removeScreenshots, screenshot, takeScreenshot } from './api.ts';
import { withShot } from './looks.ts';
import type { Picture } from './pixels.ts';

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

/** The screenshots kept this session, and the one shown, or null while the live view shows. */
export function useScreenshots(): {
  /** In the order they were kept. */
  readonly shots: readonly Shot[];
  /** Each screenshot at its own pixels, once they arrive. */
  readonly full: ReadonlyMap<number, Picture>;
  readonly selected: number | null;
  readonly setSelected: (index: number | null) => void;
  readonly error: string | null;
  /** Takes a screenshot of the newest frame and shows it. */
  readonly take: () => Promise<void>;
  /** Lists the screenshot `index`, which a recording kept, without showing it. */
  readonly adopt: (index: number) => void;
  /** Removes the screenshots `indexes` and releases their pixels; the live view shows if one of them was shown. */
  readonly remove: (indexes: readonly number[]) => Promise<void>;
  /** The screenshot's number as the strip shows it, which it keeps when another is removed. */
  readonly position: (index: number) => number;
} {
  const [shots, setShots] = useState<readonly Shot[]>([]);
  const [full, setFull] = useState<ReadonlyMap<number, Picture>>(new Map());
  const [selected, setSelected] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  // The screenshots removed, so pixels that arrive late for one are dropped.
  const removed = useRef<Set<number>>(new Set());

  // Lists the screenshot `index` and fetches its pixels.
  const list = useCallback(async (index: number, recorded: boolean): Promise<void> => {
    const time = new Date().toLocaleTimeString();
    setShots((current) => withShot(current, { index, time, thumb: null, recorded }));
    const [thumb, picture] = await Promise.all([screenshot(index, THUMB_WIDTH), screenshot(index, FULL_WIDTH)]);
    if (removed.current.has(index)) {
      return;
    }
    setShots((current) => current.map((shot) => (shot.index === index ? { ...shot, thumb } : shot)));
    if (picture !== null) {
      setFull((current) => new Map(current).set(index, picture));
    }
  }, []);

  const take = useCallback(async (): Promise<void> => {
    try {
      const index = await takeScreenshot();
      setSelected(index);
      setError(null);
      await list(index, false);
    } catch (error: unknown) {
      setError(errorText(error));
    }
  }, [list]);

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
