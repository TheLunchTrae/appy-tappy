import { describe, expect, test } from 'bun:test';

import type { AppSummary, DraftDef, DraftKind, DraftSummary } from '../src/lib/bindings.ts';
import { newAction } from '../src/lib/actions.ts';
import { EMPTY_DRAFT as EMPTY_AUTOMATION, unfinished, valueChoices } from '../src/lib/automation.ts';
import {
  type CaptureSetup,
  captureMissing,
  captureProblem,
  fixOutdated,
  refreshedEditing,
  keptSetup,
  labeledCharacters,
  restoreSetup,
  shownOnStep,
  storeSetup,
} from '../src/lib/captureDraft.ts';
import { type Draft, EMPTY_DRAFT, fixDraft } from '../src/lib/check.ts';
import {
  DRAFT_FORMAT,
  draftOfSaved,
  draftProblem,
  draftsOf,
  draftsWith,
  isKept,
  keep,
  newDrafts,
  removalLoses,
  unfinishedText,
} from '../src/lib/drafts.ts';
import { unfinishedApp } from '../src/lib/matching.ts';

const apps: readonly AppSummary[] = [
  {
    id: 'example',
    file: {
      format: 1,
      name: 'Example App',
      window: {
        title: 'Example App',
        class: 'ExampleWindow',
        executable: 'Example.exe',
        mustMatch: { title: false, class: false },
        fallback: 'none',
      },
      sizeChanges: 'often',
      antiCheat: 'unknown',
      values: [
        {
          id: 'progress',
          name: 'Progress',
          origin: { kind: 'byHand' } as const,
          screen: {
            windowSize: { width: 100, height: 50 },
            region: { x: 0, y: 0, width: 10, height: 10 },
            check: { kind: 'number', reference: 'percent', whole: false },
            fixes: [],
          },
        },
      ],
      references: [],
    },
    error: null,
    windows: [],
    running: null,
  },
];

/** What a new value's capture has set up before the user does anything. */
const EMPTY_SETUP: CaptureSetup = {
  valueName: '',
  step: 'where',
  mayMove: false,
  editing: null,
  draft: EMPTY_DRAFT,
};

function def(kind: DraftKind, state: unknown, changes: string | null = null, app: string | null = null): DraftDef {
  return { kind, app, changes, name: 'Draft', missing: [], pictures: [], state };
}

function summary(id: string, draft: DraftDef | null): DraftSummary {
  return { id, draft, problem: draft === null ? 'Its file is not valid: bad.' : null, screenshots: 0 };
}

describe('what a draft keeps', () => {
  test('only the same format and kind is read back', () => {
    const kept = keep('application', { name: 'Example' });
    expect(kept.format).toBe(DRAFT_FORMAT);
    expect(isKept(kept, 'application')).toBe(true);
    expect(isKept(kept, 'automation')).toBe(false);
    expect(isKept({ ...kept, format: DRAFT_FORMAT + 1 }, 'application')).toBe(false);
    expect(isKept({ format: DRAFT_FORMAT, kind: 'application' }, 'application')).toBe(false);
    expect(isKept(null, 'application')).toBe(false);
  });

  test('a capture keeps its boxes and drops the recording readings', () => {
    const region = { x: 1, y: 2, width: 3, height: 4 };
    const setup: CaptureSetup = {
      ...EMPTY_SETUP,
      valueName: 'Score',
      step: 'how',
      draft: { ...EMPTY_DRAFT, boxes: [{ id: 0, screenshot: 0, region, text: '12' }], recorded: new Map([[3, '45']]) },
    };
    // The draft goes through JSON, as the file keeps it.
    const kept: unknown = JSON.parse(JSON.stringify(keep('value', storeSetup(setup))));
    const back = keptSetup(def('value', kept));
    expect(back?.valueName).toBe('Score');
    expect(back?.step).toBe('how');
    expect(back?.draft.boxes).toEqual([{ id: 0, screenshot: 0, region, text: '12' }]);
    expect(back?.draft.recorded.size).toBe(0);
    expect(keptSetup(def('reference', kept))).toBeNull();
    expect(restoreSetup(storeSetup(setup)).draft.boxes).toEqual(setup.draft.boxes);
  });

  test('a capture comes back with everything it set up, but for what belongs to a recording', () => {
    const black = { red: 0, green: 0, blue: 0 };
    const region = { x: 3, y: 4, width: 20, height: 10 };
    // Every field of the check's draft, each away from what a new value starts with.
    const draft: Draft = {
      kind: 'state',
      region,
      drawnOn: 2,
      pin: { kind: 'byHand', pin: 'bottomRight' },
      tolerance: 7,
      ink: black,
      boxes: [
        { id: 0, screenshot: 0, region, text: '12%' },
        { id: 3, screenshot: 2, region: { x: 1, y: 2, width: 3, height: 4 }, text: '99' },
      ],
      recorded: new Map([[3, '45']]),
      filled: { red: 1, green: 2, blue: 3 },
      empty: { red: 4, green: 5, blue: 6 },
      color: { red: 7, green: 8, blue: 9 },
      states: [
        {
          name: 'Paused',
          screenshot: 0,
          steps: [
            { kind: 'snap', region, combine: 'add' },
            { kind: 'click', x: 5, y: 6, combine: 'remove' },
          ],
        },
      ],
      stateNames: ['Playing'],
      colorMatch: 31,
      ignoreBackground: false,
      reference: 'percent',
      whole: true,
      characters: ['1', '%'],
    };
    const drawn: Readonly<Record<string, unknown>> = { ...draft };
    for (const [key, start] of Object.entries(EMPTY_DRAFT)) {
      expect([key, drawn[key]]).not.toEqual([key, start]);
    }
    const setup: CaptureSetup = {
      valueName: 'Score',
      step: 'how',
      mayMove: true,
      editing: {
        existing: null,
        name: 'Digits',
        expects: ['1', '2', '%'],
      },
      draft,
    };
    const made: Readonly<Record<string, unknown>> = { ...setup };
    for (const [key, start] of Object.entries(EMPTY_SETUP)) {
      expect([key, made[key]]).not.toEqual([key, start]);
    }
    const kept: unknown = JSON.parse(JSON.stringify(keep('value', storeSetup(setup))));
    expect(keptSetup(def('value', kept))).toEqual({ ...setup, draft: { ...draft, recorded: new Map() } });
  });
});

