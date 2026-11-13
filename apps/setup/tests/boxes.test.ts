import { describe, expect, test } from 'bun:test';

import { boxLabels, boxNumber, boxOf, boxesOn, changedBox, withBoxes, withoutBox } from '../src/lib/boxes.ts';

const at = (x: number) => ({ x, y: 0, width: 10, height: 8 });

describe('boxes', () => {
  test('a new box takes an ID no other box has, after a deletion too', () => {
    const first = withBoxes([], [{ screenshot: 0, region: at(0), text: '' }]);
    expect(first.first).toBe(0);
    const two = withBoxes(first.boxes, [
      { screenshot: 1, region: at(20), text: '5' },
      { screenshot: 1, region: at(40), text: '6' },
    ]);
    expect(two.first).toBe(1);
    expect(two.boxes.map((box) => box.id)).toEqual([0, 1, 2]);
    // Deleting a box in the middle leaves its ID unused, and the next box goes past the highest.
    const after = withBoxes(withoutBox(two.boxes, 1), [{ screenshot: 0, region: at(60), text: '' }]);
    expect(after.boxes.map((box) => box.id)).toEqual([0, 2, 3]);
    expect(boxNumber(after.boxes, 3)).toBe(3);
  });

  test('changes, finds and deletes one box by its ID', () => {
    const { boxes } = withBoxes(
      [],
      [
        { screenshot: 0, region: at(0), text: '1' },
        { screenshot: 1, region: at(20), text: '2' },
      ],
    );
    const second = { id: 1, screenshot: 1, region: at(20), text: '2' };
    expect(changedBox(boxes, 1, { region: at(30) })).toEqual([
      { id: 0, screenshot: 0, region: at(0), text: '1' },
      { ...second, region: at(30) },
    ]);
    expect(boxOf(boxes, 1)).toEqual(second);
    expect(boxOf(boxes, 9)).toBeNull();
    expect(boxOf(boxes, null)).toBeNull();
    expect(boxesOn(boxes, 1)).toEqual([second]);
    expect(boxesOn(boxes, null)).toEqual([]);
  });

  test('teach what each box shows, trimmed, at its own region, in list order, skipping blank ones', () => {
    const { boxes } = withBoxes(
      [],
      [
        { screenshot: 2, region: at(0), text: ' 87% ' },
        { screenshot: 0, region: at(20), text: '  ' },
        { screenshot: 0, region: at(40), text: '12' },
      ],
    );
    expect(boxLabels(boxes)).toEqual([
      { screenshot: 2, region: at(0), text: '87%' },
      { screenshot: 0, region: at(40), text: '12' },
    ]);
  });
});
