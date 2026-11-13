import { describe, expect, test } from 'bun:test';

import { LIMITS } from '../src/lib/bindings.ts';
import { boxStep } from '../src/lib/boxes.ts';
import {
  type Draft,
  buildCharacters,
  buildSpec,
  DEFAULT_TOLERANCE,
  EMPTY_DRAFT,
  boxBlocked,
  findBlocked,
  fixDraft,
  fromReference,
  onPicture,
  newestOfSize,
  recordBlocked,
  recordedPlace,
  regionScreenshot,
  reviewSpec,
  settingsDiffer,
  stateOn,
  toReference,
  withFixRegion,
  withRecordedInk,
  withKind,
  withoutState,
  withState,
  withStateAt,
  withStateStep,
} from '../src/lib/check.ts';

const region = { x: 1, y: 2, width: 3, height: 4 };
const red = { red: 255, green: 0, blue: 0 };
const gray = { red: 40, green: 40, blue: 40 };
const percent = { app: 'example', id: 'percent' };

describe('buildSpec', () => {
  test('asks for a region first', () => {
    expect(buildSpec(EMPTY_DRAFT, 'example')).toEqual({ ok: false, next: 'Draw a region on a screenshot.' });
  });

  test("builds a number from the application's reference and whether it is whole", () => {
    const pick = 'Pick a reference to read the number with, or make one from this region.';
    expect(buildSpec({ ...EMPTY_DRAFT, region }, 'example')).toEqual({ ok: false, next: pick });
    expect(buildSpec({ ...EMPTY_DRAFT, region, reference: 'percent', whole: true }, 'example')).toEqual({
      ok: true,
      spec: { kind: 'number', region, reference: percent, whole: true },
    });
  });

  test('builds a bar fill from both colors', () => {
    const bar = withKind({ ...EMPTY_DRAFT, region }, 'barFill');
    expect(buildSpec(bar, 'example')).toEqual({ ok: false, next: "Pick the bar's filled color." });
    expect(buildSpec({ ...bar, filled: red }, 'example')).toEqual({ ok: false, next: "Pick the bar's empty color." });
    expect(buildSpec({ ...bar, filled: red, empty: gray }, 'example')).toEqual({
      ok: true,
      spec: { kind: 'barFill', region, filled: red, empty: gray },
    });
  });

  test('builds a color check', () => {
    const color = withKind({ ...EMPTY_DRAFT, region }, 'color');
    expect(buildSpec(color, 'example')).toEqual({ ok: false, next: 'Pick the color to look for.' });
    expect(buildSpec({ ...color, color: red }, 'example')).toEqual({
      ok: true,
      spec: { kind: 'color', region, color: red, tolerance: DEFAULT_TOLERANCE.color },
    });
  });

  test('builds a state check from its named states', () => {
    const empty = withKind({ ...EMPTY_DRAFT, region, drawnOn: 1 }, 'state');
    expect(buildSpec(empty, 'example')).toEqual({ ok: false, next: 'Add a state from a screenshot that shows it.' });
    const two = withState(withState(empty, 1), 3);
    expect(two.states.map((state) => state.name)).toEqual(['State 1', 'State 2']);
    expect(buildSpec(two, 'example')).toEqual({
      ok: true,
      spec: {
        kind: 'state',
        region,
        drawnOn: 1,
        states: [
          { name: 'State 1', screenshot: 1 },
          { name: 'State 2', screenshot: 3 },
        ],
        tolerance: DEFAULT_TOLERANCE.state,
      },
    });
    const named = withStateAt(two, 0, (state) => ({ ...state, name: '  Paused ' }));
    const built = buildSpec(named, 'example');
    expect(built.ok && built.spec.kind === 'state' ? built.spec.states[0]?.name : null).toBe('Paused');
    expect(
      buildSpec(
        withStateAt(two, 1, (state) => ({ ...state, name: ' ' })),
        'example',
      ),
    ).toEqual({
      ok: false,
      next: 'Name each state.',
    });
    expect(buildSpec({ ...two, drawnOn: null }, 'example')).toEqual({
      ok: false,
      next: 'Draw a region on a screenshot.',
    });
  });

  test('a state check compares only what was selected on each state while it ignores the background', () => {
    const step = { kind: 'click', x: 4, y: 5, combine: 'replace' } as const;
    const steps = [step];
    const draft = {
      ...withStateStep(withState(withKind({ ...EMPTY_DRAFT, region, drawnOn: 3 }, 'state'), 2), 3, step),
      colorMatch: 30,
    };
    expect(buildSpec(draft, 'app')).toEqual({
      ok: true,
      spec: {
        kind: 'state',
        region,
        drawnOn: 3,
        states: [
          { name: 'State 1', screenshot: 2 },
          { name: 'State 2', screenshot: 3, selection: { tolerance: 30, steps } },
        ],
        tolerance: DEFAULT_TOLERANCE.state,
      },
    });
    const ignoring = buildSpec({ ...draft, ignoreBackground: false }, 'app');
    expect(ignoring.ok && ignoring.spec.kind === 'state' ? ignoring.spec.states[1] : null).toEqual({
      name: 'State 2',
      screenshot: 3,
    });
  });
});

