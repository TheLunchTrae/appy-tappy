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

/** The corner opposite `corner`, which stays put while `corner` is dragged. */
function opposite(rect: Rect, corner: Corner): Point {
  const right = rect.x + rect.width;
  const bottom = rect.y + rect.height;
  switch (corner) {
    case 'nw':
      return { x: right, y: bottom };
    case 'ne':
      return { x: rect.x, y: bottom };
    case 'sw':
      return { x: right, y: rect.y };
    case 'se':
      return { x: rect.x, y: rect.y };
  }
}

/** `rect` with `corner` dragged to `to`, or `rect` itself when that would cover no pixel. */
export function dragCorner(rect: Rect, corner: Corner, to: Point, bounds: Size): Rect {
  return fromCorners(opposite(rect, corner), to, bounds) ?? rect;
}

/** `rect` moved by (`dx`, `dy`) whole pixels, stopping at the edges of `bounds`. */
export function moveRect(rect: Rect, dx: number, dy: number, bounds: Size): Rect {
  return {
    ...rect,
    x: clamp(rect.x + Math.round(dx), 0, Math.max(bounds.width - rect.width, 0)),
    y: clamp(rect.y + Math.round(dy), 0, Math.max(bounds.height - rect.height, 0)),
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