describe('the shape a draft keeps', () => {
  // A draft keeps each flow's state as it is. When one of these lists changes, raise DRAFT_FORMAT, so drafts kept in
  // the older shape say they were saved by another version instead of opening with fields missing.
  test('is the one this format names', () => {
    expect(DRAFT_FORMAT).toBe(4);
    expect(Object.keys(EMPTY_AUTOMATION).sort()).toEqual([
      'actions',
      'cooldown',
      'enabled',
      'flag',
      'is',
      'name',
      'number',
      'source',
      'state',
      'undo',
    ]);
    expect(Object.keys(newAction(0, 'click')).sort()).toEqual([
      'app',
      'at',
      'button',
      'clickApp',
      'command',
      'delivery',
      'key',
      'keys',
      'kind',
      'opens',
      'percent',
      'returnFocus',
      'sound',
      'target',
    ]);
    const stored = storeSetup(EMPTY_SETUP);
    expect(Object.keys(stored).sort()).toEqual(['draft', 'editing', 'mayMove', 'step', 'valueName']);
    expect(Object.keys(stored.draft).sort()).toEqual([
      'boxes',
      'characters',
      'color',
      'colorMatch',
      'drawnOn',
      'empty',
      'filled',
      'ignoreBackground',
      'ink',
      'kind',
      'pin',
      'reference',
      'region',
      'stateNames',
      'states',
      'tolerance',
      'whole',
    ]);
  });
});

describe('why a draft cannot be finished', () => {
  test('another version, a missing application, or a missing fixed value', () => {
    expect(draftProblem(def('automation', { format: 0 }), apps)).toBe(
      'It was saved by another version of AppyTappy Setup, so it cannot be finished here.',
    );
    expect(draftProblem(def('automation', keep('automation', {})), apps)).toBeNull();
    expect(draftProblem(def('value', keep('value', {}), null, 'gone'), apps)).toBe(
      'Its application is no longer saved.',
    );
    expect(draftProblem(def('value', keep('value', {}), null, 'example'), apps)).toBeNull();
    const unreadable: readonly AppSummary[] = [{ ...apps[0]!, file: null, error: 'Its file is not valid.' }];
    expect(draftProblem(def('value', keep('value', {}), null, 'example'), unreadable)).toBe(
      "Its application's file cannot be read.",
    );
    expect(draftProblem(def('fix', keep('fix', {}), 'gone', 'example'), apps)).toBe(
      'The value it fixes is no longer saved.',
    );
    expect(draftProblem(def('fix', keep('fix', {}), 'progress', 'example'), apps)).toBeNull();
  });

  test('what is left reads as sentences', () => {
    expect(unfinishedText([])).toBe('Ready to finish.');
    expect(unfinishedText(['Name the value.', 'Add an action.'])).toBe('Not finished. Name the value. Add an action.');
  });
});

