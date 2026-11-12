import { type PointerEvent, type ReactNode, useEffect, useRef, useState } from 'react';

import type { Combine } from '../lib/bindings.ts';
import { type Drag, type DragInput, IDLE, shownBoxes, step } from '../lib/drag.ts';
import {
  type Corner,
  type Point,
  type Rect,
  type Size,
  pixelAt,
  sameRect,
  tagPlace,
  toPicture,
} from '../lib/region.ts';
import { type SelectTool, combineOf, shade } from '../lib/selection.ts';

const CORNERS: readonly Corner[] = ['nw', 'ne', 'sw', 'se'];

/** How far the pointer travels, in drawn pixels, before a press draws a box, so a click draws nothing. */
const DRAG_SLOP = 3;

/** The room a region's label needs beside its box, in drawn pixels. */
const TAG_ROOM = 26;

/** The corner handle that `target` is, or null when it is anything else. */
function cornerOf(target: EventTarget): Corner | null {
  const name = target instanceof HTMLElement ? target.dataset['corner'] : undefined;
  return CORNERS.find((corner) => corner === name) ?? null;
}

interface Props {
  /** The picture's own size, which the region is measured in. */
  readonly size: Size;
  /** The size the picture shows at. */
  readonly shown: Size;
  /** The screenshot shown, or null for the live view. A drag ends when it changes. */
  readonly screenshot: number | null;
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
  /** Called with the region a finished drag made. A drag that is interrupted calls nothing. */
  readonly onRegion: (region: Rect) => void;
  readonly onPick: (point: Point) => void;
  /** Called with the rough box dragged with the snap tool. */
  readonly onSnap?: (rough: Rect, combine: Combine) => void;
  /** Called with the pixel clicked with the click tool. */
  readonly onSelectAt?: (point: Point, combine: Combine) => void;
}

function placed(rect: Rect, scale: number) {
  return { left: rect.x * scale, top: rect.y * scale, width: rect.width * scale, height: rect.height * scale };
}

/**
 * A region's outline over a picture drawn at `scale`, with its label. The outline and the label lie outside the box
 * wherever the picture has room, so they cover nothing of what the region holds.
 */
export function Marquee({
  rect,
  picture,
  scale,
  label,
  recording = false,
  children,
}: {
  readonly rect: Rect;
  readonly picture: Size;
  readonly scale: number;
  readonly label: string;
  readonly recording?: boolean;
  readonly children?: ReactNode;
}) {
  const tag = tagPlace(rect, picture, scale, TAG_ROOM);
  return (
    <div className={recording ? 'marquee recording' : 'marquee'} style={placed(rect, scale)}>
      {children}
      <span className={`tag ${tag.side} ${tag.align}`}>
        {label} · {rect.width} × {rect.height}
      </span>
    </div>
  );
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
  screenshot,
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
  const surface = useRef<HTMLDivElement>(null);
  // The drag as the handlers left it, which several pointer events can change before the next render.
  const latest = useRef<Drag>(IDLE);
  const [drag, setDrag] = useState<Drag>(IDLE);

  const send = (input: DragInput): void => {
    const next = step(latest.current, input);
    latest.current = next.state;
    setDrag(next.state);
    const element = surface.current;
    for (const effect of next.effects) {
      switch (effect.type) {
        case 'capture':
          try {
            element?.setPointerCapture(effect.pointer);
          } catch {
            // The pointer is gone already, so no release would end its drag.
            send({ type: 'cancel', pointer: effect.pointer });
          }
          break;
        case 'uncapture':
          if (element?.hasPointerCapture(effect.pointer) === true) {
            element.releasePointerCapture(effect.pointer);
          }
          break;
        case 'region':
          onRegion(effect.region);
          break;
        case 'snap':
          onSnap?.(effect.rough, effect.combine);
          break;
      }
    }
  };
  const latestSend = useRef(send);
  useEffect(() => {
    latestSend.current = send;
  });
  // A drag belongs to one picture and one way of taking presses, so a change to either ends it.
  useEffect(() => {
    latestSend.current({ type: 'changed' });
  }, [screenshot, size.width, size.height, editable, picking, tool]);

  const at = (event: PointerEvent<HTMLDivElement>): Point => {
    const bounds = event.currentTarget.getBoundingClientRect();
    return toPicture({ x: event.clientX - bounds.left, y: event.clientY - bounds.top }, bounds, size);
  };

  const start = (event: PointerEvent<HTMLDivElement>): void => {
    if (!editable || event.button !== 0) {
      return;
    }
    const point = at(event);
    if (picking) {
      onPick(pixelAt(point, size));
      return;
    }
    if (tool === 'click') {
      onSelectAt?.(pixelAt(point, size), combineOf(event));
      return;
    }
    const drawn = event.currentTarget.getBoundingClientRect().width;
    send({
      type: 'press',
      pointer: event.pointerId,
      point,
      corner: cornerOf(event.target),
      tool,
      combine: combineOf(event),
      region,
      bounds: { width: size.width, height: size.height },
      slop: drawn > 0 ? (DRAG_SLOP * size.width) / drawn : 0,
    });
  };

  const move = (event: PointerEvent<HTMLDivElement>): void => {
    if (latest.current.kind === 'idle') {
      return;
    }
    if ((event.buttons & 1) === 0) {
      // The button came up without a release, as it does while another button is held.
      send({ type: 'release', pointer: event.pointerId });
      return;
    }
    send({ type: 'move', pointer: event.pointerId, point: at(event) });
  };

  const end = (event: PointerEvent<HTMLDivElement>): void => {
    // The box ends where the pointer was let go, which the last move may not have reached.
    send({ type: 'move', pointer: event.pointerId, point: at(event) });
    send({ type: 'release', pointer: event.pointerId });
  };

  const cancel = (event: PointerEvent<HTMLDivElement>): void => {
    send({ type: 'cancel', pointer: event.pointerId });
  };

  const scale = size.width > 0 ? shown.width / size.width : 0;
  const drags = editable && !picking && tool !== 'click';
  const classes = [
    'surface',
    picking ? 'picking' : null,
    editable ? null : 'still',
    tool === 'click' ? 'clicking' : null,
    drags ? 'reach' : null,
  ]
    .filter(Boolean)
    .join(' ');
  const boxes = shownBoxes(drag, region);
  // The shading belongs to the region as it is, so a box being dragged shows without it.
  const shaded = region !== null && ignored?.length === region.width * region.height && sameRect(boxes.region, region);

  return (
    <div
      ref={surface}
      className={classes}
      onPointerDown={start}
      onPointerMove={move}
      onPointerUp={end}
      onPointerCancel={cancel}
      onLostPointerCapture={cancel}
    >
      {shaded ? (
        <div className="shade-box" style={placed(region, scale)}>
          <Shade ignored={ignored} width={region.width} />
        </div>
      ) : null}
      {boxes.region === null ? null : (
        <Marquee rect={boxes.region} picture={size} scale={scale} label={label} recording={recording}>
          {drags && tool === 'box' ? (
            <>
              <span className="grip" />
              {CORNERS.map((corner) => (
                <span key={corner} className={`corner ${corner}`} data-corner={corner} />
              ))}
            </>
          ) : null}
        </Marquee>
      )}
      {boxes.rough === null ? null : <div className="rough" style={placed(boxes.rough, scale)} />}
    </div>
  );
}
