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

/** `boxes` with boxes at `added`, each a region of a screenshot and its text, and the first new box's ID. */
export function withBoxes(
  boxes: readonly Box[],
  added: readonly Omit<Box, 'id'>[],
): { readonly boxes: readonly Box[]; readonly first: number } {
  const first = boxes.reduce((highest, box) => Math.max(highest, box.id), -1) + 1;
  return { boxes: [...boxes, ...added.map((box, offset) => ({ ...box, id: first + offset }))], first };
}

/** `boxes` with the box `id` changed by `change`. */
export function changedBox(boxes: readonly Box[], id: number, change: Partial<Omit<Box, 'id'>>): readonly Box[] {
  return boxes.map((box) => (box.id === id ? { ...box, ...change } : box));
}

/** `boxes` without the box `id`. */
export function withoutBox(boxes: readonly Box[], id: number): readonly Box[] {
  return boxes.filter((box) => box.id !== id);
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
