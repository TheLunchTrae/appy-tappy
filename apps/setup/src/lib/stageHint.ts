// What the capture's stage says under the picture: what the pointer does there now.

import { AREA_DRAW_HINT } from './moving.ts';
import type { SelectTool } from './selection.ts';

/** What the stage draws: the value's region, over the live window too, or a reference's boxes. */
export type Drawing = 'region' | 'live' | 'boxes';

/** What the stage shows and takes, which decides its hint. */
export interface StageView {
  /** What a recording that runs keeps, or null without one. */
  readonly recording: 'readings' | 'looks' | null;
  /** The screenshot shown, or null for the live window. */
  readonly selected: number | null;
  readonly shotCount: number;
  /** Whether the next click picks a color. */
  readonly picking: boolean;
  readonly liveError: string | null;
  /** The screenshot shortcut, as its key shows it. */
  readonly shortcut: string;
  readonly tool: SelectTool;
  /** Whether the value's region is a reading area. */
  readonly area: boolean;
  /** Whether the screenshot shown has another size than the edited value was drawn at. */
  readonly otherSize: boolean;
  readonly drawing: Drawing;
  /** Why no box can go on the screenshot shown, or null. */
  readonly boxBlock: string | null;
}

export function stageHint({
  recording,
  selected,
  shotCount,
  picking,
  liveError,
  shortcut,
  tool,
  area,
  otherSize,
  drawing,
  boxBlock,
}: StageView): string {
  if (selected === null && liveError !== null) {
    return liveError;
  }
  if (recording !== null && selected === null) {
    return recording === 'readings'
      ? 'Recording. Use the application; a reading is kept for each new shape.'
      : 'Recording. Use the application; a screenshot is kept of each different look.';
  }
  if (selected === null && drawing === 'live') {
    return `Drag over the window where the value shows; that takes the screenshot. Or take one (${shortcut}).`;
  }
  if (selected === null) {
    return shotCount === 0
      ? `Take a screenshot (${shortcut}) when the window shows what to read.`
      : 'Select a screenshot to draw on it.';
  }
  if (picking) {
    return 'Click the color to pick it.';
  }
  if (drawing === 'boxes') {
    return (
      boxBlock ??
      'Drag to draw a box around some characters. Press a box to select it, drag it to move it, or press Delete to remove it.'
    );
  }
  if (otherSize) {
    return 'The region shows on screenshots of the size the value was drawn at.';
  }
  if (tool === 'snap') {
    return 'Drag a rough box around the thing to snap to it.';
  }
  if (tool === 'click') {
    return 'Click the thing to select it.';
  }
  return area ? AREA_DRAW_HINT : 'Drag to draw a region. Drag inside it to move it, or drag a corner to resize it.';
}
