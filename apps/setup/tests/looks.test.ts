import { describe, expect, test } from 'bun:test';

import { type Draft, EMPTY_DRAFT, withState } from '../src/lib/check.ts';
import {
  keptSummary,
  keptText,
  listedKept,
  lookBlocked,
  lookTolerance,
  removableRecorded,
  removeBlocked,
  withShot,
} from '../src/lib/looks.ts';

const REGION = { x: 1, y: 2, width: 3, height: 4 };
const drawn: Draft = { ...EMPTY_DRAFT, kind: 'state', region: REGION, drawnOn: 0 };

describe('recording looks', () => {
  test('needs a region drawn on a screenshot', () => {
    const blocked = 'Draw the region on a screenshot to record it.';
    expect(lookBlocked({ ...drawn, region: null })).toBe(blocked);
    expect(lookBlocked({ ...drawn, drawnOn: null })).toBe(blocked);
    expect(lookBlocked(drawn)).toBeNull();
  });

  test("tells looks apart with a state check's tolerance, its own once it is one", () => {
    expect(lookTolerance({ ...drawn, tolerance: 4 })).toBe(4);
    // A bar's own tolerance is 0 and a color's compares colors, so neither is how alike two looks are.
    expect(lookTolerance({ ...drawn, kind: 'barFill', tolerance: 0 })).toBe(16);
    expect(lookTolerance({ ...drawn, kind: 'color', tolerance: 60 })).toBe(16);
  });

  test('counts what it kept', () => {
    expect([keptText(0), keptText(1), keptText(2)]).toEqual(['0 screenshots', '1 screenshot', '2 screenshots']);
  });

  test('reviews the kept screenshots the strip still lists, the first one first', () => {
    const strip = [
      { index: 0, recorded: false },
      { index: 3, recorded: true },
      { index: 4, recorded: true },
    ];
    // Screenshot 2 was removed, and the strip has not heard of 5 yet.
    expect(listedKept([2, 3, 4, 5], strip)).toEqual([3, 4]);
    expect(listedKept([2, 5], strip)).toEqual([]);
    expect(listedKept([], strip)).toEqual([]);
  });

  test('says what a stopped recording left, counted against the strip', () => {
    expect(keptSummary(4, 4, 65_000)).toBe('The last recording kept 4 screenshots in 1:05.');
    expect(keptSummary(1, 1, 0)).toBe('The last recording kept 1 screenshot in 0:00.');
    expect(keptSummary(4, 1, 65_000)).toBe("1 of the last recording's 4 screenshots is still in the strip.");
    expect(keptSummary(4, 0, 65_000)).toBe("0 of the last recording's 4 screenshots are still in the strip.");
  });
});

describe('removing a screenshot', () => {
  test('is refused while the region, a state, or a label uses it', () => {
    const used = { ...withState(drawn, 1), labels: new Map([[2, ' 87% ']]) };
    expect(removeBlocked(used, null, 0, false)).toBe(
      'The region is drawn on this screenshot. Draw it on another one to remove this one.',
    );
    expect(removeBlocked(used, null, 1, false)).toBe(
      'A state is taken from this screenshot. Remove the state to remove the screenshot.',
    );
    expect(removeBlocked(used, null, 2, false)).toBe('This screenshot is labeled. Clear what it shows to remove it.');
    expect(removeBlocked(used, null, 3, false)).toBeNull();
    // A label cleared again, or one of only spaces, labels nothing.
    expect(removeBlocked({ ...used, labels: new Map([[2, '  ']]) }, null, 2, false)).toBeNull();
  });

  test('is refused for a screenshot that added found text is on', () => {
    const found = { ...drawn, found: [{ screenshot: 4, region: REGION, text: '12' }] };
    expect(removeBlocked(found, null, 4, false)).toBe(
      'Text found on this screenshot is added. Remove found text to remove the screenshot.',
    );
    expect(removeBlocked(found, null, 5, false)).toBeNull();
    expect(
      removableRecorded(
        [
          { index: 4, recorded: true },
          { index: 5, recorded: true },
        ],
        found,
        null,
      ),
    ).toEqual([5]);
  });

  test("is refused for the value's region while the reference editor is open over it", () => {
    const editor = { ...drawn, drawnOn: 1 };
    const value = { region: REGION, drawnOn: 3 };
    expect(removeBlocked(editor, value, 3, false)).toBe(
      'The region is drawn on this screenshot. Draw it on another one to remove this one.',
    );
    expect(removeBlocked(editor, value, 2, false)).toBeNull();
    expect(removeBlocked(editor, { region: null, drawnOn: null }, 2, false)).toBeNull();
  });

  test('is refused for every screenshot while a recording of looks records', () => {
    const blocked = 'Stop recording to remove a screenshot.';
    expect(removeBlocked(drawn, null, 3, true)).toBe(blocked);
    expect(removeBlocked(drawn, null, 0, true)).toBe(blocked);
  });

  test('removes every recorded screenshot that nothing uses, and none the user took', () => {
    const shots = [
      { index: 0, recorded: false },
      { index: 1, recorded: true },
      { index: 2, recorded: true },
      { index: 3, recorded: false },
      { index: 4, recorded: true },
    ];
    expect(removableRecorded(shots, withState(drawn, 2), null)).toEqual([1, 4]);
    expect(removableRecorded(shots, { ...drawn, drawnOn: 4 }, { region: REGION, drawnOn: 1 })).toEqual([2]);
  });
});

describe('the strip', () => {
  test('lists screenshots in the order the session kept them, each once', () => {
    const taken = [
      { index: 0, recorded: false },
      { index: 3, recorded: false },
    ];
    // A recorded screenshot the strip hears of after one taken later.
    const late = withShot(taken, { index: 2, recorded: true });
    expect(late.map((shot) => shot.index)).toEqual([0, 2, 3]);
    expect(withShot(late, { index: 2, recorded: false })).toBe(late);
    expect(withShot(late, { index: 4, recorded: true }).map((shot) => shot.index)).toEqual([0, 2, 3, 4]);
  });
});
