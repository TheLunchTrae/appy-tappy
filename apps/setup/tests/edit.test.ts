import { describe, expect, test } from 'bun:test';

import type { ValueDef } from '../src/lib/bindings.ts';
import { DEFAULT_TOLERANCE, type Draft, EMPTY_DRAFT, withKind, withState, withStateAt } from '../src/lib/check.ts';
import { AUTOMATIC } from '../src/lib/pin.ts';
import { held } from '../src/lib/unsaved.ts';
import {
  agreedFixes,
  buildEdit,
  droppedFixesText,
  editDraft,
  editStart,
  keepsStates,
  otherSizeText,
  pickedPin,
  placedOn,
  saveable,
} from '../src/lib/edit.ts';

const region = { x: 10, y: 20, width: 30, height: 40 };
const windowSize = { width: 400, height: 200 };
const red = { red: 255, green: 0, blue: 0 };
const gray = { red: 40, green: 40, blue: 40 };

function value(check: ValueDef['screen']['check']): ValueDef {
  return { id: 'status', name: 'Status', screen: { windowSize, region, check, pin: 'topLeft', fixes: [] } };
}

const states = value({
  kind: 'state',
  states: [
    { name: 'Shown', image: 'status-1.png' },
    { name: 'Hidden', image: 'status-2.png' },
  ],
  tolerance: 9,
});
const bar = value({ kind: 'barFill', filled: red, empty: gray });
const number = value({ kind: 'number', reference: 'percent', whole: true });

/** The edit `draft` makes of `saved`, under its own name. */
function edit(draft: Parameters<typeof buildEdit>[0], saved: ValueDef, name = saved.name) {
  return buildEdit(draft, saved, 'example', name, 'bottomRight', false);
}

describe('editDraft', () => {
  test('opens with the saved region and settings, on no screenshot', () => {
    expect(editDraft(bar.screen)).toEqual({
      ...EMPTY_DRAFT,
      kind: 'barFill',
      tolerance: DEFAULT_TOLERANCE.barFill,
      region,
      filled: red,
      empty: gray,
    });
    const draft = editDraft(states.screen);
    expect(draft.stateNames).toEqual(['Shown', 'Hidden']);
    expect(draft.states).toEqual([]);
    expect(draft.drawnOn).toBeNull();
  });
});

test('the region sits on the first screenshot only when it has the size the value was drawn at', () => {
  const draft = editDraft(bar.screen);
  expect(placedOn(draft, bar.screen, 0, windowSize)).toEqual({ ...draft, drawnOn: 0 });
  expect(placedOn(draft, bar.screen, 0, { width: 800, height: 600 })).toBe(draft);
  // A region that sits on a screenshot already stays there.
  const placed = { ...draft, drawnOn: 3 };
  expect(placedOn(placed, bar.screen, 4, windowSize)).toBe(placed);
});

test('an edit holds unsaved work once anything it opened with changed, the side and the reading area included', () => {
  const centered = { ...bar, screen: { ...bar.screen, pin: 'center' as const, readingArea: true } };
  const start = editStart(centered);
  expect(start).toEqual({ valueName: 'Status', mayMove: true, draft: editDraft(centered.screen) });
  expect(start.draft.pin).toEqual({ kind: 'byHand', pin: 'center' });
  expect(editStart(bar).mayMove).toBe(false);
  expect(held(false, start, editStart(centered))).toBe('nothing');
  // Another side, back to automatic, or the reading area turned off is the only change.
  const picked = (pin: Draft['pin']) => ({ ...start, draft: { ...start.draft, pin } });
  expect(held(false, start, picked({ kind: 'byHand', pin: 'topRight' }))).toBe('unsaved');
  expect(held(false, start, picked(AUTOMATIC))).toBe('unsaved');
  expect(held(false, start, { ...start, mayMove: false })).toBe('unsaved');
  // Once the capture's own screenshot holds the region, that is where the edit starts.
  const placed = { ...start, draft: placedOn(start.draft, centered.screen, 0, windowSize) };
  expect(held(false, placed, { ...placed })).toBe('nothing');
  expect(held(false, placed, { ...placed, valueName: 'Health' })).toBe('unsaved');
});

