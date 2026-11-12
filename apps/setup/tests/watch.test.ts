import { describe, expect, test } from 'bun:test';

import type { AppSummary, CheckDef, ValueDef } from '../src/lib/bindings.ts';
import { EMPTY_DRAFT, buildAutomation, valueChoices, withSource } from '../src/lib/automation.ts';
import type { Handed } from '../src/lib/flow.ts';
import {
  type WatchStep,
  type Watching,
  arrived,
  readiness,
  returned,
  watchApps,
  watchGuide,
  watchStep,
  watching,
} from '../src/lib/watch.ts';

function value(id: string, name: string, check: CheckDef): ValueDef {
  return {
    id,
    name,
    screen: {
      windowSize: { width: 100, height: 50 },
      region: { x: 0, y: 0, width: 10, height: 10 },
      check,
      fixes: [],
    },
  };
}

function app(id: string, name: string, values: readonly ValueDef[]): AppSummary {
  return {
    id,
    file: {
      format: 1,
      name,
      window: {
        title: name,
        class: 'Window',
        executable: `${name}.exe`,
        mustMatch: { title: false, class: false },
        fallback: 'none',
      },
      sizeChanges: 'often',
      antiCheat: 'none',
      values,
      references: [],
    },
    error: null,
    windows: [],
    running: null,
  };
}

const white = { red: 255, green: 255, blue: 255 };
const progress = value('progress', 'Progress', { kind: 'number', reference: 'percent', whole: false });
const status = value('status', 'Status', {
  kind: 'state',
  states: [
    { name: 'Paused', image: 'status-1.png' },
    { name: 'Live', image: 'status-2.png' },
  ],
  tolerance: 16,
});
const icon = value('icon', 'Icon', { kind: 'color', color: white, tolerance: 10 });

const hull = value('hull', 'Hull', { kind: 'barFill', filled: white, empty: { red: 0, green: 0, blue: 0 } });

const example = app('example', 'Example App', [progress, status]);
const withHull = app('example', 'Example App', [progress, status, hull]);
const player = app('player', 'Player', [icon]);
const fresh = app('fresh', 'Fresh', []);
const broken: AppSummary = { id: 'broken', file: null, error: 'the file is not valid', windows: [], running: null };

/** What the editor knows with `apps` as read number 5 answered them, and what `more` says besides. */
function knows(apps: readonly AppSummary[], more: Partial<Watching> = {}): Watching {
  return {
    apps,
    choices: valueChoices(apps),
    listed: 5,
    readError: null,
    draft: EMPTY_DRAFT,
    chosen: null,
    handed: null,
    ...more,
  };
}

function step(apps: readonly AppSummary[], more: Partial<Watching> = {}) {
  return watchStep(knows(apps, more));
}

/** What a flow handed over after read 5 started, which no read has answered since. */
function handedOver(value: string | null): Handed {
  return { value, after: 5 };
}
/** What a flow handed over before the read that `knows` has answered started. */
function handedBefore(value: string | null): Handed {
  return { value, after: 4 };
}

const watchesProgress = { ...EMPTY_DRAFT, name: 'Quiet', number: '90', source: { app: 'example', value: 'progress' } };

test('only the saved applications whose files read can be watched', () => {
  expect(watchApps([example, broken, fresh])).toEqual([
    { id: 'example', name: 'Example App' },
    { id: 'fresh', name: 'Fresh' },
  ]);
});

