import { describe, expect, test } from 'bun:test';

import { type BoxEvent, type BoxState, boxLabels, boxNumber, boxOf, boxStep, boxesOn } from '../src/lib/boxes.ts';

const at = (x: number) => ({ x, y: 0, width: 10, height: 8 });
const NONE: BoxState = { boxes: [], selectedId: null };

/** The boxes and the selection after `events`, from `state`. */
function after(state: BoxState, ...events: readonly BoxEvent[]): BoxState {
  return events.reduce(boxStep, state);
}

/** Boxes 0 and 1 on screenshot 0 and box 2 on screenshot 1, with `selectedId` selected. */
function three(selectedId: number | null): BoxState {
  return {
    boxes: [
      { id: 0, screenshot: 0, region: at(0), text: '1' },
      { id: 1, screenshot: 0, region: at(20), text: '2' },
      { id: 2, screenshot: 1, region: at(40), text: '3' },
    ],
    selectedId,
  };
}

describe("a reference's boxes", () => {
  test('a drawn region adds a box on the screenshot shown and selects it', () => {
    const drawn = after(three(1), { type: 'region', shown: 1, region: at(60), made: 'drawn' });
    expect(drawn.boxes).toEqual([...three(1).boxes, { id: 3, screenshot: 1, region: at(60), text: '' }]);
    expect(drawn.selectedId).toBe(3);
    // Over the live view there is no screenshot for a box to be on.
    expect(after(three(1), { type: 'region', shown: null, region: at(60), made: 'drawn' })).toEqual(three(1));
  });

  test('an adjusted region changes only the selected box on the screenshot shown', () => {
    const moved = after(three(1), { type: 'region', shown: 0, region: at(30), made: 'adjusted' });
    expect(moved.boxes).toEqual(three(1).boxes.with(1, { id: 1, screenshot: 0, region: at(30), text: '2' }));
    expect(moved.selectedId).toBe(1);
  });

  test('an adjusted region with no box selected on the screenshot shown adds one', () => {
    // Box 2 is selected, but it is on another screenshot than the one shown.
    for (const selectedId of [null, 2]) {
      const added = after(three(selectedId), { type: 'region', shown: 0, region: at(30), made: 'adjusted' });
      expect(added.boxes).toEqual([...three(null).boxes, { id: 3, screenshot: 0, region: at(30), text: '' }]);
      expect(added.selectedId).toBe(3);
    }
  });

  test('deleting the selected box clears the selection, and deleting another keeps it', () => {
    expect(after(three(1), { type: 'remove', id: 1 })).toEqual({
      boxes: three(1).boxes.filter((box) => box.id !== 1),
      selectedId: null,
    });
    expect(after(three(1), { type: 'remove', id: 0 }).selectedId).toBe(1);
  });

  test('a new box takes an ID no other box has, after a deletion and within one batch of events too', () => {
    // Two drags before anything is drawn again each go on from the boxes the last one left.
    const two = after(
      NONE,
      { type: 'region', shown: 0, region: at(0), made: 'drawn' },
      { type: 'region', shown: 0, region: at(20), made: 'drawn' },
    );
    expect(two.boxes.map((box) => box.id)).toEqual([0, 1]);
    // Deleting a box in the middle leaves its ID unused, and the next box goes past the highest.
    const again = after(
      three(null),
      { type: 'remove', id: 1 },
      {
        type: 'add',
        boxes: [
          { screenshot: 1, region: at(60), text: '5' },
          { screenshot: 1, region: at(80), text: '6' },
        ],
      },
    );
    expect(again.boxes.map((box) => box.id)).toEqual([0, 2, 3, 4]);
    expect(again.selectedId).toBeNull();
    expect(boxNumber(again.boxes, 3)).toBe(3);
  });

  test("typing changes one box's text, and selecting changes no box", () => {
    const typed = after(three(null), { type: 'retype', id: 2, text: '30' }, { type: 'select', id: 2 });
    expect(typed.boxes).toEqual(three(null).boxes.with(2, { id: 2, screenshot: 1, region: at(40), text: '30' }));
    expect(typed.selectedId).toBe(2);
  });

  test('restoring puts the box a recording read at back there, first in the list, with its text', () => {
    const restored = after(three(null), { type: 'restore', id: 1, screenshot: 1, region: at(90) });
    expect(restored.boxes).toEqual([
      { id: 1, screenshot: 1, region: at(90), text: '2' },
      { id: 0, screenshot: 0, region: at(0), text: '1' },
      { id: 2, screenshot: 1, region: at(40), text: '3' },
    ]);
    expect(restored.selectedId).toBe(1);
  });

  test('restoring a box that was deleted puts a new one with no text first', () => {
    for (const id of [9, null]) {
      const restored = after(three(0), { type: 'restore', id, screenshot: 1, region: at(90) });
      expect(restored.boxes).toEqual([{ id: 3, screenshot: 1, region: at(90), text: '' }, ...three(0).boxes]);
      expect(restored.selectedId).toBe(3);
    }
  });

  test('finds one box by its ID, and the boxes on one screenshot', () => {
    const { boxes } = three(null);
    expect(boxOf(boxes, 1)).toEqual({ id: 1, screenshot: 0, region: at(20), text: '2' });
    expect(boxOf(boxes, 9)).toBeNull();
    expect(boxOf(boxes, null)).toBeNull();
    expect(boxesOn(boxes, 1)).toEqual([{ id: 2, screenshot: 1, region: at(40), text: '3' }]);
    expect(boxesOn(boxes, null)).toEqual([]);
  });

  test('teach what each box shows, trimmed, at its own region, in list order, skipping blank ones', () => {
    const boxes = [
      { id: 0, screenshot: 2, region: at(0), text: ' 87% ' },
      { id: 1, screenshot: 0, region: at(20), text: '  ' },
      { id: 2, screenshot: 0, region: at(40), text: '12' },
    ];
    expect(boxLabels(boxes)).toEqual([
      { screenshot: 2, region: at(0), text: '87%' },
      { screenshot: 0, region: at(40), text: '12' },
    ]);
  });
});