describe('which list a draft shows in', () => {
  const drafts = [
    summary('a', def('automation', keep('automation', {}))),
    summary('b', def('automation', keep('automation', {}), 'saved')),
    summary('c', def('automation', keep('automation', {}), 'deleted')),
    summary('d', def('reference', keep('reference', {}), 'saved', 'example')),
    summary('e', null),
  ];

  test('a draft of changes belongs to its saved item, in its application', () => {
    expect(draftOfSaved(drafts, 'automation', 'saved')?.id).toBe('b');
    expect(draftOfSaved(drafts, 'automation', 'other')).toBeNull();
    expect(draftOfSaved(drafts, 'reference', 'saved', 'example')?.id).toBe('d');
    expect(draftOfSaved(drafts, 'reference', 'saved', 'elsewhere')).toBeNull();
  });

  test('new drafts, and drafts of items no longer saved, get rows of their own', () => {
    expect(newDrafts(drafts, 'automation', (id) => id === 'saved').map((each) => each.id)).toEqual(['a', 'c']);
    expect(draftsOf(drafts, 'automation')).toHaveLength(3);
    expect(draftsOf(null, 'automation')).toEqual([]);
  });
});

describe('what an unfinished item still needs', () => {
  test('an automation needs a name, a value, and an action first', () => {
    const choices = valueChoices(apps);
    expect(unfinished({ ...EMPTY_AUTOMATION, actions: [] }, choices, apps)).toEqual([
      'Name the automation.',
      'Pick the value to watch.',
      'Add an action.',
    ]);
    const named = { ...EMPTY_AUTOMATION, name: 'Pause', source: { app: 'example', value: 'gone' }, actions: [] };
    expect(unfinished(named, choices, apps)).toEqual(['Pick the value to watch.', 'Add an action.']);
    const ready = {
      ...named,
      source: { app: 'example', value: 'progress' },
      number: '90',
      actions: [newAction(0)],
    };
    expect(unfinished(ready, choices, apps)).toEqual([]);
  });

  test('an application needs a window and a name', () => {
    expect(unfinishedApp({ name: ' ', window: null, sizeChanges: 'often', antiCheat: null })).toEqual([
      "Pick the application's window.",
      'Name the application.',
    ]);
  });

  test('a capture needs a name and a region, then what the check asks for', () => {
    const setup = EMPTY_SETUP;
    const next = { ok: false, next: 'Pick the reference.' } as const;
    expect(captureMissing(setup, next, false)).toEqual(['Name the value.', 'Draw the region where it shows.']);
    expect(captureMissing(setup, next, true)).toEqual(['Draw the region where it shows.']);
    const drawn = {
      ...setup,
      valueName: 'Score',
      draft: { ...EMPTY_DRAFT, region: { x: 0, y: 0, width: 4, height: 4 } },
    };
    expect(captureMissing(drawn, next, false)).toEqual(['Pick the reference.']);
    const editing = {
      ...drawn,
      editing: { existing: null, name: '', expects: [] },
    };
    expect(captureMissing(editing, next, false)).toEqual(['Name the reference.']);
  });

  test('a reference draft lists each labeled character once', () => {
    const setup = {
      ...EMPTY_SETUP,
      draft: {
        ...EMPTY_DRAFT,
        boxes: [
          { id: 0, screenshot: 0, region: { x: 0, y: 0, width: 4, height: 2 }, text: '12' },
          { id: 1, screenshot: 0, region: { x: 9, y: 0, width: 4, height: 2 }, text: '2 3' },
          { id: 2, screenshot: 1, region: { x: 0, y: 0, width: 4, height: 2 }, text: '' },
        ],
      },
    };
    expect(labeledCharacters(setup)).toBe('1 2 3');
  });
});

