import { type ReactNode, useEffect, useLayoutEffect, useRef, useState } from 'react';

import { type Point, type Size, toPicture } from '../lib/region.ts';
import { useSize } from '../lib/size.ts';
import { GUTTER, ZOOM_STEP, clampZoom, percent, scrollShift, shownAt, wheelZoom } from '../lib/zoom.ts';

/** A pixel of the picture, and the place in the window it is kept at while the zoom changes. */
interface Anchor {
  readonly point: Point;
  readonly at: Point;
}

interface Props {
  /** The size of the picture shown, or null while there is none. */
  readonly picture: Size | null;
  /** Whether the picture can be zoomed, which a screenshot can and the live view cannot. */
  readonly zoomable: boolean;
  readonly hidden?: boolean;
  /** What the user can do on the picture now. */
  readonly hint: string;
  /** What shows over the picture's place: the picture itself and what is drawn on it. */
  readonly children: (layout: { readonly space: Size; readonly shown: Size }) => ReactNode;
}

/**
 * Where a capture screen shows its picture: the whole picture at first, with room around it, and a screenshot zoomed
 * in on request. A zoomed picture pans by scrolling.
 */
export function ZoomView({ picture, zoomable, hidden = false, hint, children }: Props) {
  const view = useRef<HTMLDivElement>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const frame = useRef<HTMLDivElement>(null);
  const space = useSize(view);

  // The zoom belongs to pictures of one size: another size, or the live view, starts whole again.
  const size = picture ?? { width: 0, height: 0 };
  const of = zoomable ? `${size.width}x${size.height}` : null;
  const [zoomed, setZoomed] = useState<{ readonly of: string | null; readonly zoom: number }>({ of, zoom: 1 });
  if (zoomed.of !== of) {
    setZoomed({ of, zoom: 1 });
  }
  const zoom = zoomed.of === of ? clampZoom(zoomed.zoom, size, space) : 1;
  const shown = shownAt(size, space, zoom);

  // The pixel a zoom keeps in place, until the next render gives the picture its new size.
  const anchor = useRef<Anchor | null>(null);
  const zoomTo = (next: (zoom: number) => number, at: Point | null): void => {
    if (!zoomable) {
      return;
    }
    const box = frame.current?.getBoundingClientRect();
    anchor.current =
      at === null || box === undefined
        ? null
        : { at, point: toPicture({ x: at.x - box.left, y: at.y - box.top }, box, size) };
    setZoomed((current) => ({ of, zoom: clampZoom(next(current.of === of ? current.zoom : 1), size, space) }));
  };
  useLayoutEffect(() => {
    const kept = anchor.current;
    anchor.current = null;
    const box = frame.current?.getBoundingClientRect();
    if (kept === null || box === undefined) {
      return;
    }
    const shift = scrollShift(kept.point, kept.at, box, { width: size.width, height: size.height });
    scroller.current?.scrollBy(shift.x, shift.y);
  });

  /** Zooms around the middle of the view, as the buttons do. */
  const zoomBy = (factor: number): void => {
    const box = scroller.current?.getBoundingClientRect();
    zoomTo(
      (current) => current * factor,
      box === undefined ? null : { x: box.left + box.width / 2, y: box.top + box.height / 2 },
    );
  };

  // Ctrl and the wheel zoom around the pointer. React listens to the wheel passively, so this listener is its own.
  const onWheel = (event: WheelEvent): void => {
    if (!event.ctrlKey) {
      return;
    }
    event.preventDefault();
    zoomTo((current) => wheelZoom(current, event.deltaY), { x: event.clientX, y: event.clientY });
  };
  const latestWheel = useRef(onWheel);
  useEffect(() => {
    latestWheel.current = onWheel;
  });
  useEffect(() => {
    const target = scroller.current;
    if (target === null) {
      return undefined;
    }
    const listener = (event: WheelEvent): void => {
      latestWheel.current(event);
    };
    target.addEventListener('wheel', listener, { passive: false });
    return () => {
      target.removeEventListener('wheel', listener);
    };
  }, []);

  // The most this picture zooms, which is 1 for the live view. A control that can do nothing more says so and keeps
  // the focus: a press on it changes nothing.
  const most = zoomable ? clampZoom(Infinity, size, space) : 1;
  // Past two view pixels to one of the picture, each pixel shows as a square, so its edges are exact.
  const crisp = size.width > 0 && shown.width >= size.width * 2;
  return (
    <div className="viewer" hidden={hidden}>
      <div
        className="view"
        ref={view}
        onDragStart={(event) => {
          // A press on the picture never starts a drag of the page's own, which would cancel the pointer.
          event.preventDefault();
        }}
      >
        <div className="scroller" ref={scroller}>
          <div className="pad" style={{ padding: GUTTER }}>
            <div className={crisp ? 'frame crisp' : 'frame'} ref={frame} style={shown}>
              {children({ space, shown })}
            </div>
          </div>
        </div>
      </div>
      <div className="view-foot">
        <p className="hint">{hint}</p>
        <div className="zoom" role="group" aria-label="Zoom">
          <button
            type="button"
            className="btn small"
            aria-label="Zoom out"
            title="Zoom out"
            aria-disabled={zoom <= 1}
            onClick={() => {
              zoomBy(1 / ZOOM_STEP);
            }}
          >
            −
          </button>
          <span className="zoom-level" role="status">
            {zoomable ? percent(shown, size) : ''}
          </span>
          <button
            type="button"
            className="btn small"
            aria-label="Zoom in"
            title="Zoom in"
            aria-disabled={zoom >= most}
            onClick={() => {
              zoomBy(ZOOM_STEP);
            }}
          >
            +
          </button>
          <button
            type="button"
            className="btn small"
            title="Shows the whole screenshot"
            aria-disabled={zoom <= 1}
            onClick={() => {
              zoomTo(() => 1, null);
            }}
          >
            Fit
          </button>
        </div>
      </div>
    </div>
  );
}
