// Drawing the value's region over the live window. The press takes the screenshot the region goes on and holds the
// live view still, and the screenshot shows in its place once it arrives; once the pointer is let go and the
// screenshot has arrived, the region is placed on it.

import { useCallback, useRef, useState } from 'react';

import type { Rect, Size } from './region.ts';
import type { Taken } from './useScreenshots.ts';

/** A box drawn over the live view, in the pixels of the live frame of size `at`. */
export interface LiveBox {
  readonly rect: Rect;
  readonly at: Size;
}

/** What a press over the live view made: the screenshot it took, or null when that failed, and the box drawn. */
export interface Placed {
  readonly shot: Taken | null;
  readonly box: LiveBox | null;
}

/** Where a press over the live view is. */
export type LiveDraw =
  | { readonly kind: 'idle' }
  | {
      readonly kind: 'pressed';
      /** Which press this is, so the screenshot of an earlier one is not taken for it. */
      readonly press: number;
      readonly box: LiveBox | null;
      /** The screenshot the press took, once it arrives, or null when taking it failed. */
      readonly shot: Taken | null | 'taking';
      /** Whether the pointer was let go. */
      readonly released: boolean;
    };

export const LIVE_IDLE: LiveDraw = { kind: 'idle' };

/** What the live view hears of. */
export type LiveDrawEvent =
  /** The main button went down over the live view, which starts taking screenshot `press`. */
  | { readonly type: 'press'; readonly press: number }
  /** The drag made `box`. */
  | { readonly type: 'drew'; readonly box: LiveBox }
  /** The pointer was let go or cancelled. */
  | { readonly type: 'release' }
  /** The screenshot press `press` took arrived, or null when taking it failed. */
  | { readonly type: 'taken'; readonly press: number; readonly shot: Taken | null };

/**
 * The press after `event`, and what it placed once it is done: when the pointer is let go and its screenshot has
 * arrived, in either order. A press while one is under way is ignored, and so is a screenshot of another press.
 */
export function liveDraw(
  state: LiveDraw,
  event: LiveDrawEvent,
): { readonly state: LiveDraw; readonly placed: Placed | null } {
  if (event.type === 'press') {
    return state.kind === 'idle'
      ? { state: { kind: 'pressed', press: event.press, box: null, shot: 'taking', released: false }, placed: null }
      : { state, placed: null };
  }
  if (state.kind === 'idle') {
    return { state, placed: null };
  }
  let next = state;
  switch (event.type) {
    case 'drew':
      next = state.released ? state : { ...state, box: event.box };
      break;
    case 'release':
      next = { ...state, released: true };
      break;
    case 'taken':
      next = event.press === state.press && state.shot === 'taking' ? { ...state, shot: event.shot } : state;
      break;
  }
  return next.released && next.shot !== 'taking'
    ? { state: LIVE_IDLE, placed: { shot: next.shot, box: next.box } }
    : { state: next, placed: null };
}

/** What the stage shows of a press over the live view. */
export interface LiveShown {
  /** Whether the live view holds still for a press under way. */
  readonly frozen: boolean;
  /** The screenshot the press took, once it arrives, which shows in place of the frame held still. */
  readonly shot: Taken | null;
  /** The box drawn, once the pointer is let go, which stays outlined until its screenshot arrives. */
  readonly box: LiveBox | null;
}

/** What the stage shows for `state`. */
export function liveShown(state: LiveDraw): LiveShown {
  return state.kind === 'idle'
    ? { frozen: false, shot: null, box: null }
    : { frozen: true, shot: state.shot === 'taking' ? null : state.shot, box: state.released ? state.box : null };
}

/**
 * The live view's size after a frame of size `frame` arrives. A frame asked for before a press can arrive while the
 * view holds still for it, `frozen`; the box being drawn is measured in the size held, so that frame's size is not
 * taken. A frame of the size held keeps `current` itself.
 */
export function liveSizeAfter(current: Size | null, frame: Size, frozen: boolean): Size | null {
  if (frozen || (current?.width === frame.width && current.height === frame.height)) {
    return current;
  }
  return frame;
}

/**
 * Drawing over the live view with `take`, which takes a screenshot without showing it; `onPlaced` hears what each
 * press made. `shown` is what the stage shows of the press under way, and `isFrozen` says, as of the last event
 * rather than the last render, whether the live view holds still.
 */
export function useLiveDraw(
  take: () => Promise<Taken | null>,
  onPlaced: (placed: Placed) => void,
): {
  readonly shown: LiveShown;
  readonly isFrozen: () => boolean;
  readonly press: () => void;
  readonly drew: (box: LiveBox) => void;
  readonly release: () => void;
} {
  const state = useRef<LiveDraw>(LIVE_IDLE);
  const presses = useRef(0);
  const [rendered, setRendered] = useState<LiveDraw>(LIVE_IDLE);
  const placedRef = useRef(onPlaced);
  placedRef.current = onPlaced;
  const send = useCallback((event: LiveDrawEvent): void => {
    const next = liveDraw(state.current, event);
    state.current = next.state;
    setRendered(next.state);
    if (next.placed !== null) {
      placedRef.current(next.placed);
    }
  }, []);
  const press = useCallback((): void => {
    if (state.current.kind !== 'idle') {
      return;
    }
    presses.current += 1;
    const id = presses.current;
    send({ type: 'press', press: id });
    take().then(
      (shot) => {
        send({ type: 'taken', press: id, shot });
      },
      () => {
        send({ type: 'taken', press: id, shot: null });
      },
    );
  }, [send, take]);
  return {
    shown: liveShown(rendered),
    isFrozen: useCallback(() => state.current.kind !== 'idle', []),
    press,
    drew: useCallback(
      (box: LiveBox) => {
        send({ type: 'drew', box });
      },
      [send],
    ),
    release: useCallback(() => {
      send({ type: 'release' });
    }, [send]),
  };
}
