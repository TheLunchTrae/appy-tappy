import { describe, expect, test } from 'bun:test';

import { arrived, boxCharactersKey, stillShown } from '../src/lib/useBoxCharacters.ts';

const at = (x: number) => ({ screenshot: 0, region: { x, y: 0, width: 9, height: 4 } });
const characters = [{ shape: 's', left: 0, right: 3 }];

describe("the characters a reference's boxes show", () => {
  test('arrive under the key they were asked for, which the box makes', () => {
    const key = boxCharactersKey(at(0));
    expect(arrived(new Map(), new Set([key]), key, characters)).toEqual(new Map([[key, characters]]));
    const keys = [at(1), { ...at(0), screenshot: 1 }].map((box) => boxCharactersKey(box));
    expect(new Set([key, ...keys]).size).toBe(3);
  });

  test('an answer for where a box was before it moved is dropped, so the moved box waits for its own', () => {
    const before = boxCharactersKey(at(0));
    const moved = boxCharactersKey(at(5));
    const shown = new Map<string, typeof characters>();
    const late = arrived(shown, new Set([moved]), before, characters);
    expect(late).toBe(shown);
    expect(late.get(moved)).toBeUndefined();
  });

  test('what no box shows anymore is forgotten when the boxes change', () => {
    const [one, other] = [at(0), at(5)].map((box) => boxCharactersKey(box));
    const shown = new Map([
      [one ?? '', characters],
      [other ?? '', characters],
    ]);
    expect(stillShown(shown, new Set([other ?? '']))).toEqual(new Map([[other ?? '', characters]]));
    expect(stillShown(shown, new Set([one ?? '', other ?? '']))).toBe(shown);
  });
});