test('a pin opens as picked by hand only when it is saved and is not the side the region is nearest', () => {
  // The region's center is in the window's top left third.
  expect(pickedPin(bar.screen)).toEqual(AUTOMATIC);
  expect(pickedPin({ ...bar.screen, pin: 'center' })).toEqual({ kind: 'byHand', pin: 'center' });
  const { pin: _, ...unpinned } = bar.screen;
  expect(pickedPin(unpinned)).toEqual(AUTOMATIC);
});

test('a value drawn at another size says both sizes', () => {
  expect(otherSizeText(bar, { width: 800, height: 600 })).toBe(
    'Status was drawn at 400 × 200, and the window is 800 × 600 on this screenshot.',
  );
});

test('the fixes an edit removes are named by their sizes', () => {
  const why =
    'A fix reads the value as it is first drawn: with the same kind of check, and the same states in the same order.';
  expect(droppedFixesText([{ width: 800, height: 600 }])).toBe(`Saving this removes its fix at 800 × 600. ${why}`);
  expect(droppedFixesText([windowSize, { width: 800, height: 600 }])).toBe(
    `Saving this removes its fixes at 400 × 200 and 800 × 600. ${why}`,
  );
});

test('an edit is saved once nothing stops it and the user agreed to lose exactly the fixes it removes', () => {
  const larger = { width: 800, height: 600 };
  const clean = { unread: null, droppedFixes: [], refused: null };
  const drops = { ...clean, droppedFixes: [larger] };
  expect(saveable(null, [])).toBe(false);
  expect(saveable(clean, [])).toBe(true);
  // What the window's size now leaves unread does not stop a save.
  expect(saveable({ ...clean, unread: 'Not read at this size.' }, [])).toBe(true);
  expect(saveable({ ...clean, refused: 'Pause music uses Status.' }, [])).toBe(false);
  expect(saveable(drops, [])).toBe(false);
  expect(saveable(drops, [{ ...larger }])).toBe(true);
  // The save carries the sizes agreed to, which the owner checks again.
  expect(agreedFixes(drops, [{ ...larger }])).toEqual([larger]);
  expect(agreedFixes(drops, [])).toEqual([]);
  // Agreeing to lose one fix is not agreeing to lose another, or more.
  expect(saveable(drops, [windowSize])).toBe(false);
  expect(saveable({ ...clean, droppedFixes: [larger, windowSize] }, [larger])).toBe(false);
  expect(agreedFixes({ ...clean, droppedFixes: [larger, windowSize] }, [larger])).toEqual([]);
  // An edit that removes none drops none, whatever was agreed to before.
  expect(agreedFixes(clean, [larger])).toEqual([]);
  expect(saveable(clean, [larger])).toBe(true);
  expect(agreedFixes(null, [])).toEqual([]);
});

