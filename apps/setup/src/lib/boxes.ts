// The boxes a reference learns from: each a part of one screenshot that shows characters, with what it shows.

import type { Label } from './bindings.ts';
import type { Rect } from './region.ts';

/** A part of a screenshot that shows characters, and what it shows as typed. */
export interface Box {
  /** Its own ID, which no other box of the draft has. */
  readonly id: number;
  readonly screenshot: number;
  readonly region: Rect;
  readonly text: string;
}

/** A box before it has an ID: a region of a screenshot, and its text. */
export type NewBox = Omit<Box, 'id'>;

/** `boxes` with boxes at `added`, and the first new box's ID. */
function withBoxes(
  boxes: readonly Box[],
  added: readonly NewBox[],
): { readonly boxes: readonly Box[]; readonly first: number } {
  const first = boxes.reduce((highest, box) => Math.max(highest, box.id), -1) + 1;
  return { boxes: [...boxes, ...added.map((box, offset) => ({ ...box, id: first + offset }))], first };
}

/** `boxes` with the box `id` changed by `change`. */
function changedBox(boxes: readonly Box[], id: number, change: Partial<Omit<Box, 'id'>>): readonly Box[] {
  return boxes.map((box) => (box.id === id ? { ...box, ...change } : box));
}

/** The boxes on screenshot `screenshot`. */
export function boxesOn(boxes: readonly Box[], screenshot: number | null): readonly Box[] {
  return boxes.filter((box) => box.screenshot === screenshot);
}

/** The box `id`, or null when there is none, such as after it was deleted. */
export function boxOf(boxes: readonly Box[], id: number | null): Box | null {
  return boxes.find((box) => box.id === id) ?? null;
}

/** The box's number as the list shows it, counted from 1. */
export function boxNumber(boxes: readonly Box[], id: number): number {
  return boxes.findIndex((box) => box.id === id) + 1;
}

/** The boxes that show something, in the order they are listed, as the labels a reference learns. */
export function boxLabels(boxes: readonly Box[]): readonly Label[] {
  return boxes
    .map((box) => ({ screenshot: box.screenshot, region: box.region, text: box.text.trim() }))
    .filter((label) => label.text.length > 0);
}

/** A reference's boxes and the selected one, which the stage moves and Find more like it samples. */
export interface BoxState {
  readonly boxes: readonly Box[];
  /** The selected box's ID, or null when none is selected. */
  readonly selectedId: number | null;
}

/** What changes a reference's boxes or which one is selected. */
export type BoxEvent =
  /**
   * A drag on screenshot `shown`, or on the live view when that is null, made `region`: a box drawn anew, or the
   * selected box adjusted.
   */
  | {
      readonly type: 'region';
      readonly shown: number | null;
      readonly region: Rect;
      readonly made: 'drawn' | 'adjusted';
    }
  /** Found text was added, each find as a box. */
  | { readonly type: 'add'; readonly boxes: readonly NewBox[] }
  | { readonly type: 'retype'; readonly id: number; readonly text: string }
  | { readonly type: 'remove'; readonly id: number }
  | { readonly type: 'select'; readonly id: number | null }
  /**
   * The box the reference reads at goes back where a recording read: box `id` moves to `region` on `screenshot` and
   * to the top of the list, where the reading box is. When that box is gone, a new one with no text takes its place.
   */
  | { readonly type: 'restore'; readonly id: number | null; readonly screenshot: number; readonly region: Rect };

/**
 * The boxes and the selection after `event`. A new box takes an ID past every ID in `state`, so the ID belongs to
 * the boxes the event is applied to.
 */
export function boxStep(state: BoxState, event: BoxEvent): BoxState {
  const { boxes, selectedId } = state;
  switch (event.type) {
    case 'region': {
      if (event.shown === null) {
        return state;
      }
      const adjusted = event.made === 'adjusted' ? boxOf(boxesOn(boxes, event.shown), selectedId) : null;
      if (adjusted !== null) {
        return { boxes: changedBox(boxes, adjusted.id, { region: event.region }), selectedId };
      }
      const added = withBoxes(boxes, [{ screenshot: event.shown, region: event.region, text: '' }]);
      return { boxes: added.boxes, selectedId: added.first };
    }
    case 'add':
      return { boxes: withBoxes(boxes, event.boxes).boxes, selectedId };
    case 'retype':
      return { boxes: changedBox(boxes, event.id, { text: event.text }), selectedId };
    case 'remove':
      return {
        boxes: boxes.filter((box) => box.id !== event.id),
        selectedId: selectedId === event.id ? null : selectedId,
      };
    case 'select':
      return { boxes, selectedId: event.id };
    case 'restore': {
      const place = { screenshot: event.screenshot, region: event.region };
      const kept = boxOf(boxes, event.id);
      if (kept === null) {
        const added = withBoxes(boxes, [{ ...place, text: '' }]);
        const made = added.boxes.filter((box) => box.id === added.first);
        return { boxes: [...made, ...boxes], selectedId: added.first };
      }
      return { boxes: [{ ...kept, ...place }, ...boxes.filter((box) => box.id !== kept.id)], selectedId: kept.id };
    }
  }
}