describe('choosing what to watch', () => {
  test('starts with adding an application while none is saved', () => {
    expect(step([])).toEqual({ kind: 'no-applications', unreadable: false });
  });

  test('says so when applications are saved but none of them reads', () => {
    expect(step([broken])).toEqual({ kind: 'no-applications', unreadable: true });
  });

  test('asks which application while several are saved and none is chosen', () => {
    expect(step([example, player])).toEqual({ kind: 'choosing' });
  });

  test('takes the only saved application without asking', () => {
    expect(step([fresh]).kind).toBe('value');
    expect(step([example, broken]).kind).toBe('pick');
  });

  test('needs a value while the chosen application has none', () => {
    const at = step([example, fresh], { chosen: 'fresh' });
    expect(at.kind).toBe('value');
    expect('app' in at ? at.app.name : null).toBe('Fresh');
    expect('lost' in at && at.lost).toBe(false);
  });

  test("offers only the chosen application's values", () => {
    const at = step([example, player], { chosen: 'player' });
    expect(at.kind).toBe('pick');
    expect('values' in at ? at.values.map((choice) => choice.value.id) : null).toEqual(['icon']);
  });

  test('is ready once a saved value of the application is picked', () => {
    const at = step([example, player], { draft: { ...EMPTY_DRAFT, source: { app: 'example', value: 'status' } } });
    expect(at.kind).toBe('ready');
    expect('choice' in at ? at.choice.value.name : null).toBe('Status');
  });

  test('asks for a value again when the watched one is not saved any more', () => {
    const gone = { ...EMPTY_DRAFT, source: { app: 'example', value: 'gone' } };
    expect(step([example, player], { draft: gone })).toMatchObject({ kind: 'pick', lost: false });
    const none = { ...EMPTY_DRAFT, source: { app: 'fresh', value: 'gone' } };
    expect(step([fresh, player], { draft: none })).toMatchObject({ kind: 'value', lost: false });
  });

  test("says so when the watched value's application cannot be read", () => {
    const draft = { ...EMPTY_DRAFT, source: { app: 'broken', value: 'progress' } };
    expect(step([example, broken], { draft })).toEqual({ kind: 'missing', id: 'broken', added: false });
    // Also once the editor showed again with that application chosen, and for one chosen by hand that broke since.
    expect(step([example, broken], { draft, chosen: 'broken' })).toEqual({
      kind: 'missing',
      id: 'broken',
      added: false,
    });
    expect(step([example, broken], { chosen: 'broken' })).toEqual({ kind: 'missing', id: 'broken', added: false });
  });
});

describe('what a flow just saved', () => {
  test('is awaited until a read of the saved lists that started after it answers', () => {
    const arriving: WatchStep = { kind: 'arriving', id: 'player', error: null };
    expect(step([example], { chosen: 'player', handed: handedOver(null) })).toEqual(arriving);
    expect(step([], { chosen: 'player', handed: handedOver(null) })).toEqual(arriving);
    // A read numbered like the last one started before the save is not newer than it.
    expect(step([example], { chosen: 'player', handed: handedOver(null), listed: 5 })).toEqual(arriving);
    expect(step([example], { chosen: 'example', handed: handedOver('hull') })).toEqual({ ...arriving, id: 'example' });
  });

  test('is awaited with the reason while the saved lists cannot be read', () => {
    const at = step([example], { chosen: 'example', handed: handedOver('hull'), readError: 'access is denied' });
    expect(at).toEqual({ kind: 'arriving', id: 'example', error: 'access is denied' });
  });

  test('is missing once a newer read answers without the application', () => {
    expect(step([example], { chosen: 'player', handed: handedBefore(null) })).toEqual({
      kind: 'missing',
      id: 'player',
      added: true,
    });
    expect(step([example], { chosen: 'player', handed: handedOver(null), listed: 6 })).toEqual({
      kind: 'missing',
      id: 'player',
      added: true,
    });
  });

  test('is lost once a newer read answers without the value, and the other values stay to pick', () => {
    expect(step([example], { chosen: 'example', handed: handedBefore('hull') })).toMatchObject({
      kind: 'pick',
      lost: true,
    });
    expect(step([fresh], { chosen: 'fresh', handed: handedBefore('hull') })).toMatchObject({
      kind: 'value',
      lost: true,
    });
    // A value that is listed is there to pick, not lost.
    expect(step([withHull], { chosen: 'example', handed: handedBefore('hull') })).toMatchObject({
      kind: 'pick',
      lost: false,
    });
  });

  test('an application that is listed is the next step whatever read listed it', () => {
    expect(step([fresh], { chosen: 'fresh', handed: handedOver(null) })).toMatchObject({ kind: 'value', lost: false });
    expect(step([example], { chosen: 'example', handed: handedBefore(null) })).toMatchObject({
      kind: 'pick',
      lost: false,
    });
  });

  test('a value is picked once the saved values list it', () => {
    const choices = valueChoices([withHull, player]);
    expect(arrived({ choices, chosen: 'example', handed: handedOver('hull') })?.value.name).toBe('Hull');
    expect(arrived({ choices, chosen: 'player', handed: handedBefore('icon') })?.kind).toBe('flag');
  });

  test('a value is not picked before the lists have it, nor from another application', () => {
    const choices = valueChoices([example, player]);
    expect(arrived({ choices, chosen: 'example', handed: handedOver('hull') })).toBeNull();
    expect(arrived({ choices, chosen: 'player', handed: handedOver('progress') })).toBeNull();
    expect(arrived({ choices: [], chosen: 'player', handed: handedOver('icon') })).toBeNull();
  });

  test('nothing is picked when no flow saved a value', () => {
    const choices = valueChoices([example, player]);
    expect(arrived({ choices, chosen: 'player', handed: null })).toBeNull();
    expect(arrived({ choices, chosen: 'player', handed: handedOver(null) })).toBeNull();
    expect(arrived({ choices, chosen: null, handed: handedOver('icon') })).toBeNull();
  });
});

