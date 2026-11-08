import { type PointerEvent, useRef } from 'react';

import {
  type Corner,
  type Point,
  type Rect,
  type Size,
  contains,
  dragCorner,
  fromCorners,
  moveRect,
  toPicture,
} from './region.ts';

type Drag =
  | { readonly mode: 'draw'; readonly from: Point }
  | { readonly mode: 'move'; readonly from: Point; readonly rect: Rect }
  | { readonly mode: 'corner'; readonly corner: Corner };

const CORNERS: readonly Corner[] = ['nw', 'ne', 'sw', 'se'];

function isCorner(name: string | undefined): name is Corner {
  return CORNERS.some((corner) => corner === name);
}

interface Props {
  /** The picture's own size, which the region is measured in. */
  readonly size: Size;
  /** The size the picture shows at. */
  readonly shown: Size;
  readonly region: Rect | null;
  /** Whether the region is being recorded. */
  readonly recording?: boolean;
  readonly label: string;
  /** Whether the user may draw on it, which they may only on a screenshot. */
  readonly editable: boolean;
  /** Whether a click picks a color instead of drawing. */
  readonly picking: boolean;
  readonly onRegion: (region: Rect) => void;
  readonly onPick: (point: Point) => void;
}

/** The region over a picture, drawn and adjusted with the pointer. */
export function RegionSurface({
  size,
  shown,
  region,
  recording = false,
  label,
  editable,
  picking,
  onRegion,
  onPick,
}: Props) {
  const drag = useRef<Drag | null>(null);

  const at = (event: PointerEvent<HTMLDivElement>): Point => {
    const bounds = event.currentTarget.getBoundingClientRect();
    return toPicture(
      { x: event.clientX - bounds.left, y: event.clientY - bounds.top },
      { width: bounds.width, height: bounds.height },
      size,
    );
  };

  const start = (event: PointerEvent<HTMLDivElement>): void => {
    if (!editable || event.button !== 0) {
      return;
    }
    const point = at(event);
    if (picking) {
      onPick({ x: Math.floor(point.x), y: Math.floor(point.y) });
      return;
    }
    event.currentTarget.setPointerCapture(event.pointerId);
    const corner = event.target instanceof HTMLElement ? event.target.dataset['corner'] : undefined;
    if (region !== null && isCorner(corner)) {
      drag.current = { mode: 'corner', corner };
    } else if (region !== null && contains(region, point)) {
      drag.current = { mode: 'move', from: point, rect: region };
    } else {
      drag.current = { mode: 'draw', from: point };
    }
  };

  const move = (event: PointerEvent<HTMLDivElement>): void => {
    const current = drag.current;
    if (current === null) {
      return;
    }
    const point = at(event);
    switch (current.mode) {
      case 'draw': {
        const drawn = fromCorners(current.from, point, size);
        if (drawn !== null) {
          onRegion(drawn);
        }
        break;
      }
      case 'move':
        onRegion(moveRect(current.rect, point.x - current.from.x, point.y - current.from.y, size));
        break;
      case 'corner':
        if (region !== null) {
          onRegion(dragCorner(region, current.corner, point, size));
        }
        break;
    }
  };

  const end = (): void => {
    drag.current = null;
  };

  const scale = size.width > 0 ? shown.width / size.width : 0;
  const classes = ['surface', picking ? 'picking' : null, editable ? null : 'still'].filter(Boolean).join(' ');

  return (
    <div className={classes} onPointerDown={start} onPointerMove={move} onPointerUp={end} onPointerCancel={end}>
      {region === null ? null : (
        <div
          className={recording ? 'marquee recording' : 'marquee'}
          style={{
            left: region.x * scale,
            top: region.y * scale,
            width: region.width * scale,
            height: region.height * scale,
          }}
        >
          {editable
            ? CORNERS.map((corner) => <span key={corner} className={`corner ${corner}`} data-corner={corner} />)
            : null}
          <span className="tag">
            {label} · {region.width} × {region.height}
          </span>
        </div>
      )}
    </div>
  );
}
