import { describe, expect, test } from 'bun:test';

import { type Drag, type DragEffect, type DragInput, IDLE, shownBoxes, step } from '../src/lib/drag.ts';
import type { Corner, Point, Rect } from '../src/lib/region.ts';

const bounds = { width: 100, height: 50 };
const region: Rect = { x: 10, y: 10, width: 20, height: 10 };
const MOUSE = 1;

/** A press of the mouse with the box tool, on a picture that has `region`. */
function press(point: Point, more: Partial<DragInput & { readonly type: 'press' }> = {}): DragInput {
  return {
    type: 'press',
    pointer: MOUSE,
    point,
    corner: null,
    tool: 'box',
    combine: 'replace',
    region,
    bounds,
    slop: 3,
    ...more,
  };
}

const moveTo = (x: number, y: number, pointer = MOUSE): DragInput => ({
  type: 'move',
  pointer,
  point: { x, y },
  held: true,
});
/** A move that arrives with the main button up. */
const upAt = (x: number, y: number, pointer = MOUSE): DragInput => ({
  type: 'move',
  pointer,
  point: { x, y },
  held: false,
});
const releaseAt = (x: number, y: number, pointer = MOUSE): DragInput => ({ type: 'release', pointer, point: { x, y } });

/** The drag after `inputs`, and every effect they asked for. */
function run(inputs: readonly DragInput[], from: Drag = IDLE): { state: Drag; effects: DragEffect[] } {
  let state = from;
  const effects: DragEffect[] = [];
  for (const input of inputs) {
    const next = step(state, input);
    state = next.state;
    effects.push(...next.effects);
  }
  return { state, effects };
}

/** The regions and rough boxes `effects` hand over. */
function made(effects: readonly DragEffect[]): DragEffect[] {
  return effects.filter((effect) => effect.type === 'region' || effect.type === 'snap');
}

function inside(rect: Rect): boolean {
  return (
    rect.x >= 0 &&
    rect.y >= 0 &&
    rect.width >= 1 &&
    rect.height >= 1 &&
    rect.x + rect.width <= bounds.width &&
    rect.y + rect.height <= bounds.height
  );
}

describe('drawing', () => {
  test('a press outside the region captures the pointer and draws once the pointer leaves the slop', () => {
    const pressed = run([press({ x: 50, y: 20 })]);
    expect(pressed.state.kind).toBe('drawing');
    expect(pressed.effects).toEqual([{ type: 'capture', pointer: MOUSE }]);

    const near = run([moveTo(52, 21)], pressed.state);
    expect(shownBoxes(near.state, region)).toEqual({ region, rough: null });

    const far = run([moveTo(60.5, 30.2)], near.state);
    expect(shownBoxes(far.state, region).region).toEqual({ x: 50, y: 20, width: 11, height: 11 });
  });

  test('letting go hands over the box drawn and frees the pointer', () => {
    const drawn = run([press({ x: 50, y: 20 }), moveTo(60, 30), releaseAt(60, 30)]);
    expect(drawn.state).toEqual(IDLE);
    expect(drawn.effects).toEqual([
      { type: 'capture', pointer: MOUSE },
      { type: 'uncapture', pointer: MOUSE },
      { type: 'region', region: { x: 50, y: 20, width: 10, height: 10 } },
    ]);
  });

  test('a click, which stays inside the slop, changes nothing', () => {
    const clicked = run([press({ x: 50.4, y: 20.6 }), moveTo(51, 21.5), releaseAt(51, 21.5)]);
    expect(clicked.state).toEqual(IDLE);
    expect(made(clicked.effects)).toEqual([]);
  });

  test('the box stays drawn when the pointer comes back inside the slop', () => {
    const back = run([press({ x: 50.4, y: 20.6 }), moveTo(60, 30), moveTo(50.6, 20.8), releaseAt(50.6, 20.8)]);
    expect(made(back.effects)).toEqual([{ type: 'region', region: { x: 50, y: 20, width: 1, height: 1 } }]);
  });

  test('with no region, a press where one would be draws', () => {
    const drawn = run([press({ x: 15, y: 15 }, { region: null }), moveTo(25, 25), releaseAt(25, 25)]);
    expect(made(drawn.effects)).toEqual([{ type: 'region', region: { x: 15, y: 15, width: 10, height: 10 } }]);
  });
});

