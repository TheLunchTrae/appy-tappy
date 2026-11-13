import { type KeyboardEvent, type PointerEvent, type ReactNode, useEffect, useRef, useState } from 'react';

import type { Combine } from '../lib/bindings.ts';
import { type Drag, type DragInput, IDLE, shownBoxes, step } from '../lib/drag.ts';
import {
  type Corner,
  type Point,
  type Rect,
  type Size,
  contains,
  pixelAt,
  sameRect,
  tagPlace,
  toPicture,
} from '../lib/region.ts';
import { type SelectTool, combineOf, shade } from '../lib/selection.ts';

const CORNERS: readonly Corner[] = ['nw', 'ne', 'sw', 'se'];

/** How far the pointer travels, in drawn pixels, before a press becomes a drag, so a click changes nothing. */
const DRAG_SLOP = 3;

/** The room a region's label needs beside its box, in drawn pixels. */
const TAG_ROOM = 26;

/** The corner handle that `target` is, or null when it is anything else. */
function cornerOf(target: EventTarget): Corner | null {
  const name = target instanceof HTMLElement ? target.dataset['corner'] : undefined;
  return CORNERS.find((corner) => corner === name) ?? null;
}

/** A box on the picture other than the region, which a press selects. */
export interface OtherBox {
  readonly id: number;
  readonly rect: Rect;
  /** What its tag says, such as its number. */
  readonly label: string;
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
  /** Whether the region is a reading area, larger than what it reads, which shows dashed. */
  readonly area?: boolean;
  readonly label: string;
  /** Whether the user may draw on it, which they may only on a screenshot. */
  readonly editable: boolean;
  /** Whether a click picks a color instead of drawing. */
  readonly picking: boolean;
  /** How a drag or a click marks the region; the box is drawn by hand unless one is given. */
  readonly tool?: SelectTool;
  /** Which pixels of the region a selection leaves out, row by row, to shade them. */
  readonly ignored?: Uint8Array | null;
  /**
   * The other boxes on the picture, outlined without dimming the rest. A press on one selects it, through
   * `onSelectOther`, and drags it; the region is the selected box, and a drag away from every box draws a new one.
   */
  readonly others?: readonly OtherBox[];
  readonly onSelectOther?: (id: number) => void;
  /** Called when Delete or Backspace is pressed on the surface, which can then take focus. */
  readonly onDelete?: () => void;
  /** Called when the main button goes down on the surface to draw, and when that pointer is let go or cancelled. */
  readonly onPointer?: (phase: 'down' | 'up') => void;
  /** Called with the region a finished drag made, drawn anew or adjusted. A drag that is interrupted calls nothing. */
  readonly onRegion: (region: Rect, made: 'drawn' | 'adjusted') => void;
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
 * A region's outline over a picture drawn at `scale`, with its label, inside an element that covers the picture. The
 * outline and the label lie outside the box wherever the picture has room, and the rest of the picture is dimmed, so
 * nothing covers what the region holds.
 */
export function Marquee({
  rect,
  picture,
  scale,
  label,
  recording = false,
  area = false,
  dim = true,
  children,
}: {
  readonly rect: Rect;
  readonly picture: Size;
  readonly scale: number;
  readonly label: string;
  readonly recording?: boolean;
  readonly area?: boolean;
  /** Whether the rest of the picture is dimmed, which it is not while other boxes show beside it. */
  readonly dim?: boolean;
  readonly children?: ReactNode;
}) {
  const tag = tagPlace(rect, picture, scale, TAG_ROOM);
  const box = placed(rect, scale);
  const [left, top, right, bottom] = [box.left, box.top, box.left + box.width, box.top + box.height];
  // The whole picture but the box, as one shape with a hole.
  const rest =
    `polygon(evenodd, 0 0, 100% 0, 100% 100%, 0 100%, 0 0, ` +
    `${left}px ${top}px, ${left}px ${bottom}px, ${right}px ${bottom}px, ${right}px ${top}px, ${left}px ${top}px)`;
  return (
    <>
      {dim ? <div className="dim" style={{ clipPath: rest }} /> : null}
      <div
        className={['marquee', recording ? 'recording' : null, area ? 'area' : null].filter(Boolean).join(' ')}
        style={box}
      >
        {children}
        <span className={`tag ${tag.side} ${tag.align}`}>
          {label} · {rect.width} × {rect.height}
        </span>
      </div>
    </>
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
  area = false,
  label,
  editable,
  picking,
  tool = 'box',
  ignored = null,
  others = [],
  onSelectOther,
  onDelete,
  onPointer,
  onRegion,
  onPick,
  onSnap,
  onSelectAt,
}: Props) {
  const surface = useRef<HTMLDivElement>(null);
  // The drag as the handlers left it, which several pointer events can change before the next render.
  const latest = useRef<Drag>(IDLE);
  const [drag, setDrag] = useState<Drag>(IDLE);

  // Where the pointer last was in the window, to find what it is over once the picture moves under it.
  const pointer = useRef<Point | null>(null);

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
            send({ type: 'capture-failed', pointer: effect.pointer });
          }
          break;
        case 'uncapture':
          if (element?.hasPointerCapture(effect.pointer) === true) {
            element.releasePointerCapture(effect.pointer);
          }
          break;
        case 'region':
          onRegion(effect.region, effect.made);
          break;
        case 'snap':
          onSnap?.(effect.rough, effect.combine);
          break;
      }
    }
  };

  /** The place in the picture under `client`, a place in the window, when the surface is at `bounds`. */
  const under = (client: Point, bounds: DOMRect): Point =>
    toPicture({ x: client.x - bounds.left, y: client.y - bounds.top }, bounds, size);
  /** The place in the picture under the pointer of `event`, which is remembered. */
  const at = (event: PointerEvent<HTMLDivElement>): Point => {
    pointer.current = { x: event.clientX, y: event.clientY };
    return under(pointer.current, event.currentTarget.getBoundingClientRect());
  };
  const shifted = (): void => {
    const bounds = surface.current?.getBoundingClientRect();
    if (pointer.current !== null && bounds !== undefined) {
      send({ type: 'shifted', point: under(pointer.current, bounds) });
    }
  };

  const latestly = useRef({ send, shifted });
  useEffect(() => {
    latestly.current = { send, shifted };
  });
  // A drag belongs to one picture and one way of taking presses, so a change to either ends it.
  useEffect(() => {
    latestly.current.send({ type: 'changed' });
  }, [screenshot, size.width, size.height, editable, picking, tool]);
  // A scroll or a zoom moves the picture under a pointer that stays still, and no pointer event tells of it.
  useEffect(() => {
    const scrolled = (): void => {
      latestly.current.shifted();
    };
    document.addEventListener('scroll', scrolled, true);
    return () => {
      document.removeEventListener('scroll', scrolled, true);
    };
  }, []);
  useEffect(() => {
    latestly.current.shifted();
  }, [shown.width, shown.height]);

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
    onPointer?.('down');
    const drawn = event.currentTarget.getBoundingClientRect().width;
    const corner = cornerOf(event.target);
    // A press on another box, away from the region, selects that box and drags it.
    const other =
      tool === 'box' && corner === null && (region === null || !contains(region, point))
        ? others.findLast((box) => contains(box.rect, point))
        : undefined;
    if (other !== undefined) {
      onSelectOther?.(other.id);
    }
    send({
      type: 'press',
      pointer: event.pointerId,
      point,
      corner,
      tool,
      combine: combineOf(event),
      region: other?.rect ?? region,
      bounds: { width: size.width, height: size.height },
      slop: drawn > 0 ? (DRAG_SLOP * size.width) / drawn : 0,
    });
  };

  const move = (event: PointerEvent<HTMLDivElement>): void => {
    send({ type: 'move', pointer: event.pointerId, point: at(event), held: (event.buttons & 1) !== 0 });
  };

  const end = (event: PointerEvent<HTMLDivElement>): void => {
    send({ type: 'release', pointer: event.pointerId, point: at(event) });
    onPointer?.('up');
  };

  const cancel = (event: PointerEvent<HTMLDivElement>): void => {
    send({ type: 'cancel', pointer: event.pointerId });
    onPointer?.('up');
  };

  const key = (event: KeyboardEvent<HTMLDivElement>): void => {
    if (onDelete !== undefined && (event.key === 'Delete' || event.key === 'Backspace')) {
      event.preventDefault();
      onDelete();
    }
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
      // With boxes to delete, the surface takes focus, so Delete reaches it after a box is pressed.
      {...(onDelete === undefined
        ? {}
        : { tabIndex: 0, role: 'group', 'aria-label': 'Boxes on this screenshot', onKeyDown: key })}
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
      {others.map((other) => (
        <div key={other.id} className="other-box" style={placed(other.rect, scale)}>
          <span className="tag">{other.label}</span>
        </div>
      ))}
      {boxes.region === null ? null : (
        <Marquee
          rect={boxes.region}
          picture={size}
          scale={scale}
          label={label}
          recording={recording}
          area={area}
          dim={onSelectOther === undefined}
        >
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
