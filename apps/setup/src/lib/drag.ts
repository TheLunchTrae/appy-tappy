// The drag that draws, moves, or resizes the region on a screenshot, as a pure
// (state, event) machine. The region surface feeds it pointer events and
// applies the effects it asks for. Points and rectangles are in the picture's
// own pixels, so the size the picture shows at never reaches the machine.

import type { Combine } from './bindings.ts';
import {
  type Corner,
  type Point,
  type Rect,
  type Size,
  across,
  contains,
  cornerAt,
  fromCorners,
  moveRect,
  sameRect,
} from './region.ts';

/** What a drawn box becomes when it is let go: the region, or the rough box the snap tool fits. */
type Draws = { readonly as: 'region' } | { readonly as: 'rough'; readonly combine: Combine };

/** What every drag keeps: the pointer that makes it, and the picture it is made on. */
interface Held {
  readonly pointer: number;
  readonly bounds: Size;
  /** Where the pointer went down, which may be outside the picture. */
  readonly pressed: Point;
}

/** Where a drag is. The region changes only when a drag is let go, so one that is interrupted leaves it alone. */
export type Drag =
  | { readonly kind: 'idle' }
  | (Held & {
      readonly kind: 'drawing';
      /** How far the pointer must still leave `pressed` before a box is drawn, or 0 once it has. */
      readonly slop: number;
      readonly draws: Draws;
      /** The part of the picture between `pressed` and the pointer, or null while that covers none of its pixels. */
      readonly box: Rect | null;
    })
  | (Held & {
      readonly kind: 'moving';
      /** The region as it was when the pointer went down. */
      readonly rect: Rect;
      readonly box: Rect;
    })
  | (Held & {
      readonly kind: 'resizing';
      readonly corner: Corner;
      /** The region as it was when the pointer went down. */
      readonly rect: Rect;
      readonly box: Rect;
    });

export const IDLE: Drag = { kind: 'idle' };

/** What the surface tells the drag of. */
export type DragInput =
  | {
      /** The main button went down on the surface with a tool that drags. */
      readonly type: 'press';
      readonly pointer: number;
      /** Where, which may be outside the picture. */
      readonly point: Point;
      /** The corner handle pressed, or null for a press anywhere else. */
      readonly corner: Corner | null;
      readonly tool: 'box' | 'snap';
      readonly combine: Combine;
      /** The region as it is. */
      readonly region: Rect | null;
      readonly bounds: Size;
      /** How far the pointer must travel before a press draws, so a click draws nothing. */
      readonly slop: number;
    }
  | { readonly type: 'move'; readonly pointer: number; readonly point: Point }
  | { readonly type: 'release'; readonly pointer: number }
  /** The pointer was cancelled or its capture was lost. */
  | { readonly type: 'cancel'; readonly pointer: number }
  /** The surface shows another picture, or takes presses another way. */
  | { readonly type: 'changed' };

/** What the surface does for the drag. */
export type DragEffect =
  | { readonly type: 'capture'; readonly pointer: number }
  | { readonly type: 'uncapture'; readonly pointer: number }
  | { readonly type: 'region'; readonly region: Rect }
  | { readonly type: 'snap'; readonly rough: Rect; readonly combine: Combine };

interface Step {
  readonly state: Drag;
  readonly effects: readonly DragEffect[];
}

function stay(state: Drag): Step {
  return { state, effects: [] };
}

function press(event: DragInput & { readonly type: 'press' }): Step {
  const { pointer, point, bounds, region } = event;
  const held: Held = { pointer, bounds, pressed: point };
  const effects: readonly DragEffect[] = [{ type: 'capture', pointer }];
  if (event.tool === 'box' && region !== null) {
    if (event.corner !== null) {
      return { state: { ...held, kind: 'resizing', corner: event.corner, rect: region, box: region }, effects };
    }
    if (contains(region, point)) {
      return { state: { ...held, kind: 'moving', rect: region, box: region }, effects };
    }
  }
  const draws: Draws = event.tool === 'box' ? { as: 'region' } : { as: 'rough', combine: event.combine };
  return {
    state: { ...held, kind: 'drawing', slop: event.slop, draws, box: null },
    effects,
  };
}

function move(state: Exclude<Drag, { readonly kind: 'idle' }>, point: Point): Drag {
  const dx = point.x - state.pressed.x;
  const dy = point.y - state.pressed.y;
  switch (state.kind) {
    case 'drawing': {
      if (Math.max(Math.abs(dx), Math.abs(dy)) < state.slop) {
        return state;
      }
      return { ...state, slop: 0, box: fromCorners(state.pressed, point, state.bounds) };
    }
    case 'moving':
      return { ...state, box: moveRect(state.rect, dx, dy, state.bounds) };
    case 'resizing': {
      // The corner moves as far as the pointer has, so a press that is off its tip does not make it jump.
      const tip = cornerAt(state.rect, state.corner);
      const to = { x: Math.round(tip.x + dx), y: Math.round(tip.y + dy) };
      const anchor = cornerAt(state.rect, across(state.corner));
      return { ...state, box: fromCorners(anchor, to, state.bounds) ?? state.box };
    }
  }
}

/** What letting go of `state` changes. */
function finish(state: Exclude<Drag, { readonly kind: 'idle' }>): readonly DragEffect[] {
  if (state.kind !== 'drawing') {
    return sameRect(state.box, state.rect) ? [] : [{ type: 'region', region: state.box }];
  }
  if (state.box === null) {
    return [];
  }
  return state.draws.as === 'region'
    ? [{ type: 'region', region: state.box }]
    : [{ type: 'snap', rough: state.box, combine: state.draws.combine }];
}

/**
 * The drag after `event`, and what the surface does for it. Only letting go changes the region: a cancelled pointer,
 * a lost capture, and a changed surface end the drag with the region as it was.
 */
export function step(state: Drag, event: DragInput): Step {
  if (event.type === 'press') {
    if (state.kind === 'idle') {
      return press(event);
    }
    if (state.pointer !== event.pointer) {
      return stay(state);
    }
    // The same pointer can only go down again after a release that never arrived, so its old drag is dropped.
    const next = press(event);
    return { state: next.state, effects: [{ type: 'uncapture', pointer: state.pointer }, ...next.effects] };
  }
  if (state.kind === 'idle') {
    return stay(state);
  }
  if (event.type === 'changed') {
    return { state: IDLE, effects: [{ type: 'uncapture', pointer: state.pointer }] };
  }
  if (event.pointer !== state.pointer) {
    return stay(state);
  }
  switch (event.type) {
    case 'move':
      return stay(move(state, event.point));
    case 'release':
      return { state: IDLE, effects: [{ type: 'uncapture', pointer: state.pointer }, ...finish(state)] };
    case 'cancel':
      return { state: IDLE, effects: [{ type: 'uncapture', pointer: state.pointer }] };
  }
}

/** What the surface shows for `state` over `region`, the region as it is: the region's box, and the rough box. */
export function shownBoxes(
  state: Drag,
  region: Rect | null,
): { readonly region: Rect | null; readonly rough: Rect | null } {
  switch (state.kind) {
    case 'idle':
      return { region, rough: null };
    case 'drawing':
      return state.draws.as === 'region' ? { region: state.box ?? region, rough: null } : { region, rough: state.box };
    case 'moving':
    case 'resizing':
      return { region: state.box, rough: null };
  }
}
