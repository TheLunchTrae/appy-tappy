// A click's target: where it lands in its application's window, as the
// target screen sets it up and the automation editor shows it.

import {
  type AntiCheatDef,
  type AppSummary,
  type ClickAtDef,
  type ClickButtonDef,
  type CornerDef,
  LIMITS,
} from './bindings.ts';
import type { Point, Rect, Size } from './region.ts';

/** What each button choice is called on screen. */
export const BUTTON_NAMES: Readonly<Record<ClickButtonDef, string>> = {
  left: 'Left',
  right: 'Right',
  double: 'Double',
};

/** The button choices, in the order the target screen offers them. */
export const BUTTONS: readonly ClickButtonDef[] = ['left', 'right', 'double'];

/** How alike a picture must look, as a new target starts: a little room for a highlighted button. */
export const DEFAULT_TOLERANCE = 20;

/** How far around the picture a new search area reaches, in pixels. */
export const SEARCH_MARGIN = 50;

/** The side of the largest square search area the runtime allows, in pixels. */
export const SEARCH_SIDE = Math.floor(Math.sqrt(LIMITS.maxSearchPixels));

/** The spot at `point` of a window `size`, measured from the corner nearest it, so it stays put when the window grows away from that corner. */
export function spotAt(point: Point, size: Size): Extract<ClickAtDef, { kind: 'spot' }> {
  const right = point.x >= size.width / 2;
  const bottom = point.y >= size.height / 2;
  const corner: CornerDef = bottom ? (right ? 'bottomRight' : 'bottomLeft') : right ? 'topRight' : 'topLeft';
  return {
    kind: 'spot',
    corner,
    x: right ? size.width - 1 - point.x : point.x,
    y: bottom ? size.height - 1 - point.y : point.y,
  };
}

/** Where a spot lands in a window `size`, from its top-left corner, or null when it lies outside it. */
export function placeSpot(at: Extract<ClickAtDef, { kind: 'spot' }>, size: Size): Point | null {
  const right = at.corner === 'topRight' || at.corner === 'bottomRight';
  const bottom = at.corner === 'bottomLeft' || at.corner === 'bottomRight';
  const x = right ? size.width - 1 - at.x : at.x;
  const y = bottom ? size.height - 1 - at.y : at.y;
  return x >= 0 && y >= 0 && x < size.width && y < size.height ? { x, y } : null;
}

/**
 * The area a picture `picture` is looked for in: up to {@link SEARCH_MARGIN} pixels around it, inside the window
 * `size`, and narrower where that would hold more than `LIMITS.maxSearchPixels`. Null when the picture alone is too
 * large.
 */
export function searchArea(picture: Rect, size: Size): Rect | null {
  if (picture.width * picture.height > LIMITS.maxSearchPixels) {
    return null;
  }
  for (let margin = SEARCH_MARGIN; margin >= 0; margin -= 1) {
    const left = Math.max(picture.x - margin, 0);
    const top = Math.max(picture.y - margin, 0);
    const right = Math.min(picture.x + picture.width + margin, size.width);
    const bottom = Math.min(picture.y + picture.height + margin, size.height);
    const area = { x: left, y: top, width: right - left, height: bottom - top };
    if (area.width * area.height <= LIMITS.maxSearchPixels) {
      return area;
    }
  }
  return picture;
}

/** The middle pixel of a picture `size`, where a click on it presses unless the user picks another place. */
export function middle(size: Size): Point {
  return { x: Math.floor(size.width / 2), y: Math.floor(size.height / 2) };
}

/** A button choice as a click's name, such as `Left click`. */
export function clickName(button: ClickButtonDef): string {
  return `${BUTTON_NAMES[button]} click`;
}

/** How far from which corner a spot is, such as `40 px from the right, 12 px from the top`. */
function spotWords(at: Extract<ClickAtDef, { kind: 'spot' }>): string {
  const across = at.corner === 'topRight' || at.corner === 'bottomRight' ? 'right' : 'left';
  const down = at.corner === 'bottomLeft' || at.corner === 'bottomRight' ? 'bottom' : 'top';
  return `${at.x} px from the ${across}, ${at.y} px from the ${down}`;
}

/** A target in a few words, after its picture if it has one, such as `Left click on this picture, when it's found near where it was`. */
export function targetSummary(button: ClickButtonDef, at: ClickAtDef | null): string {
  if (at === null) {
    return 'No target yet. Set up where it clicks.';
  }
  return at.kind === 'spot'
    ? `${clickName(button)} ${spotWords(at)}`
    : `${clickName(button)} on this picture, when it's found near where it was`;
}

/** The saved applications a click can click in: those whose files read fine. */
export function clickApps(
  apps: readonly AppSummary[],
): readonly { readonly id: string; readonly name: string; readonly antiCheat: AntiCheatDef }[] {
  return apps.flatMap((app) =>
    app.file === null ? [] : [{ id: app.id, name: app.file.name, antiCheat: app.file.antiCheat }],
  );
}

/** What each anti-cheat class is called in a sentence. */
const ANTI_CHEAT_WORDS: Readonly<Record<AntiCheatDef, string>> = {
  none: 'none',
  userMode: 'user-mode',
  kernelLevel: 'kernel-level',
  unknown: 'not set',
};

/**
 * Why a click cannot run between the watched application and the one it clicks in, or null when it can: anti-cheat
 * can see a click, so both must have none, as the runtime also checks.
 */
export function clickBlocked(
  watched: { readonly name: string; readonly antiCheat: AntiCheatDef },
  clicked: { readonly name: string; readonly antiCheat: AntiCheatDef } | null,
): string | null {
  const blocking = [watched, clicked].find((app) => app !== null && app.antiCheat !== 'none');
  if (blocking === undefined || blocking === null) {
    return null;
  }
  const what =
    blocking.antiCheat === 'unknown'
      ? `${blocking.name}'s anti-cheat is not set`
      : `${blocking.name}'s anti-cheat is ${ANTI_CHEAT_WORDS[blocking.antiCheat]}`;
  return `${what}, so AppyTappy cannot click while it runs: anti-cheat can see clicks that AppyTappy makes. A click needs the watched application and the one it clicks in both set to None on the Applications page.`;
}
