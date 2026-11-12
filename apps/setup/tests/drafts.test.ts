import { describe, expect, test } from 'bun:test';

import type { AppSummary, DraftDef, DraftKind, DraftSummary } from '../src/lib/bindings.ts';
import { EMPTY_DRAFT as EMPTY_AUTOMATION, newAction, unfinished, valueChoices } from '../src/lib/automation.ts';
import {
  type CaptureSetup,
  captureMissing,
  keptSetup,
  labeledCharacters,
  restoreSetup,
  storeSetup,
} from '../src/lib/captureDraft.ts';
import { type Draft, EMPTY_DRAFT } from '../src/lib/check.ts';
import {
  DRAFT_FORMAT,
  draftOfSaved,
  draftProblem,
  draftsOf,
  isKept,
  keep,
  newDrafts,
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
  recordedShots: [],
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

  test('a capture keeps its labels and drops the recording readings', () => {
    const setup: CaptureSetup = {
      ...EMPTY_SETUP,
      valueName: 'Score',
      step: 'how',
      draft: { ...EMPTY_DRAFT, labels: new Map([[0, '12']]), recorded: new Map([[3, '45']]) },
    };
    // The draft goes through JSON, as the file keeps it.
    const kept: unknown = JSON.parse(JSON.stringify(keep('value', storeSetup(setup))));
    const back = keptSetup(def('value', kept));
    expect(back?.valueName).toBe('Score');
    expect(back?.step).toBe('how');
    expect([...(back?.draft.labels ?? [])]).toEqual([[0, '12']]);
    expect(back?.draft.recorded.size).toBe(0);
    expect(keptSetup(def('reference', kept))).toBeNull();
    expect(restoreSetup(storeSetup(setup)).draft.labels.get(0)).toBe('12');
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
      labels: new Map([
        [0, '12%'],
        [2, '7'],
      ]),
      recorded: new Map([[3, '45']]),
      found: [{ screenshot: 2, region: { x: 1, y: 2, width: 3, height: 4 }, text: '99' }],
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
        valueRegion: { region, drawnOn: 0 },
      },
      draft,
      recordedShots: [2],
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
    expect(DRAFT_FORMAT).toBe(1);
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
    expect(Object.keys(stored).sort()).toEqual(['draft', 'editing', 'mayMove', 'recordedShots', 'step', 'valueName']);
    expect(Object.keys(stored.draft).sort()).toEqual([
      'color',
      'colorMatch',
      'drawnOn',
      'empty',
      'filled',
      'found',
      'ignoreBackground',
      'ink',
      'kind',
      'labels',
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
    expect(unfinishedApp({ name: ' ', window: null, sizeChanges: 'often' })).toEqual([
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
      editing: { existing: null, name: '', expects: [], valueRegion: { region: null, drawnOn: null } },
    };
    expect(captureMissing(editing, next, false)).toEqual(['Name the reference.']);
  });

  test('a reference draft lists each labeled character once', () => {
    const setup = {
      ...EMPTY_SETUP,
      draft: {
        ...EMPTY_DRAFT,
        labels: new Map([
          [0, '12'],
          [1, '2 3'],
          [2, ''],
        ]),
      },
    };
    expect(labeledCharacters(setup)).toBe('1 2 3');
  });
});