describe('a press outside the picture', () => {
  test('draws the part of the picture the dragged box holds', () => {
    const cases: readonly (readonly [Point, Point, Rect])[] = [
      // From above, below, left and right of the picture, then from past a corner.
      [
        { x: 40, y: -12 },
        { x: 60, y: 8 },
        { x: 40, y: 0, width: 20, height: 8 },
      ],
      [
        { x: 40, y: 70 },
        { x: 60, y: 45 },
        { x: 40, y: 45, width: 20, height: 5 },
      ],
      [
        { x: -9, y: 30 },
        { x: 5, y: 40 },
        { x: 0, y: 30, width: 5, height: 10 },
      ],
      [
        { x: 130, y: 30 },
        { x: 95, y: 40 },
        { x: 95, y: 30, width: 5, height: 10 },
      ],
      [
        { x: -9, y: -9 },
        { x: 6, y: 7 },
        { x: 0, y: 0, width: 6, height: 7 },
      ],
      // From above the picture to its left, round the corner: the box holds the corner.
      [
        { x: 40.5, y: -12 },
        { x: -20, y: 30.5 },
        { x: 0, y: 0, width: 41, height: 31 },
      ],
    ];
    for (const [from, to, expected] of cases) {
      const drawn = run([press(from), moveTo(to.x, to.y), releaseAt(to.x, to.y)]);
      expect(made(drawn.effects)).toEqual([{ type: 'region', region: expected }]);
    }
  });

  test('makes no region while the dragged box holds none of the picture', () => {
    const cases: readonly (readonly [Point, Point])[] = [
      [
        { x: 40.5, y: -12 },
        { x: 70.5, y: -3 },
      ],
      [
        { x: -9, y: 30.5 },
        { x: -2, y: 44.5 },
      ],
      [
        { x: 130, y: 60 },
        { x: 104, y: 52 },
      ],
      // Down to the very edge, which holds no pixel yet.
      [
        { x: 40.5, y: -12 },
        { x: 70.5, y: 0 },
      ],
    ];
    for (const [from, to] of cases) {
      const outside = run([press(from), moveTo(to.x, to.y)]);
      expect(shownBoxes(outside.state, region)).toEqual({ region, rough: null });
      expect(made(run([releaseAt(to.x, to.y)], outside.state).effects)).toEqual([]);
    }
  });

  test('never moves or resizes the region, even right beside it', () => {
    const edge: Rect = { x: 0, y: 0, width: 20, height: 10 };
    expect(run([press({ x: -3, y: 5 }, { region: edge })]).state.kind).toBe('drawing');
  });
});

describe('moving', () => {
  test('a press inside the region moves it by the pointer travel, and letting go hands it over', () => {
    const moved = run([press({ x: 15.5, y: 12.5 }), moveTo(20.9, 9.9)]);
    expect(moved.state.kind).toBe('moving');
    expect(shownBoxes(moved.state, region).region).toEqual({ x: 15, y: 7, width: 20, height: 10 });
    expect(made(run([releaseAt(20.9, 9.9)], moved.state).effects)).toEqual([
      { type: 'region', region: { x: 15, y: 7, width: 20, height: 10 } },
    ]);
  });

  test('stops at the edges of the picture', () => {
    const moved = run([press({ x: 15, y: 12 }), moveTo(400, -80)]);
    expect(shownBoxes(moved.state, region).region).toEqual({ x: 80, y: 0, width: 20, height: 10 });
  });

  test('a press and release in place hands nothing over', () => {
    expect(made(run([press({ x: 15, y: 12 }), moveTo(15.2, 12.1), releaseAt(15.2, 12.1)]).effects)).toEqual([]);
  });
});