describe('states', () => {
  const draft = withKind(EMPTY_DRAFT, 'state');
  const click = (combine: 'replace' | 'add' | 'remove', x = 1) => ({ kind: 'click', x, y: 0, combine }) as const;

  test('a screenshot holds at most one state, and a check at most sixteen', () => {
    const once = withState(draft, 4);
    expect(withState(once, 4)).toBe(once);
    expect(stateOn(once, 4)).toBe(0);
    expect(stateOn(once, 5)).toBe(-1);
    expect(stateOn(once, null)).toBe(-1);
    let full = draft;
    for (let screenshot = 0; screenshot < LIMITS.maxStates; screenshot += 1) {
      full = withState(full, screenshot);
    }
    expect(full.states).toHaveLength(LIMITS.maxStates);
    expect(withState(full, 99)).toBe(full);
  });

  test('a new state takes the first free number', () => {
    const renamed = withStateAt(withState(draft, 0), 0, (state) => ({ ...state, name: 'state 2' }));
    expect(withState(renamed, 1).states.map((state) => state.name)).toEqual(['state 2', 'State 3']);
    const removed = withoutState(withState(withState(draft, 0), 1), 0);
    expect(removed.states.map((state) => [state.name, state.screenshot])).toEqual([['State 2', 1]]);
    expect(withState(removed, 2).states.map((state) => state.name)).toEqual(['State 2', 'State 3']);
  });

  test("a selection step goes to its screenshot's state, adding one when needed", () => {
    const first = withStateStep(draft, 7, click('replace'));
    expect(first.states).toEqual([{ name: 'State 1', screenshot: 7, steps: [click('replace')] }]);
    const added = withStateStep(first, 7, click('add', 2));
    expect(added.states[0]?.steps).toEqual([click('replace'), click('add', 2)]);
    const other = withStateStep(added, 8, click('add', 3));
    expect(other.states.map((state) => state.steps)).toEqual([
      [click('replace'), click('add', 2)],
      [click('replace', 3)],
    ]);
    // Taking away from nothing changes nothing, and adds no state.
    expect(withStateStep(draft, 7, click('remove'))).toBe(draft);
    expect(withStateAt(draft, 3, (state) => state)).toBe(draft);
  });
});

describe('buildCharacters', () => {
  test('teaches a new reference its boxes in list order, each at its own region, skipping blank ones', () => {
    const other = { x: 40, y: 50, width: 30, height: 12 };
    const draft = {
      ...EMPTY_DRAFT,
      ink: red,
      boxes: [
        { id: 4, screenshot: 2, region, text: ' 87% ' },
        { id: 1, screenshot: 0, region: other, text: '12%' },
        { id: 2, screenshot: 1, region, text: '  ' },
      ],
    };
    expect(buildCharacters(draft, null)).toEqual({
      ok: true,
      spec: {
        kind: 'characters',
        // With no value's region, the reference reads where its first box is.
        region,
        screenshot: 2,
        reference: null,
        ink: red,
        tolerance: DEFAULT_TOLERANCE.number,
        labels: [
          { screenshot: 2, region, text: '87%' },
          { screenshot: 0, region: other, text: '12%' },
        ],
        recorded: [],
      },
    });
  });

  test("reads at the value's region while the value has one", () => {
    const value = { x: 9, y: 9, width: 20, height: 8 };
    const built = buildCharacters(
      { ...EMPTY_DRAFT, region: value, drawnOn: 3, ink: red, boxes: [{ id: 0, screenshot: 1, region, text: '5' }] },
      null,
    );
    expect(built.ok && [built.spec.region, built.spec.screenshot]).toEqual([value, 3]);
  });

  test('teaches kept recorded readings alone, in reading order', () => {
    const draft = {
      ...EMPTY_DRAFT,
      region,
      drawnOn: 0,
      ink: red,
      recorded: new Map([
        [4, '9%'],
        [1, ' 8% '],
        [2, ''],
      ]),
    };
    const built = buildCharacters(draft, null);
    expect(built.ok && built.spec.recorded).toEqual([
      { reading: 1, text: '8%' },
      { reading: 4, text: '9%' },
    ]);
  });

  test('asks for what a new reference needs', () => {
    expect(buildCharacters(EMPTY_DRAFT, null)).toEqual({
      ok: false,
      next: 'Draw a box around the characters on a screenshot.',
    });
    expect(buildCharacters({ ...EMPTY_DRAFT, region, ink: red }, null)).toEqual({
      ok: false,
      next: 'Draw a box around the characters on a screenshot.',
    });
    expect(buildCharacters({ ...EMPTY_DRAFT, region, drawnOn: 0 }, null)).toEqual({
      ok: false,
      next: "Pick the characters' color.",
    });
    expect(
      buildCharacters(
        { ...EMPTY_DRAFT, region, drawnOn: 0, ink: red, boxes: [{ id: 0, screenshot: 0, region, text: ' ' }] },
        null,
      ),
    ).toEqual({
      ok: false,
      next: 'Type what a box shows, or record the characters.',
    });
  });

  test('a saved reference needs nothing new to be tried', () => {
    const built = buildCharacters({ ...EMPTY_DRAFT, region, drawnOn: 0, ink: red }, percent);
    expect(built.ok && built.spec.reference).toEqual(percent);
  });
});

