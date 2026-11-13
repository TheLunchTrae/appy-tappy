// The boxes of the reference being taught as the stage and the list change them, and the one that is selected.

import { type Dispatch, type SetStateAction, useState } from 'react';

import { type Box, boxNumber, boxOf, boxesOn, changedBox, withBoxes, withoutBox } from './boxes.ts';
import type { Draft } from './check.ts';
import type { Rect } from './region.ts';

/**
 * The boxes of `draft`, and the selected one, which the stage moves and Find more like it samples. `shown` is the
 * screenshot the stage shows, or null for the live window.
 */
export function useBoxes(
  draft: Draft,
  setDraft: Dispatch<SetStateAction<Draft>>,
  shown: number | null,
): {
  readonly selectedId: number | null;
  /** The selected box, or null when none is, such as after it was deleted. */
  readonly selected: Box | null;
  /** The selected box when it is on the screenshot shown, which the stage draws as its region. */
  readonly onShown: Box | null;
  /** The other boxes on the screenshot shown, each with its number as its label. */
  readonly others: readonly { readonly id: number; readonly rect: Rect; readonly label: string }[];
  readonly select: (id: number | null) => void;
  /** Takes a region the stage's drag made: the selected box adjusted, or a new box, which it selects. */
  readonly draw: (region: Rect, made: 'drawn' | 'adjusted') => void;
  readonly retype: (id: number, text: string) => void;
  readonly remove: (id: number) => void;
} {
  const [selectedId, select] = useState<number | null>(null);
  const { boxes } = draft;
  const onShown = boxOf(boxesOn(boxes, shown), selectedId);
  return {
    selectedId,
    selected: boxOf(boxes, selectedId),
    onShown,
    others: boxesOn(boxes, shown)
      .filter((box) => box.id !== onShown?.id)
      .map((box) => ({ id: box.id, rect: box.region, label: `${boxNumber(boxes, box.id)}` })),
    select,
    draw: (region, made) => {
      if (shown === null) {
        return;
      }
      if (made === 'adjusted' && onShown !== null) {
        setDraft((current) => ({ ...current, boxes: changedBox(current.boxes, onShown.id, { region }) }));
        return;
      }
      const added = withBoxes(boxes, [{ screenshot: shown, region, text: '' }]);
      setDraft((current) => ({ ...current, boxes: added.boxes }));
      select(added.first);
    },
    retype: (id, text) => {
      setDraft((current) => ({ ...current, boxes: changedBox(current.boxes, id, { text }) }));
    },
    remove: (id) => {
      setDraft((current) => ({ ...current, boxes: withoutBox(current.boxes, id) }));
      if (selectedId === id) {
        select(null);
      }
    },
  };
}