describe('resizing', () => {
  const grab = (corner: Corner, at: Point): Drag => run([press(at, { corner })]).state;

  test('a press on a corner drags that corner and keeps the one across from it', () => {
    const cases: readonly (readonly [Corner, Point, Point, Rect])[] = [
      ['se', { x: 30, y: 20 }, { x: 40, y: 25 }, { x: 10, y: 10, width: 30, height: 15 }],
      ['nw', { x: 10, y: 10 }, { x: 5, y: 5 }, { x: 5, y: 5, width: 25, height: 15 }],
      ['ne', { x: 30, y: 10 }, { x: 35, y: 0 }, { x: 10, y: 0, width: 25, height: 20 }],
      ['sw', { x: 10, y: 20 }, { x: 0, y: 30 }, { x: 0, y: 10, width: 30, height: 20 }],
    ];
    for (const [corner, from, to, expected] of cases) {
      const resized = run([moveTo(to.x, to.y), releaseAt(to.x, to.y)], grab(corner, from));
      expect(made(resized.effects)).toEqual([{ type: 'region', region: expected }]);
    }
  });

  test('a press off the corner tip does not make the corner jump', () => {
    // The handle reaches past the corner, so the press is a few pixels off it.
    const held = grab('se', { x: 32.6, y: 21.7 });
    expect(shownBoxes(held, region).region).toEqual(region);
    expect(made(run([moveTo(32.9, 21.9), releaseAt(32.9, 21.9)], held).effects)).toEqual([]);
    expect(shownBoxes(run([moveTo(36.6, 21.7)], held).state, region).region).toEqual({
      x: 10,
      y: 10,
      width: 24,
      height: 10,
    });
  });

  test('the corner across stays where it was after the dragged corner passes it', () => {
    const flipped = run([moveTo(5, 5), moveTo(4, 4), moveTo(2, 3)], grab('se', { x: 30, y: 20 }));
    expect(shownBoxes(flipped.state, region).region).toEqual({ x: 2, y: 3, width: 8, height: 7 });
    // Passing it on one side only keeps the other side's edge too.
    const half = run([moveTo(5, 25), moveTo(4, 25)], grab('se', { x: 30, y: 20 }));
    expect(shownBoxes(half.state, region).region).toEqual({ x: 4, y: 10, width: 6, height: 15 });
  });

  test('keeps the last box while the drag would leave none', () => {
    const emptied = run([moveTo(12, 25), moveTo(10, 40)], grab('se', { x: 30, y: 20 }));
    expect(shownBoxes(emptied.state, region).region).toEqual({ x: 10, y: 10, width: 2, height: 15 });
  });

  test('a corner handle resizes only with the box tool and a region', () => {
    expect(run([press({ x: 30, y: 20 }, { corner: 'se', region: null })]).state.kind).toBe('drawing');
    expect(run([press({ x: 30, y: 20 }, { corner: 'se', tool: 'snap' })]).state.kind).toBe('drawing');
  });
});

