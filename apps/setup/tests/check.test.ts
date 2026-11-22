import { describe, expect, test } from 'bun:test';

import { LIMITS, NO_STATE } from '../src/lib/bindings.ts';
import { TOO_MANY } from '../src/lib/expected.ts';
import { boxStep } from '../src/lib/boxes.ts';
import {
  type Draft,
  type DraftState,
  buildCharacters,
  buildReferenceSave,
  buildSpec,
  savedNeeds,
  CHECKS,
  CHECK_KINDS,
  EMPTY_DRAFT,
  boxBlocked,
  findBlocked,
  fixDraft,
  onPicture,
  newestOfSize,
  recordBlocked,
  recordedPlace,
  regionScreenshot,
  firstWithPixels,
  stateNameProblems,
  stateNamesNext,
  reviewSpec,
  settingsDiffer,
  stateOn,
  withoutSample,
  withFixRegion,
  withKind,
  withoutState,
  withState,
  withStateAt,
  withStateStep,
  NEEDS_REFERENCE,
} from '../src/lib/check.ts';

const region = { x: 1, y: 2, width: 3, height: 4 };
const red = { red: 255, green: 0, blue: 0 };
const gray = { red: 40, green: 40, blue: 40 };
const percent = { app: 'example', id: 'percent' };

describe('the kinds of check', () => {
  test("each is offered in the table's order, and its entry builds a check of its own kind", () => {
    expect(CHECK_KINDS).toEqual(['number', 'barFill', 'color', 'state']);
    const complete: Draft = {
      ...EMPTY_DRAFT,
      region,
      drawnOn: 1,
      reference: 'percent',
      filled: red,
      empty: gray,
      color: red,
      states: [{ name: 'Paused', screenshot: 1, steps: [] }],
    };
    for (const kind of CHECK_KINDS) {
      const built = buildSpec(withKind(complete, kind), 'example');
      expect(built.ok ? built.spec.kind : built.next).toBe(kind);
    }
  });
});

describe('buildSpec', () => {
  test('asks for a region first', () => {
    expect(buildSpec(EMPTY_DRAFT, 'example')).toEqual({ ok: false, next: 'Draw a region on a screenshot.' });
  });

  test("builds a number from the application's reference and whether it is whole", () => {
    expect(buildSpec({ ...EMPTY_DRAFT, region }, 'example')).toEqual({ ok: false, next: NEEDS_REFERENCE });
    expect(buildSpec({ ...EMPTY_DRAFT, region, reference: 'percent', whole: true }, 'example')).toEqual({
      ok: true,
      spec: { kind: 'number', region, reference: percent, whole: true },
    });
  });

  test('a number names the characters it needs once some are chosen, up to the most a value saves', () => {
    const number = { ...EMPTY_DRAFT, region, reference: 'percent' };
    expect(buildSpec({ ...number, characters: ['1', '2', '%'] }, 'example')).toEqual({
      ok: true,
      spec: { kind: 'number', region, reference: percent, whole: false, characters: '12%' },
    });
    expect(buildSpec({ ...number, characters: [] }, 'example')).toEqual({
      ok: true,
      spec: { kind: 'number', region, reference: percent, whole: false },
    });
    const most = Array.from({ length: LIMITS.maxExpectedCharacters }, (_, index) =>
      String.fromCodePoint(0x100 + index),
    );
    expect(buildSpec({ ...number, characters: most }, 'example').ok).toBe(true);
    const over = [...most, '%'];
    expect(buildSpec({ ...number, characters: over }, 'example')).toEqual({ ok: false, next: TOO_MANY });
    // A saved value keeps a longer list through an edit or a fix that leaves it as it is.
    expect(buildSpec({ ...number, characters: over }, 'example', over).ok).toBe(true);
    expect(buildSpec({ ...number, characters: over }, 'example', most)).toEqual({ ok: false, next: TOO_MANY });
    expect(savedNeeds({ kind: 'number', reference: 'percent', whole: false, characters: '%1 1' })).toEqual(['1', '%']);
    expect(savedNeeds({ kind: 'number', reference: 'percent', whole: false })).toEqual([]);
    expect(savedNeeds({ kind: 'color', color: red, tolerance: 4 })).toBeNull();
    // What needs a reference still comes first.
    expect(buildSpec({ ...number, reference: null, characters: over }, 'example').ok).toBe(false);
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
      spec: { kind: 'color', region, color: red, tolerance: CHECKS.color.tolerance },
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
        tolerance: CHECKS.state.tolerance,
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
      next: 'Fix each state name that has a message under it.',
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
        tolerance: CHECKS.state.tolerance,
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
    expect(buildCharacters(draft, null, [])).toEqual({
      ok: true,
      spec: {
        // The reference reads where its first box is.
        region,
        screenshot: 2,
        reference: null,
        labels: [
          { screenshot: 2, region, text: '87%' },
          { screenshot: 0, region: other, text: '12%' },
        ],
        recorded: [],
        dropped: [],
      },
    });
  });

  test('teaches kept recorded readings alone, in reading order', () => {
    const draft = {
      ...EMPTY_DRAFT,
      ink: red,
      boxes: [{ id: 0, screenshot: 0, region, text: '' }],
      recorded: new Map([
        [4, '9%'],
        [1, ' 8% '],
        [2, ''],
      ]),
    };
    const built = buildCharacters(draft, null, []);
    expect(built.ok && built.spec.recorded).toEqual([
      { reading: 1, text: '8%' },
      { reading: 4, text: '9%' },
    ]);
  });

  test('asks for what a new reference needs', () => {
    expect(buildCharacters(EMPTY_DRAFT, null, [])).toEqual({
      ok: false,
      next: 'Draw a box around the characters on a screenshot.',
    });
    expect(buildCharacters({ ...EMPTY_DRAFT, region }, null, [])).toEqual({
      ok: false,
      next: 'Draw a box around the characters on a screenshot.',
    });
    expect(
      buildCharacters(
        { ...EMPTY_DRAFT, region, drawnOn: 0, boxes: [{ id: 0, screenshot: 0, region, text: ' ' }] },
        null,
        [],
      ),
    ).toEqual({
      ok: false,
      next: 'Type what a box shows, or record the characters.',
    });
  });

  test('a saved reference needs nothing new to be tried', () => {
    const built = buildCharacters({ ...EMPTY_DRAFT, boxes: [{ id: 0, screenshot: 0, region, text: '' }] }, percent, []);
    expect(built.ok && built.spec.reference).toEqual(percent);
  });
});