describe('a draft and what it belongs to', () => {
  const progress = apps[0]!.file!.values[0]!;
  const fixOf = (draft: Draft): DraftDef =>
    def('fix', JSON.parse(JSON.stringify(keep('fix', storeSetup({ ...EMPTY_SETUP, draft })))), 'progress', 'example');

  test('a fix whose value was changed since no longer fits it, and can only be deleted', () => {
    const fits = fixOf(fixDraft(progress.screen.check));
    expect(fixOutdated(fits, progress)).toBe(false);
    expect(captureProblem(fits, apps)).toBeNull();
    // The reference a fix reads with is its own choice, so another one still fits.
    expect(fixOutdated(fixOf({ ...fixDraft(progress.screen.check), reference: 'other' }), progress)).toBe(false);
    // What a number counts and which characters it reads are the value's, which a fix follows.
    const whole = { ...progress.screen.check, whole: true };
    expect(fixOutdated(fits, { ...progress, screen: { ...progress.screen, check: whole } })).toBe(true);
    const digits = { ...progress.screen.check, characters: '0123456789' };
    expect(fixOutdated(fits, { ...progress, screen: { ...progress.screen, check: digits } })).toBe(true);
    expect(fixOutdated(fixOf(fixDraft(digits)), { ...progress, screen: { ...progress.screen, check: digits } })).toBe(
      false,
    );
    const bar = {
      kind: 'barFill' as const,
      filled: { red: 1, green: 2, blue: 3 },
      empty: { red: 4, green: 5, blue: 6 },
    };
    const asBar = fixOf(fixDraft(bar));
    expect(fixOutdated(asBar, progress)).toBe(true);
    expect(captureProblem(asBar, apps)).toBe('The value it fixes was changed since, so this fix no longer fits it.');
    // The same kind with another color is another way to read it too.
    const recolored = {
      ...progress,
      screen: { ...progress.screen, check: { ...bar, filled: { red: 9, green: 9, blue: 9 } } },
    };
    expect(fixOutdated(asBar, recolored)).toBe(true);
    expect(fixOutdated(asBar, { ...progress, screen: { ...progress.screen, check: bar } })).toBe(false);
    // A missing value comes first, and a value draft has nothing to be outdated against.
    expect(captureProblem({ ...asBar, changes: 'gone' }, apps)).toBe('The value it fixes is no longer saved.');
    expect(fixOutdated({ ...asBar, kind: 'value' }, progress)).toBe(false);
  });

  test('a reference editor a draft kept open goes on from the reference as it is saved now', () => {
    const saved = {
      id: 'percent',
      name: 'Percent',
      origin: { kind: 'byHand' } as const,
      ink: { red: 0, green: 0, blue: 0 },
      tolerance: 0,
      samples: [],
    };
    const taught = { ...saved, tolerance: 12 };
    const editing = { existing: saved, name: 'Percent', expects: ['1'] };
    expect(refreshedEditing(editing, [taught])).toEqual({ ...editing, existing: taught });
    // A reference deleted since is made as a new one, and a new one stays new.
    expect(refreshedEditing(editing, [])).toEqual({ ...editing, existing: null });
    const fresh = { ...editing, existing: null };
    expect(refreshedEditing(fresh, [taught])).toBe(fresh);
    expect(refreshedEditing(null, [taught])).toBeNull();
  });

  test('the newest draft of a saved item is the one finished', () => {
    const both = [
      summary('a', def('application', keep('application', {}), 'example')),
      summary('b', def('application', keep('application', {}), 'example')),
    ];
    expect(draftOfSaved(both, 'application', 'example')?.id).toBe('b');
  });

  test('removing an application says what goes with it', () => {
    const drafts = [
      summary('a', def('value', keep('value', {}), null, 'example')),
      summary('b', def('fix', keep('fix', {}), 'progress', 'example')),
      summary('c', def('application', keep('application', {}), 'example')),
      summary('d', def('application', keep('application', {}), 'other')),
      summary('e', def('automation', keep('automation', {}), 'example')),
      summary('f', null),
    ];
    expect(draftsWith(drafts, 'example')).toBe(3);
    expect(draftsWith(null, 'example')).toBe(0);
    expect(removalLoses(0, 0)).toBe('This cannot be undone.');
    expect(removalLoses(1, 0)).toBe('Its value goes with it, and this cannot be undone.');
    expect(removalLoses(0, 1)).toBe('Its draft goes with it, and this cannot be undone.');
    expect(removalLoses(2, 0)).toBe('Its 2 values go with it, and this cannot be undone.');
    expect(removalLoses(1, 1)).toBe('Its value and draft go with it, and this cannot be undone.');
    expect(removalLoses(2, 3)).toBe('Its 2 values and 3 drafts go with it, and this cannot be undone.');
  });
});

test("entering Where to read shows the screenshot the value's region is drawn on, which is the one it lists", () => {
  // Screenshot 5 was selected in a later step; the region is drawn on 2.
  expect(shownOnStep('where', 2, 5)).toBe(2);
  expect(shownOnStep('where', 2, null)).toBe(2);
  // With no region drawn, and in the other steps, what shows stays.
  expect(shownOnStep('where', null, 5)).toBe(5);
  expect(shownOnStep('how', 2, 5)).toBe(5);
  expect(shownOnStep('check', 2, null)).toBeNull();
});