describe('a click on the region', () => {
  // One pixel of the view is this many of the picture: a 4K screenshot fitted to the view, a 1280 wide one, the
  // picture at its own size, and two zooms. The slop is three pixels of the view at each.
  const scales = [3, 2.2, 1.47, 1, 0.33, 0.125];
  const typed: Rect = { x: 37, y: 13, width: 21, height: 9 };
  const jitters: readonly Point[] = [
    { x: 1, y: 0 },
    { x: 0, y: -1 },
    { x: -1, y: 1 },
    { x: 2, y: 2 },
  ];

  test('leaves it exactly as it was when the pointer jitters by a pixel or two of the view', () => {
    const places: readonly (readonly [Corner | null, Point])[] = [
      [null, { x: 47.5, y: 17.5 }],
      ['nw', { x: 36.2, y: 12.4 }],
      ['ne', { x: 58.9, y: 12.4 }],
      ['sw', { x: 36.2, y: 22.8 }],
      ['se', { x: 58.9, y: 22.8 }],
    ];
    for (const one of scales) {
      for (const [corner, at] of places) {
        for (const jitter of jitters) {
          const to = { x: at.x + jitter.x * one, y: at.y + jitter.y * one };
          const pressed = run([press(at, { corner, region: typed, slop: 3 * one }), moveTo(to.x, to.y)]);
          expect(shownBoxes(pressed.state, typed)).toEqual({ region: typed, rough: null });
          expect(made(run([releaseAt(to.x, to.y)], pressed.state).effects)).toEqual([]);
        }
      }
    }
  });

  test('moves and resizes by the whole travel once the pointer has left the slop', () => {
    for (const one of scales) {
      const travel = Math.ceil(3 * one) + 2;
      const moved = run([
        press({ x: 47.5, y: 17.5 }, { region: typed, slop: 3 * one }),
        moveTo(47.5 + travel, 17.5),
        releaseAt(47.5 + travel, 17.5),
      ]);
      expect(made(moved.effects)).toEqual([{ type: 'region', region: { ...typed, x: typed.x + travel } }]);
      const resized = run([
        press({ x: 58.9, y: 22.8 }, { corner: 'se', region: typed, slop: 3 * one }),
        moveTo(58.9, 22.8 + travel),
        releaseAt(58.9, 22.8 + travel),
      ]);
      expect(made(resized.effects)).toEqual([{ type: 'region', region: { ...typed, height: typed.height + travel } }]);
    }
  });
  test('follows the pointer back to the press once the drag is under way', () => {
    const moved = run([press({ x: 15, y: 12 }), moveTo(40, 30), moveTo(16, 12)]);
    expect(shownBoxes(moved.state, region).region).toEqual({ x: 11, y: 10, width: 20, height: 10 });
    const resized = run([press({ x: 30, y: 20 }, { corner: 'se' }), moveTo(60, 40), moveTo(31, 20)]);
    expect(shownBoxes(resized.state, region).region).toEqual({ x: 10, y: 10, width: 21, height: 10 });
  });
});

describe('letting go', () => {
  test('ends the box where the pointer is let go, which no move reached', () => {
    const drawn = run([press({ x: 50, y: 20 }), releaseAt(60, 30)]);
    expect(drawn.state).toEqual(IDLE);
    expect(made(drawn.effects)).toEqual([{ type: 'region', region: { x: 50, y: 20, width: 10, height: 10 } }]);
    const moved = run([press({ x: 15, y: 12 }), moveTo(20, 12), releaseAt(25, 16)]);
    expect(made(moved.effects)).toEqual([{ type: 'region', region: { x: 20, y: 14, width: 20, height: 10 } }]);
    const resized = run([press({ x: 30, y: 20 }, { corner: 'se' }), releaseAt(40, 25)]);
    expect(made(resized.effects)).toEqual([{ type: 'region', region: { x: 10, y: 10, width: 30, height: 15 } }]);
  });

  test('a move with the main button up ends the drag with the box as it last was', () => {
    // The button came up somewhere before this move, so the place the move names is not where the box ends.
    const up = upAt(90, 45);
    const drawn = run([press({ x: 50, y: 20 }), moveTo(60, 30), up]);
    expect(drawn.state).toEqual(IDLE);
    expect(drawn.effects).toEqual([
      { type: 'capture', pointer: MOUSE },
      { type: 'uncapture', pointer: MOUSE },
      { type: 'region', region: { x: 50, y: 20, width: 10, height: 10 } },
    ]);
    const moved = run([press({ x: 15, y: 12 }), moveTo(20, 12), up]);
    expect(made(moved.effects)).toEqual([{ type: 'region', region: { x: 15, y: 10, width: 20, height: 10 } }]);
    // With nothing dragged yet it hands nothing over, and the next press starts a new drag.
    const clicked = run([press({ x: 50, y: 20 }), up]);
    expect(clicked.state).toEqual(IDLE);
    expect(made(clicked.effects)).toEqual([]);
  });
});

