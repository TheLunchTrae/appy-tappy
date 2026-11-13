// The rectangle the user draws on a screenshot, in the window's own pixels.

/** A point in a picture's pixels. */
export interface Point {
  readonly x: number;
  readonly y: number;
}

/** A width and a height in pixels. */
export interface Size {
  readonly width: number;
  readonly height: number;
}

/** A rectangle in a picture's whole pixels. */
export interface Rect {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

/** A corner the user can drag. */
export type Corner = 'nw' | 'ne' | 'sw' | 'se';

function clamp(value: number, low: number, high: number): number {
  return Math.min(Math.max(value, low), high);
}

/** The smallest rectangle that holds every one of `rects`, or null when there are none. */
export function bounding(rects: readonly Rect[]): Rect | null {
  const [first, ...rest] = rects;
  if (first === undefined) {
    return null;
  }
  let [left, top, right, bottom] = [first.x, first.y, first.x + first.width, first.y + first.height];
  for (const rect of rest) {
    left = Math.min(left, rect.x);
    top = Math.min(top, rect.y);
    right = Math.max(right, rect.x + rect.width);
    bottom = Math.max(bottom, rect.y + rect.height);
  }
  return { x: left, y: top, width: right - left, height: bottom - top };
}

/** Whether `a` and `b` are the same rectangle, or both missing. */
export function sameRect(a: Rect | null, b: Rect | null): boolean {
  if (a === null || b === null) {
    return a === b;
  }
  return a.x === b.x && a.y === b.y && a.width === b.width && a.height === b.height;
}

/** The whole-pixel rectangle between two points, kept inside `bounds`, or null when it covers no pixel. */
export function fromCorners(start: Point, end: Point, bounds: Size): Rect | null {
  const left = Math.floor(clamp(Math.min(start.x, end.x), 0, bounds.width));
  const top = Math.floor(clamp(Math.min(start.y, end.y), 0, bounds.height));
  const right = Math.ceil(clamp(Math.max(start.x, end.x), 0, bounds.width));
  const bottom = Math.ceil(clamp(Math.max(start.y, end.y), 0, bounds.height));
  if (right - left < 1 || bottom - top < 1) {
    return null;
  }
  return { x: left, y: top, width: right - left, height: bottom - top };
}

/** The corner across the rectangle from `corner`, which stays put while `corner` is dragged. */
export function across(corner: Corner): Corner {
  switch (corner) {
    case 'nw':
      return 'se';
    case 'ne':
      return 'sw';
    case 'sw':
      return 'ne';
    case 'se':
      return 'nw';
  }
}

/** Where `corner` of `rect` is. */
export function cornerAt(rect: Rect, corner: Corner): Point {
  return {
    x: corner === 'nw' || corner === 'sw' ? rect.x : rect.x + rect.width,
    y: corner === 'nw' || corner === 'ne' ? rect.y : rect.y + rect.height,
  };
}

/**
 * `rect` moved by (`dx`, `dy`) whole pixels, stopping at the edges of `bounds`. A rectangle wider or taller than
 * `bounds` is cut to it, so the result always lies inside.
 */
export function moveRect(rect: Rect, dx: number, dy: number, bounds: Size): Rect {
  const width = Math.min(rect.width, bounds.width);
  const height = Math.min(rect.height, bounds.height);
  return {
    x: clamp(rect.x + Math.round(dx), 0, bounds.width - width),
    y: clamp(rect.y + Math.round(dy), 0, bounds.height - height),
    width,
    height,
  };
}

/**
 * `rect`, measured on a `from`-sized copy of a picture, in the pixels of its `to`-sized copy: every pixel it reaches
 * into, and at least one each way, inside the picture. Null when either size is empty.
 */
export function scaleRect(rect: Rect, from: Size, to: Size): Rect | null {
  if (from.width <= 0 || from.height <= 0 || to.width <= 0 || to.height <= 0) {
    return null;
  }
  const x = to.width / from.width;
  const y = to.height / from.height;
  const left = clamp(Math.floor(rect.x * x), 0, to.width - 1);
  const top = clamp(Math.floor(rect.y * y), 0, to.height - 1);
  const right = clamp(Math.ceil((rect.x + rect.width) * x), left + 1, to.width);
  const bottom = clamp(Math.ceil((rect.y + rect.height) * y), top + 1, to.height);
  return { x: left, y: top, width: right - left, height: bottom - top };
}

/** The pixel of a `bounds`-sized picture under `point`, or the nearest one when `point` is on or past its far edge. */
export function pixelAt(point: Point, bounds: Size): Point {
  return {
    x: clamp(Math.floor(point.x), 0, Math.max(bounds.width - 1, 0)),
    y: clamp(Math.floor(point.y), 0, Math.max(bounds.height - 1, 0)),
  };
}

/** Whether `point` lies inside `rect`. */
export function contains(rect: Rect, point: Point): boolean {
  return point.x >= rect.x && point.x < rect.x + rect.width && point.y >= rect.y && point.y < rect.y + rect.height;
}

/** The largest size with `content`'s shape that fits in `space`. */
export function fit(content: Size, space: Size): Size {
  if (content.width <= 0 || content.height <= 0 || space.width <= 0 || space.height <= 0) {
    return { width: 0, height: 0 };
  }
  const scale = Math.min(space.width / content.width, space.height / content.height);
  return { width: content.width * scale, height: content.height * scale };
}

/** `point`, measured on a picture drawn at `shown` size, in the picture's own pixels. */
export function toPicture(point: Point, shown: Size, picture: Size): Point {
  if (shown.width <= 0 || shown.height <= 0) {
    return { x: 0, y: 0 };
  }
  return {
    x: (point.x * picture.width) / shown.width,
    y: (point.y * picture.height) / shown.height,
  };
}

/** `point`, in a picture's own pixels, measured on the picture drawn at `shown` size. */
export function fromPicture(point: Point, shown: Size, picture: Size): Point {
  if (picture.width <= 0 || picture.height <= 0) {
    return { x: 0, y: 0 };
  }
  return {
    x: (point.x * shown.width) / picture.width,
    y: (point.y * shown.height) / picture.height,
  };
}

/** Where a region's label goes, and which of the box's sides it lines up with. */
export interface TagPlace {
  readonly side: 'above' | 'below' | 'inside';
  readonly align: 'left' | 'right';
}

/**
 * Where the label of `rect` goes on a picture drawn at `scale`, when the label needs `room` drawn pixels: above the
 * box where the picture has that room, else below it, and inside it only when neither side has room. It lines up
 * with the box's side that is farther from the picture's edge, so it stays over the picture.
 */
export function tagPlace(rect: Rect, picture: Size, scale: number, room: number): TagPlace {
  const above = rect.y * scale;
  const below = (picture.height - rect.y - rect.height) * scale;
  return {
    side: above >= room ? 'above' : below >= room ? 'below' : 'inside',
    align: rect.x + rect.width / 2 <= picture.width / 2 ? 'left' : 'right',
  };
}