test('reviewSpec waits for the region and color, and names the reference', () => {
  expect(reviewSpec({ ...EMPTY_DRAFT, region, drawnOn: 1 }, [], null)).toBeNull();
  expect(reviewSpec({ ...EMPTY_DRAFT, region, ink: red }, [], null)).toBeNull();
  const kept = [{ reading: 0, text: '5' }];
  expect(reviewSpec({ ...EMPTY_DRAFT, region, drawnOn: 1, ink: red }, kept, percent)).toEqual({
    kind: 'characters',
    region,
    screenshot: 1,
    reference: percent,
    ink: red,
    tolerance: DEFAULT_TOLERANCE.number,
    labels: [],
    recorded: kept,
  });
});

test('withKind keeps the region and colors and resets the tolerance', () => {
  const draft = withKind({ ...EMPTY_DRAFT, region, ink: red, tolerance: 99 }, 'color');
  expect(draft.kind).toBe('color');
  expect(draft.region).toEqual(region);
  expect(draft.ink).toEqual(red);
  expect(draft.tolerance).toBe(DEFAULT_TOLERANCE.color);
});

describe('the reference editor', () => {
  const region = { x: 1, y: 2, width: 30, height: 10 };
  const taught = { x: 5, y: 6, width: 40, height: 12 };
  const saved = {
    id: 'percent',
    name: 'Percent',
    ink: red,
    tolerance: 25,
    samples: [{ image: 'ref-percent-1.png', text: '87%' }],
  };

  test("opens with the reference's characters and the value's region, and no boxes", () => {
    const value = { ...EMPTY_DRAFT, region, ink: gray, boxes: [{ id: 0, screenshot: 0, region, text: '5' }] };
    expect(toReference(value, saved)).toEqual({ ...value, ink: red, tolerance: 25, boxes: [] });
    expect(toReference(value, null)).toEqual({ ...value, ink: null, tolerance: DEFAULT_TOLERANCE.number, boxes: [] });
  });

  test("leaves the value's region as it was, without the editor's boxes or readings", () => {
    const editor = {
      ...toReference({ ...EMPTY_DRAFT, region, drawnOn: 0 }, saved),
      boxes: [{ id: 0, screenshot: 3, region: taught, text: '100%' }],
      recorded: new Map([[0, '99%']]),
    };
    expect(fromReference(editor)).toEqual({ ...editor, boxes: [], recorded: new Map() });
  });
});

