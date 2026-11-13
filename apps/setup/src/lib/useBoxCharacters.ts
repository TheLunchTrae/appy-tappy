// The characters each box of the reference being taught shows, as the check of its characters needs them.

import { useEffect, useRef, useState } from 'react';

import type { ShownCharacter } from './bindings.ts';
import { boxCharacters } from './api.ts';
import type { Box } from './boxes.ts';
import type { Color } from './pixels.ts';
import { background } from './repeat.ts';

/** A name for what `box` shows in the characters' color `ink` within `tolerance`, which stays while none of them changes. */
export function boxCharactersKey(box: Pick<Box, 'screenshot' | 'region'>, ink: Color, tolerance: number): string {
  const { x, y, width, height } = box.region;
  return [box.screenshot, x, y, width, height, ink.red, ink.green, ink.blue, tolerance].join(',');
}

/**
 * The characters each of `boxes` shows in the characters' color `ink` within `tolerance`, by
 * {@link boxCharactersKey}, once they arrive. A box that moves, or a color that changes, is asked about anew, and an
 * answer for what no box shows anymore is held under its own name until the boxes next change. A box that could not
 * be asked about shows no characters, so the check has the user type it instead of waiting.
 */
export function useBoxCharacters(
  boxes: readonly Box[],
  ink: Color | null,
  tolerance: number,
): ReadonlyMap<string, readonly ShownCharacter[]> {
  const [shown, setShown] = useState<ReadonlyMap<string, readonly ShownCharacter[]>>(new Map());
  // What was asked for and is still wanted, so nothing is asked for twice.
  const asked = useRef(new Set<string>());
  const wanted = ink === null ? [] : boxes.map((box) => ({ box, key: boxCharactersKey(box, ink, tolerance) }));
  const keys = wanted.map((each) => each.key).join(' ');
  const latest = useRef({ wanted, ink, tolerance });
  latest.current = { wanted, ink, tolerance };
  useEffect(() => {
    const now = latest.current;
    const live = new Set(now.wanted.map((each) => each.key));
    asked.current = new Set([...asked.current].filter((key) => live.has(key)));
    setShown((current) =>
      [...current.keys()].every((key) => live.has(key))
        ? current
        : new Map([...current].filter(([key]) => live.has(key))),
    );
    if (now.ink === null) {
      return;
    }
    for (const { box, key } of now.wanted) {
      if (asked.current.has(key)) {
        continue;
      }
      asked.current.add(key);
      background(
        boxCharacters(box.screenshot, box.region, now.ink, now.tolerance)
          .catch((): readonly ShownCharacter[] => [])
          .then((characters) => {
            setShown((current) => new Map(current).set(key, characters));
          }),
      );
    }
  }, [keys]);
  return shown;
}