describe('saving while a value a flow saved is awaited', () => {
  // The automation watched Progress, and a value flow just saved Hull for the same application.
  const back = returned(watchesProgress, 'example', handedOver('hull'));

  test('has no value to save, so the old one cannot be saved in its place', () => {
    expect(back.source).toBeNull();
    const built = buildAutomation(back, valueChoices([example]), '');
    expect(built).toEqual({ ok: false, next: 'Pick the value to watch.' });
    const at = step([example], { draft: back, chosen: 'example', handed: handedOver('hull') });
    expect(readiness(built, at)).toEqual({ text: 'Reading what you saved…', ready: false });
  });

  test('saves nothing while it waits, whatever the draft would build', () => {
    const built = buildAutomation(watchesProgress, valueChoices([example]), '');
    expect(built.ok).toBe(true);
    const at = step([example], { draft: watchesProgress, chosen: 'example', handed: handedOver('hull') });
    expect(readiness(built, at)).toEqual({ text: 'Reading what you saved…', ready: false });
  });

  test('builds with the new value once the saved values list it', () => {
    const choices = valueChoices([withHull]);
    const landed = arrived({ choices, chosen: 'example', handed: handedOver('hull') });
    if (landed === null) {
      throw new Error('the value is listed');
    }
    const picked = withSource(back, landed);
    const built = buildAutomation(picked, choices, '');
    expect(built.ok && built.automation.when).toEqual({ app: 'example', value: 'hull', is: 'atLeast', target: 90 });
    const at = step([withHull], { draft: picked, chosen: 'example' });
    expect(readiness(built, at)).toEqual({ text: 'Ready to save.', ready: true });
  });

  test('says what the automation still needs otherwise', () => {
    const built = buildAutomation(EMPTY_DRAFT, valueChoices([example]), '');
    expect(readiness(built, step([example]))).toEqual({ text: 'Name the automation.', ready: false });
  });
});

describe('the draft when the editor shows again', () => {
  test('keeps its value when only an application of its own came back, or nothing did', () => {
    expect(returned(watchesProgress, 'example', null)).toBe(watchesProgress);
    expect(returned(watchesProgress, 'example', handedOver(null))).toBe(watchesProgress);
    expect(returned(watchesProgress, null, null)).toBe(watchesProgress);
  });

  test('drops a value of another application than the one that came back', () => {
    expect(returned(watchesProgress, 'player', handedOver(null))).toEqual({ ...watchesProgress, source: null });
    expect(returned(watchesProgress, 'player', null)).toEqual({ ...watchesProgress, source: null });
  });
});