describe('buildEdit', () => {
  test('keeps the drawing while the region is where it was saved, whatever the window', () => {
    const draft = { ...editDraft(bar.screen), filled: gray, empty: red };
    expect(edit(draft, bar, ' Health ')).toEqual({
      ok: true,
      spec: {
        app: 'example',
        value: 'status',
        valueName: 'Health',
        pin: 'bottomRight',
        readingArea: false,
        drawing: { kind: 'kept', check: { kind: 'barFill', filled: gray, empty: red } },
      },
    });
    // A screenshot the region sits on changes nothing while the region stays.
    expect(edit({ ...draft, drawnOn: 3 }, bar)).toEqual(edit(draft, bar));
    expect(edit(editDraft(number.screen), number)).toMatchObject({
      spec: { drawing: { kind: 'kept', check: { kind: 'number', reference: 'percent', whole: true } } },
    });
    const color = { ...withKind(editDraft(bar.screen), 'color'), color: red };
    expect(edit(color, bar)).toMatchObject({
      spec: { drawing: { kind: 'kept', check: { kind: 'color', color: red, tolerance: DEFAULT_TOLERANCE.color } } },
    });
  });

  test('carries whether the region is a reading area, and needs a side', () => {
    const draft = editDraft(bar.screen);
    expect(buildEdit(draft, bar, 'example', 'Status', 'topLeft', true)).toMatchObject({
      spec: { pin: 'topLeft', readingArea: true },
    });
    expect(buildEdit(draft, bar, 'example', 'Status', null, false)).toEqual({
      ok: false,
      next: 'Draw a region on a screenshot.',
    });
  });

  test('asks for a name and for what the check still needs', () => {
    expect(edit(editDraft(bar.screen), bar, '  ')).toEqual({ ok: false, next: 'Name the value.' });
    expect(edit({ ...editDraft(bar.screen), filled: null }, bar)).toEqual({
      ok: false,
      next: "Pick the bar's filled color.",
    });
  });

  test('draws a region that changed on the screenshot it was drawn on', () => {
    const moved = { ...editDraft(bar.screen), region: { ...region, x: 11 } };
    const shot = 'Take a screenshot while the window is 400 × 200, the size Status was drawn at.';
    expect(edit(moved, bar)).toEqual({ ok: false, next: shot });
    expect(edit({ ...moved, drawnOn: 2 }, bar)).toMatchObject({
      spec: {
        drawing: {
          kind: 'drawn',
          screenshot: 2,
          check: { kind: 'barFill', region: { ...region, x: 11 }, filled: red, empty: gray },
        },
      },
    });
  });

  test('keeps the saved pictures of states that are only renamed', () => {
    const draft = { ...editDraft(states.screen), tolerance: 4, stateNames: ['Shown', ' Away '] };
    expect(keepsStates(draft, states.screen)).toBe(true);
    expect(edit(draft, states)).toMatchObject({
      spec: {
        drawing: {
          kind: 'kept',
          check: {
            kind: 'state',
            states: [
              { name: 'Shown', image: 'status-1.png' },
              { name: 'Away', image: 'status-2.png' },
            ],
            tolerance: 4,
          },
        },
      },
    });
    expect(edit({ ...draft, stateNames: ['Shown', ' '] }, states)).toEqual({ ok: false, next: 'Name each state.' });
  });

  test('takes every state again once the region changed or one is taken from a screenshot', () => {
    const draft = { ...editDraft(states.screen), drawnOn: 0 };
    const moved = { ...draft, region: { ...region, y: 21 } };
    expect(keepsStates(moved, states.screen)).toBe(false);
    expect(edit(moved, states)).toEqual({
      ok: false,
      next: 'The region changed, so each state needs a new screenshot. Add each state from one that shows it.',
    });
    // The first state taken ends the saved pictures, and takes the first saved name.
    const first = withState(draft, 0);
    expect(keepsStates(first, states.screen)).toBe(false);
    expect(edit(first, states)).toMatchObject({
      spec: {
        drawing: {
          kind: 'drawn',
          screenshot: 0,
          check: { kind: 'state', region, drawnOn: 0, states: [{ name: 'Shown', screenshot: 0 }], tolerance: 9 },
        },
      },
    });
    // Removing it again reads with the saved pictures.
    expect(keepsStates({ ...first, states: [] }, states.screen)).toBe(true);
    // A state can only come from a screenshot of the size the value was drawn at.
    expect(edit({ ...first, drawnOn: null }, states)).toEqual({
      ok: false,
      next: 'Take a screenshot while the window is 400 × 200, the size Status was drawn at.',
    });
    expect(
      edit(
        withStateAt(first, 0, (state) => ({ ...state, name: '' })),
        states,
      ),
    ).toEqual({
      ok: false,
      next: 'Name each state.',
    });
  });

  test('a value that becomes a state check has no saved pictures to keep', () => {
    const draft = { ...withKind(editDraft(bar.screen), 'state'), drawnOn: 0 };
    expect(keepsStates(draft, bar.screen)).toBe(false);
    expect(edit(draft, bar)).toEqual({ ok: false, next: 'Add a state from a screenshot that shows it.' });
  });
});
