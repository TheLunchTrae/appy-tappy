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

/** The characters that have arrived, by {@link boxCharactersKey}. */
export type ShownByKey = ReadonlyMap<string, readonly ShownCharacter[]>;

/** `shown` with only what `live` names: the answers for what a box of the reference still shows. */
export function stillShown(shown: ShownByKey, live: ReadonlySet<string>): ShownByKey {
  return [...shown.keys()].every((key) => live.has(key)) ? shown : new Map([...shown].filter(([key]) => live.has(key)));
}

/**
 * `shown` with the `characters` that arrived for `key`. An answer for a key that `live` does not name is dropped:
 * the box it was asked for has moved or gone since, or the characters' color has changed, and what the box shows now
 * was asked for under its own key.
 */
export function arrived(
  shown: ShownByKey,
  live: ReadonlySet<string>,
  key: string,
  characters: readonly ShownCharacter[],
): ShownByKey {
  return live.has(key) ? new Map(shown).set(key, characters) : shown;
}

/**
 * The characters each of `boxes` shows in the characters' color `ink` within `tolerance`, by
 * {@link boxCharactersKey}, once they arrive. A box that moves, or a color that changes, is asked about anew, and an
 * answer for what no box shows anymore is dropped. A box that could not be asked about shows no characters, so the
 * check leaves it to the list of boxes instead of waiting for it.
 */
export function useBoxCharacters(boxes: readonly Box[], ink: Color | null, tolerance: number): ShownByKey {
  const [shown, setShown] = useState<ShownByKey>(new Map());
  // What was asked for and is still wanted, so nothing is asked for twice.
  const asked = useRef(new Set<string>());
  const wanted = ink === null ? [] : boxes.map((box) => ({ box, key: boxCharactersKey(box, ink, tolerance) }));
  const keys = wanted.map((each) => each.key).join(' ');
  const latest = useRef({ wanted, ink, tolerance, live: new Set<string>() });
  latest.current = { wanted, ink, tolerance, live: new Set(wanted.map((each) => each.key)) };
  useEffect(() => {
    const now = latest.current;
    asked.current = new Set([...asked.current].filter((key) => now.live.has(key)));
    setShown((current) => stillShown(current, now.live));
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
            setShown((current) => arrived(current, latest.current.live, key, characters));
          }),
      );
    }
  }, [keys]);
  return shown;
}