describe('the guide to the next step', () => {
  const guide = (apps: readonly AppSummary[], more: Partial<Watching> = {}, picked: string | null = null) =>
    watchGuide(step(apps, more), picked);

  test('leads to the flow that makes what is missing', () => {
    expect(guide([])?.opens).toBe('application');
    expect(guide([])?.text).toStartWith('First, choose what to watch.');
    expect(guide([fresh])?.opens).toBe('value');
    expect(guide([fresh])?.text).toStartWith('Next, make a value for Fresh:');
  });

  test('says that the saved applications cannot be read rather than that none is saved', () => {
    expect(guide([broken])).toEqual({
      text: 'None of the saved applications can be read, so there is nothing to watch yet. Check the Applications page, or add the application again; you come back here with it chosen.',
      opens: 'application',
      warn: true,
    });
    expect(guide([])?.text).toContain('no application is saved yet');
  });

  test('asks for a choice in the editor when what is saved is enough', () => {
    expect(guide([example, player])).toEqual({
      text: 'First, choose the application to watch, or add it if it is not in the list.',
      opens: null,
      warn: false,
    });
    expect(guide([example, player], { chosen: 'player' })).toEqual({
      text: 'Pick the value of Player to watch, or make a new one.',
      opens: null,
      warn: false,
    });
  });

  test('says what it waits for, and why when the lists cannot be read', () => {
    const waits = { chosen: 'player', handed: handedOver(null) };
    expect(guide([example], waits)).toEqual({ text: 'Reading what you saved…', opens: null, warn: false });
    expect(guide([example], { ...waits, readError: 'access is denied' })).toEqual({
      text: "Couldn't read what you saved: access is denied. AppyTappy tries again every few seconds.",
      opens: null,
      warn: true,
    });
  });

  test('says what is gone, and a way forward that keeps the automation', () => {
    expect(guide([example], { chosen: 'player', handed: handedBefore(null) })).toEqual({
      text: "The application you saved isn't among the saved applications: it was removed, or its file can't be read. Choose another, or add it again.",
      opens: null,
      warn: true,
    });
    expect(guide([example], { chosen: 'example', handed: handedBefore('hull') })).toEqual({
      text: "The value you saved isn't among Example App's values: it was deleted, or Example App's file changed. Pick another, or make it again.",
      opens: null,
      warn: true,
    });
    expect(guide([fresh], { chosen: 'fresh', handed: handedBefore('hull') })).toEqual({
      text: "The value you saved isn't among Fresh's values: it was deleted, or Fresh's file changed. Make it again.",
      opens: 'value',
      warn: true,
    });
  });

  test('warns about a watched application that cannot be read', () => {
    const draft = { ...EMPTY_DRAFT, source: { app: 'broken', value: 'progress' } };
    expect(guide([example, broken], { draft })).toEqual({
      text: "The application this automation watches can't be read any more. Choose another, add it again, or check the Applications page.",
      opens: null,
      warn: true,
    });
    expect(guide([])?.warn).toBe(false);
    expect(guide([fresh])?.warn).toBe(false);
  });

  test('names the value a flow just saved once it is picked, and is silent for any other', () => {
    const ready = { draft: watchesProgress };
    expect(guide([example], ready, 'progress')).toEqual({
      text: 'Progress is picked. Say how it compares.',
      opens: null,
      warn: false,
    });
    expect(guide([example], ready, 'status')).toBeNull();
    expect(guide([example], ready)).toBeNull();
  });
});

describe('the draft once an application is chosen', () => {
  test('drops a value of another application', () => {
    expect(watching(watchesProgress, 'player')).toEqual({ ...watchesProgress, source: null });
  });

  test("keeps the application's own value", () => {
    expect(watching(watchesProgress, 'example')).toBe(watchesProgress);
  });

  test('is unchanged with no application, or with no value yet', () => {
    expect(watching(watchesProgress, null)).toBe(watchesProgress);
    expect(watching(EMPTY_DRAFT, 'player')).toBe(EMPTY_DRAFT);
  });
});
