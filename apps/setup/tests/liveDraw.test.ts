import { describe, expect, test } from 'bun:test';

import {
  LIVE_IDLE,
  type LiveDraw,
  type LiveDrawEvent,
  liveDraw,
  liveShown,
  liveSizeAfter,
} from '../src/lib/liveDraw.ts';

const box = { rect: { x: 1, y: 2, width: 30, height: 10 }, at: { width: 800, height: 450 } };
const shot = { index: 4, size: { width: 1600, height: 900 } };

/** The press after `events` from idle, and what each step placed. */
function run(events: readonly LiveDrawEvent[]): { readonly state: LiveDraw; readonly placed: readonly unknown[] } {
  let state = LIVE_IDLE;
  const placed: unknown[] = [];
  for (const event of events) {
    const next = liveDraw(state, event);
    state = next.state;
    if (next.placed !== null) {
      placed.push(next.placed);
    }
  }
  return { state, placed };
}

describe('drawing over the live view', () => {
  test('places the box once the pointer is let go and the screenshot has arrived, in either order', () => {
    const released = run([
      { type: 'press', press: 1 },
      { type: 'drew', box },
      { type: 'release' },
      { type: 'taken', press: 1, shot },
    ]);
    expect(released).toEqual({ state: LIVE_IDLE, placed: [{ shot, box }] });
    const taken = run([
      { type: 'press', press: 1 },
      { type: 'taken', press: 1, shot },
      { type: 'drew', box },
      { type: 'release' },
    ]);
    expect(taken).toEqual({ state: LIVE_IDLE, placed: [{ shot, box }] });
  });

  test('holds still while the screenshot is taken, even after the pointer is let go', () => {
    const waiting = run([{ type: 'press', press: 1 }, { type: 'drew', box }, { type: 'release' }]);
    expect(waiting.placed).toEqual([]);
    expect(waiting.state.kind).toBe('pressed');
  });

  test('a click with no drag places the screenshot alone, and a failed screenshot places nothing to draw on', () => {
    expect(run([{ type: 'press', press: 1 }, { type: 'release' }, { type: 'taken', press: 1, shot }]).placed).toEqual([
      { shot, box: null },
    ]);
    expect(
      run([
        { type: 'press', press: 1 },
        { type: 'drew', box },
        { type: 'release' },
        { type: 'taken', press: 1, shot: null },
      ]).placed,
    ).toEqual([{ shot: null, box }]);
  });

  test("ignores a second press under way, and an earlier press's screenshot", () => {
    const stale = run([
      { type: 'press', press: 2 },
      { type: 'press', press: 3 },
      { type: 'taken', press: 1, shot: { ...shot, index: 9 } },
      { type: 'drew', box },
      { type: 'release' },
    ]);
    expect(stale.placed).toEqual([]);
    expect(stale.state).toEqual({ kind: 'pressed', press: 2, box, shot: 'taking', released: true });
    expect(run([{ type: 'taken', press: 2, shot }, { type: 'release' }]).state).toBe(LIVE_IDLE);
  });

  test('a drag after the pointer was let go changes nothing', () => {
    const later = { ...box, rect: { ...box.rect, x: 99 } };
    const done = run([
      { type: 'press', press: 1 },
      { type: 'drew', box },
      { type: 'release' },
      { type: 'drew', box: later },
      { type: 'taken', press: 1, shot },
    ]);
    expect(done.placed).toEqual([{ shot, box }]);
  });

  test('shows the screenshot in place of the frame held still once it arrives, while the drag goes on', () => {
    expect(liveShown(LIVE_IDLE)).toEqual({ frozen: false, shot: null, box: null });
    expect(liveShown(run([{ type: 'press', press: 1 }]).state)).toEqual({ frozen: true, shot: null, box: null });
    const arrived = run([
      { type: 'press', press: 1 },
      { type: 'taken', press: 1, shot },
    ]);
    expect(liveShown(arrived.state)).toEqual({ frozen: true, shot, box: null });
    // A screenshot that failed leaves the frame held still.
    const failed = run([
      { type: 'press', press: 1 },
      { type: 'taken', press: 1, shot: null },
    ]);
    expect(liveShown(failed.state)).toEqual({ frozen: true, shot: null, box: null });
  });

  test('keeps the box drawn outlined from when the pointer is let go until its screenshot arrives', () => {
    const waiting = run([{ type: 'press', press: 1 }, { type: 'drew', box }, { type: 'release' }]);
    expect(liveShown(waiting.state)).toEqual({ frozen: true, shot: null, box });
    expect(liveShown(liveDraw(waiting.state, { type: 'taken', press: 1, shot }).state).box).toBeNull();
  });

  test('a frame that arrives while the view holds still does not change the size the box is measured in', () => {
    const held = { width: 800, height: 450 };
    expect(liveSizeAfter(held, { width: 640, height: 480 }, true)).toBe(held);
    expect(liveSizeAfter(held, { width: 640, height: 480 }, false)).toEqual({ width: 640, height: 480 });
    expect(liveSizeAfter(held, { ...held }, false)).toBe(held);
    expect(liveSizeAfter(null, held, false)).toBe(held);
  });
});
