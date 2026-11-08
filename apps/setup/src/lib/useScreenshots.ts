import { useCallback, useState } from 'react';

import { errorText, screenshot, takeScreenshot } from './api.ts';
import type { Picture } from './pixels.ts';

/** A screenshot taken this session. */
interface Shot {
  readonly index: number;
  readonly time: string;
  readonly thumb: Picture | null;
}

const THUMB_WIDTH = 224;
/** Wide enough for any window, so a screenshot arrives at its own pixels. */
const FULL_WIDTH = 16_384;

/** The screenshots taken this session, and the one shown, or null while the live view shows. */
export function useScreenshots(): {
  /** In the order they were taken. */
  readonly shots: readonly Shot[];
  /** Each screenshot at its own pixels, once they arrive. */
  readonly full: ReadonlyMap<number, Picture>;
  readonly selected: number | null;
  readonly setSelected: (index: number | null) => void;
  readonly error: string | null;
  /** Takes a screenshot of the newest frame and shows it. */
  readonly take: () => Promise<void>;
  /** The screenshot's number as the strip shows it. */
  readonly position: (index: number) => number;
} {
  const [shots, setShots] = useState<readonly Shot[]>([]);
  const [full, setFull] = useState<ReadonlyMap<number, Picture>>(new Map());
  const [selected, setSelected] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const take = useCallback(async (): Promise<void> => {
    try {
      const index = await takeScreenshot();
      const time = new Date().toLocaleTimeString();
      setShots((current) => [...current, { index, time, thumb: null }]);
      setSelected(index);
      setError(null);
      const [thumb, picture] = await Promise.all([screenshot(index, THUMB_WIDTH), screenshot(index, FULL_WIDTH)]);
      setShots((current) => current.map((shot) => (shot.index === index ? { ...shot, thumb } : shot)));
      if (picture !== null) {
        setFull((current) => new Map(current).set(index, picture));
      }
    } catch (error: unknown) {
      setError(errorText(error));
    }
  }, []);
  const position = (index: number): number => shots.findIndex((shot) => shot.index === index) + 1;
  return { shots, full, selected, setSelected, error, take, position };
}