describe('a stopped recording', () => {
  const settings = { region, ink: red, tolerance: 40, windowSize: { width: 1920, height: 1080 } };
  const recorded = { ...EMPTY_DRAFT, region, drawnOn: 0, ink: red, tolerance: 40 };
  const big = { width: 1920, height: 1080 };
  const small = { width: 1280, height: 720 };

  test('records the same while the draft has its settings at its window size', () => {
    expect(settingsDiffer({ ...recorded, region: { ...region } }, settings, { ...big })).toBe(false);
  });

  test('offers its settings back for any other region, color, tolerance or window size', () => {
    expect(settingsDiffer({ ...recorded, region: { ...region, x: 2 } }, settings, big)).toBe(true);
    expect(settingsDiffer({ ...recorded, ink: gray }, settings, big)).toBe(true);
    expect(settingsDiffer({ ...recorded, ink: null }, settings, big)).toBe(true);
    expect(settingsDiffer({ ...recorded, tolerance: 41 }, settings, big)).toBe(true);
    expect(settingsDiffer(recorded, settings, small)).toBe(true);
    expect(settingsDiffer(recorded, settings, null)).toBe(true);
  });

  test('goes back on the newest screenshot of its window size', () => {
    const screenshots = [
      [0, big],
      [1, small],
      [2, big],
      [3, small],
    ] as const;
    expect(newestOfSize(screenshots, { ...big })).toBe(2);
    expect(newestOfSize(screenshots, { width: 800, height: 600 })).toBeNull();
  });

  // A reference taught by itself: no value's region, and it reads at its first box.
  const first = { id: 0, screenshot: 0, region, text: '87%' };
  const other = { id: 1, screenshot: 0, region: { ...region, y: 40 }, text: '12' };
  const alone = { ...EMPTY_DRAFT, ink: red, tolerance: 40, boxes: [first, other] };
  const sizes = [
    [0, big],
    [1, small],
    [2, big],
  ] as const;

  /** `draft` after Use settings, with `boxId` the box the recording read at. */
  function usedSettings(draft: Draft, boxId: number | null): Draft {
    const on = draft.boxes.find((box) => box.id === boxId)?.screenshot ?? null;
    const place = recordedPlace(draft, settings, on, sizes);
    const { boxes } =
      place === null
        ? draft
        : boxStep(
            { boxes: draft.boxes, selectedId: null, recordedId: boxId },
            { type: 'restore', screenshot: place, region: settings.region },
          );
    return withRecordedInk({ ...draft, boxes }, settings);
  }

  test('using its settings in a reference taught by itself moves the reading box back and sets no region', () => {
    // The reading box was moved, then deleted, and the color and tolerance changed.
    const moved = { ...alone, ink: gray, tolerance: 12, boxes: [{ ...first, region: { ...region, x: 9 } }, other] };
    const deleted = { ...alone, boxes: [other] };
    for (const changed of [moved, deleted]) {
      expect(settingsDiffer(changed, settings, big)).toBe(true);
      const back = usedSettings(changed, 0);
      expect(back.region).toBeNull();
      expect(back.drawnOn).toBeNull();
      expect(settingsDiffer(back, settings, big)).toBe(false);
      expect(back.boxes).toHaveLength(2);
    }
    // The box keeps its text and the other box stays; a deleted box comes back with none.
    expect(usedSettings(moved, 0).boxes).toEqual([first, other]);
    // It goes on the newest screenshot of the size recorded at.
    expect(usedSettings(deleted, 0).boxes).toEqual([{ id: 2, screenshot: 2, region, text: '' }, other]);
  });

  test('the reading box goes back on a screenshot of the size recorded at', () => {
    // Its own screenshot when that has the size, or else the newest that has, or nowhere when none has.
    expect(recordedPlace(alone, settings, 0, sizes)).toBe(0);
    expect(recordedPlace(alone, settings, 1, sizes)).toBe(2);
    expect(recordedPlace(alone, settings, null, sizes)).toBe(2);
    expect(recordedPlace(alone, settings, 1, [[1, small]])).toBeNull();
  });

  test("using its settings in a reference taught from a value leaves the value's region and the boxes", () => {
    const taughtFrom = { ...recorded, ink: gray, tolerance: 12, boxes: [other] };
    expect(recordedPlace(taughtFrom, settings, null, sizes)).toBeNull();
    expect(usedSettings(taughtFrom, null)).toEqual({ ...taughtFrom, ink: red, tolerance: 40 });
  });
});

test('the region shows over the screenshot shown, else the one it was drawn on, else the newest', () => {
  const full = new Map([
    [1, null],
    [2, null],
  ]);
  expect(regionScreenshot(1, 2, full, 3)).toBe(1);
  // A screenshot whose pixels have not arrived yet does not count.
  expect(regionScreenshot(3, 2, full, 3)).toBe(2);
  expect(regionScreenshot(null, 2, full, 3)).toBe(2);
  expect(regionScreenshot(null, 4, full, 3)).toBe(3);
  expect(regionScreenshot(null, null, new Map(), null)).toBeNull();
});