test('reviewSpec waits for a box and names the reference', () => {
  const boxes = [{ id: 0, screenshot: 1, region, text: '' }];
  expect(reviewSpec({ ...EMPTY_DRAFT, boxes }, [], null)).toEqual({
    region,
    screenshot: 1,
    reference: null,
    labels: [],
    recorded: [],
    dropped: [],
  });
  expect(reviewSpec({ ...EMPTY_DRAFT, region, drawnOn: 1 }, [], null)).toBeNull();
  const kept = [{ reading: 0, text: '5' }];
  // The saved pictures the editor left out go with every ask about the characters.
  const dropped = ['ref-percent-2.png'];
  expect(reviewSpec({ ...EMPTY_DRAFT, boxes, dropped }, kept, percent)).toEqual({
    region,
    screenshot: 1,
    reference: percent,
    labels: [],
    recorded: kept,
    dropped,
  });
});

test('withKind keeps the region and colors and resets the tolerance', () => {
  const draft = withKind({ ...EMPTY_DRAFT, region, color: red, tolerance: 99 }, 'color');
  expect(draft.kind).toBe('color');
  expect(draft.region).toEqual(region);
  expect(draft.color).toEqual(red);
  expect(draft.tolerance).toBe(CHECKS.color.tolerance);
});

test('a saved picture the reference editor leaves out is left out once', () => {
  const left = withoutSample(EMPTY_DRAFT, 'ref-percent-1.png');
  expect(left.dropped).toEqual(['ref-percent-1.png']);
  expect(withoutSample(left, 'ref-percent-1.png')).toBe(left);
});

