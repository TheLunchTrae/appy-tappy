// How far a screenshot is zoomed in its view. The zoom only changes the size
// the picture shows at: a region stays in the picture's own pixels.

import { type Point, type Size, fit, fromPicture } from './region.ts';

/** The empty space kept around the picture, in view pixels, where a drag can start and a corner's handle shows. */
export const GUTTER = 16;

/** The most view pixels one pixel of the picture is shown at. */
const MAX_SCALE = 8;

/** How much Zoom in and Zoom out change the zoom. */
export const ZOOM_STEP = 1.5;

/** How far the wheel turns to double the zoom, in the pixels a wheel event counts. */
const WHEEL_DOUBLE = 200;

/** Where the picture shows, in the coordinates its view is measured in. */
interface Box extends Size {
  readonly left: number;
  readonly top: number;
}

/** The size the picture shows at with no zoom: the largest that fits `space` with the gutter around it. */
function fitted(picture: Size, space: Size): Size {
  return fit(picture, { width: space.width - 2 * GUTTER, height: space.height - 2 * GUTTER });
}

/** `zoom` kept between 1, the whole picture, and the zoom at which one of its pixels shows `MAX_SCALE` wide. */
export function clampZoom(zoom: number, picture: Size, space: Size): number {
  const whole = fitted(picture, space);
  const most = whole.width > 0 ? Math.max(1, (MAX_SCALE * picture.width) / whole.width) : 1;
  return Math.min(Math.max(zoom, 1), most);
}

/** The zoom after the wheel turns by `deltaY`: away from the user zooms in. */
export function wheelZoom(zoom: number, deltaY: number): number {
  return zoom * 2 ** (-deltaY / WHEEL_DOUBLE);
}

/** The size the picture shows at in `space` at `zoom`, where 1 shows the whole picture. */
export function shownAt(picture: Size, space: Size, zoom: number): Size {
  const whole = fitted(picture, space);
  return { width: whole.width * zoom, height: whole.height * zoom };
}

/**
 * How far the view scrolls so that `point` of the picture shows at `anchor`, when the picture shows at `box`. A zoom
 * around the pointer scrolls by this once the picture has its new size, so the pixel under the pointer stays there.
 */
export function scrollShift(point: Point, anchor: Point, box: Box, picture: Size): Point {
  const at = fromPicture(point, box, picture);
  return { x: box.left + at.x - anchor.x, y: box.top + at.y - anchor.y };
}

/** How large the picture shows, as a percentage of its own pixels. */
export function percent(shown: Size, picture: Size): string {
  return picture.width > 0 ? `${Math.round((shown.width * 100) / picture.width)}%` : '';
}

/** A pan of a zoomed picture: the pointer whose right or middle button drags it, and where it was last. */
export interface Pan {
  readonly pointer: number;
  readonly at: Point;
}

/** The pan a press starts: the right and the middle button pan, as in image and video editors, and others don't. */
export function panStart(button: number, pointer: number, at: Point): Pan | null {
  return button === 1 || button === 2 ? { pointer, at } : null;
}

/** The bits of a pointer event's `buttons` that hold the right and the middle button down. */
const PAN_BUTTONS = 2 | 4;

/**
 * Where `pan` is after `pointer` moves to `at` with `buttons` held, and how far the view scrolls so the picture
 * follows the pointer. The pan ends, with no scroll, once neither the right nor the middle button is held: a button
 * let go while another stays down sends a move, not a release.
 */
export function panMove(
  pan: Pan,
  pointer: number,
  at: Point,
  buttons: number,
): { readonly pan: Pan | null; readonly scroll: Point } {
  if (pointer !== pan.pointer) {
    return { pan, scroll: { x: 0, y: 0 } };
  }
  if ((buttons & PAN_BUTTONS) === 0) {
    return { pan: null, scroll: { x: 0, y: 0 } };
  }
  return { pan: { pointer, at }, scroll: { x: pan.at.x - at.x, y: pan.at.y - at.y } };
}

/** The pan after `pointer` is let go, cancelled, or loses the view's hold: over for its own pointer only. */
export function panEnd(pan: Pan | null, pointer: number): Pan | null {
  return pan?.pointer === pointer ? null : pan;
}
