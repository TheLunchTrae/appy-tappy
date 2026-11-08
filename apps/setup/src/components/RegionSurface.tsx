import { type PointerEvent, useEffect, useRef, useState } from 'react';

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
} from '../lib/region.ts';
import { type Combine, type SelectTool, combineOf, shade } from '../lib/selection.ts';

type Drag =
  | { readonly mode: 'draw'; readonly from: Point }
  | { readonly mode: 'rough'; readonly from: Point; readonly combine: Combine }
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
  /** How a drag or a click marks the region; the box is drawn by hand unless one is given. */
  readonly tool?: SelectTool;
  /** Which pixels of the region a selection leaves out, row by row, to shade them. */
  readonly ignored?: Uint8Array | null;
  readonly onRegion: (region: Rect) => void;
  readonly onPick: (point: Point) => void;
  /** Called with the rough box dragged with the snap tool. */
  readonly onSnap?: (rough: Rect, combine: Combine) => void;
  /** Called with the pixel clicked with the click tool. */
  readonly onSelectAt?: (point: Point, combine: Combine) => void;
}

/** The pixels a selection leaves out of a `width`-wide region, shaded over it. */
function Shade({ ignored, width }: { readonly ignored: Uint8Array; readonly width: number }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const height = width > 0 ? ignored.length / width : 0;
  useEffect(() => {
    const context = canvas.current?.getContext('2d');
    if (context === null || context === undefined || width === 0 || !Number.isInteger(height) || height === 0) {
      return;
    }
    context.putImageData(new ImageData(shade(ignored, width), width, height), 0, 0);
  }, [ignored, width, height]);
  return <canvas ref={canvas} className="shade" width={width} height={height} aria-hidden="true" />;
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
  tool = 'box',
  ignored = null,
  onRegion,
  onPick,
  onSnap,
  onSelectAt,
}: Props) {
  const drag = useRef<Drag | null>(null);
  // The rough box being dragged with the snap tool, until it is let go.
  const [rough, setRough] = useState<Rect | null>(null);

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
    if (tool === 'click') {
      onSelectAt?.({ x: Math.floor(point.x), y: Math.floor(point.y) }, combineOf(event));
      return;
    }
    event.currentTarget.setPointerCapture(event.pointerId);
    if (tool === 'snap') {
      drag.current = { mode: 'rough', from: point, combine: combineOf(event) };
      return;
    }
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
      case 'rough':
        setRough(fromCorners(current.from, point, size));
        break;
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

  const end = (event: PointerEvent<HTMLDivElement>): void => {
    const current = drag.current;
    drag.current = null;
    setRough(null);
    if (current?.mode === 'rough') {
      // Taken from where the pointer was let go, since the last move may not
      // have been drawn yet.
      const box = fromCorners(current.from, at(event), size);
      if (box !== null) {
        onSnap?.(box, current.combine);
      }
    }
  };

  const scale = size.width > 0 ? shown.width / size.width : 0;
  const classes = [
    'surface',
    picking ? 'picking' : null,
    editable ? null : 'still',
    tool === 'click' ? 'clicking' : null,
  ]
    .filter(Boolean)
    .join(' ');
  const place = (rect: Rect) => ({
    left: rect.x * scale,
    top: rect.y * scale,
    width: rect.width * scale,
    height: rect.height * scale,
  });
  const cancel = (): void => {
    drag.current = null;
    setRough(null);
  };

  return (
    <div className={classes} onPointerDown={start} onPointerMove={move} onPointerUp={end} onPointerCancel={cancel}>
      {region === null || ignored === null || ignored.length !== region.width * region.height ? null : (
        <div className="shade-box" style={place(region)}>
          <Shade ignored={ignored} width={region.width} />
        </div>
      )}
      {region === null ? null : (
        <div className={recording ? 'marquee recording' : 'marquee'} style={place(region)}>
          {editable && tool === 'box'
            ? CORNERS.map((corner) => <span key={corner} className={`corner ${corner}`} data-corner={corner} />)
            : null}
          <span className="tag">
            {label} · {region.width} × {region.height}
          </span>
        </div>
      )}
      {rough === null ? null : <div className="rough" style={place(rough)} />}
    </div>
  );
}