describe('a stopped recording', () => {
  const settings = {
    region,
    windowSize: { width: 1920, height: 1080 },
    scope: 'place' as const,
  };
  const recorded = { ...EMPTY_DRAFT, boxes: [{ id: 0, screenshot: 0, region, text: '87%' }] };
  /** `recorded` with its reading box at `at`. */
  const readingAt = (at: typeof region) => ({
    ...recorded,
    boxes: [{ id: 0, screenshot: 0, region: at, text: '87%' }],
  });
  const big = { width: 1920, height: 1080 };
  const small = { width: 1280, height: 720 };

  test('records the same while the draft has its settings at its window size', () => {
    expect(settingsDiffer(readingAt({ ...region }), settings, { ...big })).toBe(false);
  });

  test('offers its settings back for any other region or window size', () => {
    expect(settingsDiffer(readingAt({ ...region, x: 2 }), settings, big)).toBe(true);
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

  // A reference reads at its first box.
  const first = { id: 0, screenshot: 0, region, text: '87%' };
  const other = { id: 1, screenshot: 0, region: { ...region, y: 40 }, text: '12' };
  const alone = { ...EMPTY_DRAFT, boxes: [first, other] };
  const sizes = [
    [0, big],
    [1, small],
    [2, big],
  ] as const;

  /** `draft` after Use settings, with `boxId` the box the recording read at. */
  function usedSettings(draft: Draft, boxId: number | null): Draft {
    const on = draft.boxes.find((box) => box.id === boxId)?.screenshot ?? null;
    const place = recordedPlace(settings, on, sizes);
    const { boxes } =
      place === null
        ? draft
        : boxStep(
            { boxes: draft.boxes, selectedId: null, recordedId: boxId },
            { type: 'restore', screenshot: place, region: settings.region },
          );
    return { ...draft, boxes };
  }

  test('using its settings moves the reading box back', () => {
    // The reading box was moved, then deleted.
    const moved = { ...alone, boxes: [{ ...first, region: { ...region, x: 9 } }, other] };
    const deleted = { ...alone, boxes: [other] };
    for (const changed of [moved, deleted]) {
      expect(settingsDiffer(changed, settings, big)).toBe(true);
      const back = usedSettings(changed, 0);
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
    expect(recordedPlace(settings, 0, sizes)).toBe(0);
    expect(recordedPlace(settings, 1, sizes)).toBe(2);
    expect(recordedPlace(settings, null, sizes)).toBe(2);
    expect(recordedPlace(settings, 1, [[1, small]])).toBeNull();
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

test('a fix places its region on the first screenshot whose pixels arrived', () => {
  const shots = [{ index: 0 }, { index: 2 }, { index: 5 }];
  // The first screenshot's pixels never came, so the second one is the first that can carry the region.
  expect(
    firstWithPixels(
      shots,
      new Map([
        [2, 'two'],
        [5, 'five'],
      ]),
    ),
  ).toEqual({ index: 2, picture: 'two' });
  expect(
    firstWithPixels(
      shots,
      new Map([
        [0, 'zero'],
        [2, 'two'],
      ]),
    ),
  ).toEqual({ index: 0, picture: 'zero' });
  // Pixels of a screenshot the strip does not list do not count.
  expect(firstWithPixels(shots, new Map([[1, 'one']]))).toBeNull();
});

describe('state names', () => {
  // Each name breaks one rule of the saved file's, and the message says which.
  const problem = (name: string): string | undefined => stateNameProblems([name]).get(0);
  const longest = 'é'.repeat(LIMITS.maxStateNameChars);

  test('a name the saved file takes has no message, once trimmed', () => {
    for (const name of ['Paused', 'a', 'Game over', 'Nonever', ' none of these ', longest, `${longest} `]) {
      expect(problem(name)).toBeUndefined();
    }
  });

  test('a name with no characters but spaces has none', () => {
    expect(problem('')).toBe('Name this state.');
    expect(problem('  ')).toBe('Name this state.');
  });

  test('a name holds no control character', () => {
    expect(problem('Pau\tsed')).toBe('A state name cannot hold a control character, such as a tab.');
    // Trimming keeps a control character that is not a space, which the saved file refuses too.
    expect(problem('Paused\u0085')).toBe('A state name cannot hold a control character, such as a tab.');
  });

  test('a name is at most the longest the saved file takes, in characters', () => {
    expect(problem(`${longest}a`)).toBe(`A state name can be at most ${LIMITS.maxStateNameChars} characters long.`);
  });

  test('a name is not the one that stands for no state, in any capitals', () => {
    const reserved = `The value reads "${NO_STATE}" when no state shows, so a state cannot have that name.`;
    expect(problem(NO_STATE)).toBe(reserved);
    expect(problem(' NONE ')).toBe(reserved);
    expect(problem('NoNe')).toBe(reserved);
  });

  test('names that differ only in case or in spaces at their ends are one name', () => {
    const shared = 'Another state has this name, or one that differs from it only in capitals.';
    expect([...stateNameProblems(['Ready', 'Paused', ' ready '])]).toEqual([
      [0, shared],
      [2, shared],
    ]);
    // A name's own problem comes first, so two empty names each say to name them.
    expect([...stateNameProblems(['', ' ', 'Ready'])]).toEqual([
      [0, 'Name this state.'],
      [1, 'Name this state.'],
    ]);
  });

  test('a state check waits until every name can be saved', () => {
    const ready: DraftState = { name: 'Ready', screenshot: 0, steps: [] };
    const draft: Draft = { ...EMPTY_DRAFT, kind: 'state', region, drawnOn: 0, states: [ready] };
    const second = (name: string): Draft => ({ ...draft, states: [ready, { name, screenshot: 1, steps: [] }] });
    const next = 'Fix each state name that has a message under it.';
    expect(stateNamesNext(['Ready', 'Paused'])).toBeNull();
    for (const name of ['ready', NO_STATE, 'a\tb', `${longest}a`]) {
      expect(stateNamesNext(['Ready', name])).toBe(next);
      expect(buildSpec(second(name), 'example')).toEqual({ ok: false, next });
    }
    expect(buildSpec(second('Paused'), 'example').ok).toBe(true);
  });
});

test('recording waits for a box to read at', () => {
  expect(recordBlocked(EMPTY_DRAFT)).toBe('Draw a box around the characters on a screenshot to record them.');
  // A value's region is no place for a reference to read.
  expect(recordBlocked({ ...EMPTY_DRAFT, region, drawnOn: 0 })).toBe(
    'Draw a box around the characters on a screenshot to record them.',
  );
  expect(recordBlocked({ ...EMPTY_DRAFT, boxes: [{ id: 0, screenshot: 1, region, text: '' }] })).toBeNull();
});

test('finding more like it waits for a selected box', () => {
  expect(findBlocked({ id: 0, screenshot: 0, region, text: '' })).toBeNull();
  expect(findBlocked(null)).toBe('Select a box around one sample first.');
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
  expect(onPicture(drawn, 1, true, 'draw')).toEqual({ ...drawn, drawnOn: 1 });
  expect(onPicture(drawn, 1, false, 'draw')).toEqual({ ...drawn, region: null, drawnOn: null });
  expect(onPicture(EMPTY_DRAFT, 1, true, 'draw')).toBe(EMPTY_DRAFT);
  // A saved value's region waits, on no screenshot, for one of the size it was drawn at.
  const saved = { ...EMPTY_DRAFT, region };
  expect(onPicture(saved, 1, false, 'draw')).toBe(saved);
  // While a size is unknown, such as before the screenshot's pixels arrive, the region stays where it is drawn.
  expect(onPicture(drawn, 1, null, 'draw')).toBe(drawn);
  // A saved value can only move its region at its original size, even after it sat on a screenshot.
  expect(onPicture(drawn, 1, false, 'edit')).toBe(drawn);
  expect(onPicture(drawn, 1, true, 'edit')).toEqual({ ...drawn, drawnOn: 1 });
});

describe('fixDraft', () => {
  test("starts from the saved check's kind and settings, with no region", () => {
    expect(fixDraft({ kind: 'number', reference: 'percent', whole: true })).toEqual({
      ...EMPTY_DRAFT,
      reference: 'percent',
      whole: true,
    });
    expect(fixDraft({ kind: 'number', reference: 'percent', whole: false, characters: '%10 1' })).toEqual({
      ...EMPTY_DRAFT,
      reference: 'percent',
      characters: ['0', '1', '%'],
    });
    expect(fixDraft({ kind: 'barFill', filled: red, empty: gray })).toEqual({
      ...EMPTY_DRAFT,
      kind: 'barFill',
      tolerance: CHECKS.barFill.tolerance,
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

describe('buildReferenceSave', () => {
  test('saves a saved reference alone while it has no place to read', () => {
    // The region of a value whose window is not captured sits on no screenshot.
    const dropped = ['ref-percent-1.png'];
    const windowless = { ...EMPTY_DRAFT, region, dropped };
    expect(buildReferenceSave(windowless, percent, [])).toEqual({
      ok: true,
      spec: { kind: 'settings', reference: percent, dropped },
    });
  });

  test('teaches the characters once there is a place to read them', () => {
    const draft = { ...EMPTY_DRAFT, boxes: [{ id: 0, screenshot: 1, region, text: '5' }] };
    const built = buildReferenceSave(draft, percent, []);
    expect(built.ok && built.spec.kind).toBe('taught');
  });

  test('a new reference still says what it needs', () => {
    expect(buildReferenceSave({ ...EMPTY_DRAFT }, null, []).ok).toBe(false);
  });

  test('sends the characters the reference reads either way, and none for every character', () => {
    const windowless = { ...EMPTY_DRAFT, region };
    expect(buildReferenceSave(windowless, percent, ['1', '%'])).toEqual({
      ok: true,
      spec: { kind: 'settings', reference: percent, dropped: [], characters: '1%' },
    });
    const draft = { ...EMPTY_DRAFT, boxes: [{ id: 0, screenshot: 1, region, text: '5' }] };
    const taught = buildReferenceSave(draft, percent, ['5']);
    expect(taught.ok && taught.spec.kind === 'taught' && taught.spec.characters.characters).toBe('5');
    const every = buildReferenceSave(draft, percent, []);
    expect(every.ok && every.spec.kind === 'taught' && 'characters' in every.spec.characters).toBe(false);
  });
});
