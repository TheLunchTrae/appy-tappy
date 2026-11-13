// The boxes of the reference being taught as the stage and the list change them, and the one that is selected.

import { type Dispatch, type SetStateAction, useRef, useState } from 'react';

import { type Box, type BoxEvent, type BoxState, type NewBox, boxNumber, boxOf, boxStep, boxesOn } from './boxes.ts';
import type { Draft } from './check.ts';
import type { Rect } from './region.ts';

/**
 * The boxes of `draft`, and the selected one, which the stage moves and Find more like it samples. `shown` is the
 * screenshot the stage shows, or null for the live window. Every change to a box goes through here, as an event to `boxStep`.
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
  /**
   * Takes a region the stage's drag made: the selected box adjusted, or a new box, which it selects. Returns the
   * new box's ID, or null when it made none.
   */
  readonly draw: (region: Rect, made: 'drawn' | 'adjusted') => number | null;
  readonly retype: (id: number, text: string) => void;
  readonly remove: (id: number) => void;
  /** Adds found text, each find as a box. */
  readonly add: (added: readonly NewBox[]) => void;
  /** The box a new recording reads at, or null when it reads at none. */
  readonly recordedId: number | null;
  /** Notes that a new recording started at box `id`, or at none. */
  readonly recorded: (id: number | null) => void;
  /** Puts the box the recording read at back at `region` on `screenshot`, where the recording read. */
  readonly restore: (screenshot: number, region: Rect) => void;
} {
  const [selectedId, setSelected] = useState<number | null>(null);
  const [recordedId, setRecorded] = useState<number | null>(null);
  const { boxes } = draft;
  // The boxes and the selection as the last event left them, which the next event before a render goes on from.
  const latest = useRef<BoxState>({ boxes, selectedId, recordedId });
  latest.current = { boxes, selectedId, recordedId };
  const send = (event: BoxEvent): BoxState => {
    const { selectedId: selectedBefore, recordedId: recordedBefore } = latest.current;
    const next = boxStep(latest.current, event);
    latest.current = next;
    // The draft's own boxes take the event, so a change still queued is kept.
    setDraft((current) => {
      const changed = boxStep(
        { boxes: current.boxes, selectedId: selectedBefore, recordedId: recordedBefore },
        event,
      ).boxes;
      return changed === current.boxes ? current : { ...current, boxes: changed };
    });
    setSelected(next.selectedId);
    setRecorded(next.recordedId);
    return next;
  };
  const onShown = boxOf(boxesOn(boxes, shown), selectedId);
  return {
    selectedId,
    selected: boxOf(boxes, selectedId),
    onShown,
    others: boxesOn(boxes, shown)
      .filter((box) => box.id !== onShown?.id)
      .map((box) => ({ id: box.id, rect: box.region, label: `${boxNumber(boxes, box.id)}` })),
    select: (id) => {
      send({ type: 'select', id });
    },
    draw: (region, made) => {
      const before = latest.current;
      const next = send({ type: 'region', shown, region, made });
      return next.boxes.length > before.boxes.length ? next.selectedId : null;
    },
    retype: (id, text) => {
      send({ type: 'retype', id, text });
    },
    remove: (id) => {
      send({ type: 'remove', id });
    },
    add: (added) => {
      send({ type: 'add', boxes: added });
    },
    recordedId,
    recorded: (id) => {
      send({ type: 'recorded', id });
    },
    restore: (screenshot, region) => {
      send({ type: 'restore', screenshot, region });
    },
  };
}
