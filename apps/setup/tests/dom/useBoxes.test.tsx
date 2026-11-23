import { afterEach, expect, test } from 'bun:test';
import { useState } from 'react';
import { act, cleanup, renderHook } from '@testing-library/react';

import { EMPTY_DRAFT } from '../../src/lib/check.ts';
import { useBoxes } from '../../src/lib/useBoxes.ts';

afterEach(cleanup);

test('a box drawn over the live window goes on the screenshot its press took, and is selected', () => {
  const { result } = renderHook(() => {
    const [draft, setDraft] = useState(EMPTY_DRAFT);
    // The live window shows, so no screenshot does.
    return { draft, boxes: useBoxes(draft, setDraft, null) };
  });
  const region = { x: 2, y: 3, width: 10, height: 6 };
  act(() => {
    // On the live window alone, a drag makes no box.
    expect(result.current.boxes.draw(region, 'drawn')).toBeNull();
  });
  let drawn: number | null = null;
  act(() => {
    drawn = result.current.boxes.draw(region, 'drawn', 4);
  });
  expect(result.current.draft.boxes.map((box) => [box.screenshot, box.region])).toEqual([[4, region]]);
  expect(result.current.boxes.selectedId).toBe(drawn);
});