test("recording waits for a place to read and the characters' color", () => {
  const ready = { ...EMPTY_DRAFT, region, drawnOn: 0, ink: red };
  const drawFirst = 'Draw a box around the characters on a screenshot to record them.';
  expect(recordBlocked(ready)).toBeNull();
  expect(recordBlocked({ ...ready, region: null })).toBe(drawFirst);
  expect(recordBlocked({ ...ready, drawnOn: null })).toBe(drawFirst);
  // A box is a place to read while the value has none.
  expect(recordBlocked({ ...ready, region: null, boxes: [{ id: 0, screenshot: 1, region, text: '' }] })).toBeNull();
  expect(recordBlocked({ ...ready, ink: null })).toBe("Pick the characters' color to record.");
});

test('finding more like it waits for a selected box and the color', () => {
  const ready = { ...EMPTY_DRAFT, ink: red };
  const box = { id: 0, screenshot: 0, region, text: '' };
  expect(findBlocked(ready, box)).toBeNull();
  expect(findBlocked(ready, null)).toBe('Select a box around one sample first.');
  expect(findBlocked({ ...ready, ink: null }, box)).toBe("Pick the characters' color first.");
});

test('boxes go on screenshots of the size the reference reads at, once it has one', () => {
  const size = { width: 800, height: 600 };
  expect(boxBlocked(null, size)).toBeNull();
  expect(boxBlocked(size, null)).toBeNull();
  expect(boxBlocked(size, { ...size })).toBeNull();
  const other = 'Boxes go on screenshots at 800 × 600, the size the reference reads at.';
  expect(boxBlocked(size, { width: 800, height: 601 })).toBe(other);
  expect(boxBlocked(size, { width: 801, height: 600 })).toBe(other);
});

test("a new screenshot carries the value's region when it fits, and clears it when it does not", () => {
  const drawn = { ...EMPTY_DRAFT, region, drawnOn: 0 };
  expect(onPicture(drawn, 1, true)).toEqual({ ...drawn, drawnOn: 1 });
  expect(onPicture(drawn, 1, false)).toEqual({ ...drawn, region: null, drawnOn: null });
  expect(onPicture(EMPTY_DRAFT, 1, true)).toBe(EMPTY_DRAFT);
  // A saved value's region waits, on no screenshot, for one of the size it was drawn at.
  const saved = { ...EMPTY_DRAFT, region };
  expect(onPicture(saved, 1, false)).toBe(saved);
  // While a size is unknown, such as before the screenshot's pixels arrive, the region stays where it is drawn.
  expect(onPicture(drawn, 1, null)).toBe(drawn);
});

describe('fixDraft', () => {
  test("starts from the saved check's kind and settings, with no region", () => {
    expect(fixDraft({ kind: 'number', reference: 'percent', whole: true })).toEqual({
      ...EMPTY_DRAFT,
      reference: 'percent',
      whole: true,
    });
    expect(fixDraft({ kind: 'barFill', filled: red, empty: gray })).toEqual({
      ...EMPTY_DRAFT,
      kind: 'barFill',
      tolerance: DEFAULT_TOLERANCE.barFill,
      filled: red,
      empty: gray,
    });
    expect(fixDraft({ kind: 'color', color: red, tolerance: 7 })).toEqual({
      ...EMPTY_DRAFT,
      kind: 'color',
      tolerance: 7,
      color: red,
    });
  });

  test('starts its region where the value reads on its first screenshot, unless one is drawn', () => {
    const draft = fixDraft({ kind: 'barFill', filled: red, empty: gray });
    expect(withFixRegion(draft, region, 2)).toEqual({ ...draft, region, drawnOn: 2 });
    // Scaled past the window, the value reads nowhere there.
    expect(withFixRegion(draft, null, 2)).toBe(draft);
    const drawn = { ...draft, region: { ...region, x: 9 }, drawnOn: 5 };
    expect(withFixRegion(drawn, region, 2)).toBe(drawn);
  });

  test('names the states taken at the new size after the saved ones, in order', () => {
    const draft = fixDraft({
      kind: 'state',
      states: [
        { name: 'Shown', image: 'icon-1.png' },
        { name: 'Hidden', image: 'icon-2.png' },
      ],
      tolerance: 9,
    });
    expect(draft.tolerance).toBe(9);
    const first = withState(draft, 4);
    expect(first.states.map((state) => state.name)).toEqual(['Shown']);
    // A name typed over a saved one leaves that one for the next state.
    const renamed = withStateAt(first, 0, (state) => ({ ...state, name: 'Hidden' }));
    expect(withState(renamed, 5).states.map((state) => state.name)).toEqual(['Hidden', 'Shown']);
    const both = withState(first, 5);
    expect(withState(both, 6).states.map((state) => state.name)).toEqual(['Shown', 'Hidden', 'State 3']);
  });
});