describe('a capture that cannot be taken', () => {
  test('ends the drag at once, with nothing to free and nothing handed over', () => {
    const pressed = run([press({ x: 15, y: 12 })]).state;
    expect(step(pressed, { type: 'capture-failed', pointer: MOUSE })).toEqual({ state: IDLE, effects: [] });
    const after = run([
      press({ x: 50, y: 20 }),
      { type: 'capture-failed', pointer: MOUSE },
      moveTo(70, 40),
      releaseAt(70, 40),
    ]);
    expect(after.state).toEqual(IDLE);
    expect(after.effects).toEqual([{ type: 'capture', pointer: MOUSE }]);
  });
});

describe('a picture that moves under the pointer', () => {
  test('carries the box to where the pointer now is, as a move would', () => {
    const shifted = (x: number, y: number): DragInput => ({ type: 'shifted', point: { x, y } });
    const drawn = run([press({ x: 50, y: 20 }), moveTo(60, 30), shifted(60, 45)]);
    expect(shownBoxes(drawn.state, region).region).toEqual({ x: 50, y: 20, width: 10, height: 25 });
    const moved = run([press({ x: 15, y: 12 }), moveTo(20, 12), shifted(20, 30)]);
    expect(shownBoxes(moved.state, region).region).toEqual({ x: 15, y: 28, width: 20, height: 10 });
    const resized = run([press({ x: 30, y: 20 }, { corner: 'se' }), moveTo(36, 20), shifted(36, 40)]);
    expect(shownBoxes(resized.state, region).region).toEqual({ x: 10, y: 10, width: 26, height: 30 });
    // The drag goes on: letting go hands over the box from there.
    expect(made(run([releaseAt(60, 45)], drawn.state).effects)).toEqual([
      { type: 'region', region: { x: 50, y: 20, width: 10, height: 25 } },
    ]);
  });
});

describe('the snap tool', () => {
  const snap = (point: Point): DragInput => press(point, { tool: 'snap', combine: 'add' });

  test('draws a rough box, even inside the region, and leaves the region showing', () => {
    const dragged = run([snap({ x: 15, y: 12 }), moveTo(40, 30)]);
    expect(dragged.state.kind).toBe('drawing');
    expect(shownBoxes(dragged.state, region)).toEqual({
      region,
      rough: { x: 15, y: 12, width: 25, height: 18 },
    });
  });

  test('hands the rough box over with how it combines, and never the region', () => {
    const snapped = run([snap({ x: 15, y: 12 }), moveTo(40, 30), releaseAt(40, 30)]);
    expect(made(snapped.effects)).toEqual([
      { type: 'snap', rough: { x: 15, y: 12, width: 25, height: 18 }, combine: 'add' },
    ]);
  });

  test('a click hands nothing over', () => {
    expect(made(run([snap({ x: 15.5, y: 12.5 }), moveTo(16, 13), releaseAt(16, 13)]).effects)).toEqual([]);
  });
});

