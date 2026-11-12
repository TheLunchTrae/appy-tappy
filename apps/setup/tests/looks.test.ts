import { describe, expect, test } from 'bun:test';

import { type Draft, EMPTY_DRAFT, withState } from '../src/lib/check.ts';
import { keptText, lookBlocked, lookTolerance, removableRecorded, removeBlocked, withShot } from '../src/lib/looks.ts';

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
});

describe('removing a screenshot', () => {
  test('is refused while the region, a state, or a label uses it', () => {
    const used = { ...withState(drawn, 1), labels: new Map([[2, ' 87% ']]) };
    expect(removeBlocked(used, null, 0)).toBe(
      'The region is drawn on this screenshot. Draw it on another one to remove this one.',
    );
    expect(removeBlocked(used, null, 1)).toBe(
      'A state is taken from this screenshot. Remove the state to remove the screenshot.',
    );
    expect(removeBlocked(used, null, 2)).toBe('This screenshot is labeled. Clear what it shows to remove it.');
    expect(removeBlocked(used, null, 3)).toBeNull();
    // A label cleared again, or one of only spaces, labels nothing.
    expect(removeBlocked({ ...used, labels: new Map([[2, '  ']]) }, null, 2)).toBeNull();
  });

  test("is refused for the value's region while the reference editor is open over it", () => {
    const editor = { ...drawn, drawnOn: 1 };
    const value = { region: REGION, drawnOn: 3 };
    expect(removeBlocked(editor, value, 3)).toBe(
      'The region is drawn on this screenshot. Draw it on another one to remove this one.',
    );
    expect(removeBlocked(editor, value, 2)).toBeNull();
    expect(removeBlocked(editor, { region: null, drawnOn: null }, 2)).toBeNull();
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