describe('an interrupted drag', () => {
  // Each drag has changed what shows by the time it is interrupted.
  const drags: readonly (readonly [Drag['kind'], readonly DragInput[]])[] = [
    ['drawing', [press({ x: 50, y: 20 }), moveTo(70, 40)]],
    ['moving', [press({ x: 15, y: 12 }), moveTo(40, 30)]],
    ['resizing', [press({ x: 30, y: 20 }, { corner: 'se' }), moveTo(60, 40)]],
    ['drawing', [press({ x: 50, y: 20 }, { tool: 'snap' }), moveTo(70, 40)]],
  ];
  const interruptions: readonly DragInput[] = [{ type: 'cancel', pointer: MOUSE }, { type: 'changed' }];

  test('goes back to idle with the region as it was, and frees the pointer', () => {
    for (const [kind, inputs] of drags) {
      const dragging = run(inputs).state;
      expect(dragging.kind).toBe(kind);
      expect(shownBoxes(dragging, region)).not.toEqual({ region, rough: null });
      for (const interruption of interruptions) {
        const after = step(dragging, interruption);
        expect(after.state).toEqual(IDLE);
        expect(after.effects).toEqual([{ type: 'uncapture', pointer: MOUSE }]);
        expect(shownBoxes(after.state, region)).toEqual({ region, rough: null });
        // A release that arrives late hands nothing over either.
        expect(step(after.state, releaseAt(90, 45)).effects).toEqual([]);
      }
    }
  });

  test('an idle surface ignores every event but a press', () => {
    const late: readonly DragInput[] = [
      moveTo(5, 5),
      upAt(5, 5),
      releaseAt(5, 5),
      { type: 'shifted', point: { x: 5, y: 5 } },
      { type: 'capture-failed', pointer: MOUSE },
      ...interruptions,
    ];
    for (const input of late) {
      expect(step(IDLE, input)).toEqual({ state: IDLE, effects: [] });
    }
  });
});

describe('another pointer', () => {
  const TOUCH = 7;
  const dragging = run([press({ x: 50, y: 20 }), moveTo(70, 40)]).state;

  test('neither moves, ends, nor restarts a drag', () => {
    const others: readonly DragInput[] = [
      moveTo(90, 45, TOUCH),
      upAt(90, 45, TOUCH),
      releaseAt(90, 45, TOUCH),
      { type: 'cancel', pointer: TOUCH },
      { type: 'capture-failed', pointer: TOUCH },
      press({ x: 5, y: 5 }, { pointer: TOUCH }),
    ];
    for (const other of others) {
      expect(step(dragging, other)).toEqual({ state: dragging, effects: [] });
    }
  });

  test('the same pointer going down again starts over without handing the old box over', () => {
    const again = step(dragging, press({ x: 15, y: 12 }));
    expect(again.state.kind).toBe('moving');
    expect(again.effects).toEqual([
      { type: 'uncapture', pointer: MOUSE },
      { type: 'capture', pointer: MOUSE },
    ]);
  });
});

test('whatever the pointer does, the box stays inside the picture', () => {
  const far: readonly Point[] = [
    { x: -500, y: -500 },
    { x: 500, y: 500 },
    { x: -0.4, y: 49.6 },
    { x: 99.7, y: -0.2 },
    { x: 100, y: 50 },
    { x: 47.5, y: 900 },
  ];
  // The last region is one drawn on a larger screenshot than the one shown.
  const regions: readonly Rect[] = [
    region,
    { x: 80, y: 40, width: 20, height: 10 },
    { x: 60, y: 30, width: 300, height: 90 },
  ];
  const presses: readonly Partial<DragInput & { readonly type: 'press' }>[] = [
    {},
    { corner: 'nw' },
    { corner: 'se' },
    { tool: 'snap' },
  ];
  for (const rect of regions) {
    for (const how of presses) {
      for (const from of [...far, { x: rect.x + 1, y: rect.y + 1 }]) {
        for (const to of far) {
          const dragged = run([press(from, { ...how, region: rect }), moveTo(to.x, to.y), releaseAt(to.x, to.y)]);
          for (const effect of dragged.effects) {
            if (effect.type === 'region') {
              expect(inside(effect.region)).toBe(true);
            }
            if (effect.type === 'snap') {
              expect(inside(effect.rough)).toBe(true);
            }
          }
        }
      }
    }
  }
});
