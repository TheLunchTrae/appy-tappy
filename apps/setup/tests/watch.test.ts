import { describe, expect, test } from 'bun:test';

import type { AppSummary, CheckDef, ValueDef } from '../src/lib/bindings.ts';
import { EMPTY_DRAFT, valueChoices } from '../src/lib/automation.ts';
import { pickerApps } from '../src/lib/picker.ts';
import { watchGuide, watchStep, watching } from '../src/lib/watch.ts';

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

const example = app('example', 'Example App', [progress, status]);
const player = app('player', 'Player', [icon]);
const fresh = app('fresh', 'Fresh', []);
const broken: AppSummary = { id: 'broken', file: null, error: 'the file is not valid', windows: [], running: null };

function step(apps: readonly AppSummary[], draft = EMPTY_DRAFT, chosen: string | null = null) {
  return watchStep(pickerApps(apps), valueChoices(apps), draft, chosen);
}

describe('choosing what to watch', () => {
  test('starts with adding an application while none is saved', () => {
    expect(step([])).toEqual({ kind: 'no-applications' });
    expect(step([broken])).toEqual({ kind: 'no-applications' });
  });

  test('asks which application while several are saved and none is chosen', () => {
    expect(step([example, player])).toEqual({ kind: 'choosing' });
  });

  test('takes the only saved application without asking', () => {
    expect(step([fresh]).kind).toBe('value');
    expect(step([example, broken]).kind).toBe('pick');
  });

  test('needs a value while the chosen application has none', () => {
    const at = step([example, fresh], EMPTY_DRAFT, 'fresh');
    expect(at.kind).toBe('value');
    expect('app' in at ? at.app.name : null).toBe('Fresh');
  });

  test("offers only the chosen application's values", () => {
    const at = step([example, player], EMPTY_DRAFT, 'player');
    expect(at.kind).toBe('pick');
    expect('values' in at ? at.values.map((choice) => choice.value.id) : null).toEqual(['icon']);
  });

  test('is ready once a saved value of the application is picked', () => {
    const draft = { ...EMPTY_DRAFT, source: { app: 'example', value: 'status' } };
    const at = step([example, player], draft);
    expect(at.kind).toBe('ready');
    expect('choice' in at ? at.choice.value.name : null).toBe('Status');
  });

  test("follows the watched value's application rather than the one chosen before it", () => {
    const draft = { ...EMPTY_DRAFT, source: { app: 'example', value: 'progress' } };
    const at = step([example, player], draft, 'player');
    expect('app' in at ? at.app.id : null).toBe('example');
  });

  test('asks for a value again when the watched one is not saved any more', () => {
    const gone = { ...EMPTY_DRAFT, source: { app: 'example', value: 'gone' } };
    expect(step([example, player], gone).kind).toBe('pick');
    expect(step([fresh, player], { ...EMPTY_DRAFT, source: { app: 'fresh', value: 'gone' } }).kind).toBe('value');
  });

  test('says so when the watched application cannot be read', () => {
    const draft = { ...EMPTY_DRAFT, source: { app: 'broken', value: 'progress' } };
    expect(step([example, broken], draft)).toEqual({ kind: 'missing', id: 'broken' });
  });
});

describe('the guide to the next step', () => {
  const guide = (apps: readonly AppSummary[], chosen: string | null = null, draft = EMPTY_DRAFT) =>
    watchGuide(step(apps, draft, chosen));

  test('leads to the flow that makes what is missing', () => {
    expect(guide([])?.opens).toBe('application');
    expect(guide([])?.text).toStartWith('First, choose what to watch.');
    expect(guide([fresh])?.opens).toBe('value');
    expect(guide([fresh])?.text).toStartWith('Next, make a value for Fresh:');
  });

  test('asks for a choice in the editor when what is saved is enough', () => {
    expect(guide([example, player])).toEqual({
      text: 'First, choose the application to watch, or add it if it is not in the list.',
      opens: null,
      warn: false,
    });
    expect(guide([example, player], 'player')).toEqual({
      text: 'Pick the value of Player to watch, or make a new one.',
      opens: null,
      warn: false,
    });
  });

  test('warns about an application that cannot be read, and only then', () => {
    const draft = { ...EMPTY_DRAFT, source: { app: 'broken', value: 'progress' } };
    expect(guide([example, broken], null, draft)?.warn).toBe(true);
    expect(guide([])?.warn).toBe(false);
    expect(guide([fresh])?.warn).toBe(false);
  });

  test('says nothing once there is a value to compare', () => {
    expect(guide([example], null, { ...EMPTY_DRAFT, source: { app: 'example', value: 'progress' } })).toBeNull();
  });
});

describe('the draft once an application is chosen', () => {
  const choices = valueChoices([example, player]);
  const typed = { ...EMPTY_DRAFT, name: 'Pause music', number: '90' };

  test('picks the value a flow just saved, with a comparison that suits it', () => {
    expect(watching(typed, choices, 'player', 'icon')).toEqual({
      ...typed,
      source: { app: 'player', value: 'icon' },
      is: 'equals',
    });
    expect(watching(typed, choices, 'example', 'status')).toEqual({
      ...typed,
      source: { app: 'example', value: 'status' },
      is: 'equals',
      state: 'Paused',
    });
  });

  test('drops a value of another application', () => {
    const watched = { ...typed, source: { app: 'example', value: 'progress' } };
    expect(watching(watched, choices, 'player', null)).toEqual({ ...typed, source: null });
  });

  test("keeps the application's own value", () => {
    const watched = { ...typed, source: { app: 'example', value: 'progress' } };
    expect(watching(watched, choices, 'example', null)).toBe(watched);
  });

  test('keeps the draft when the value is not among the saved ones', () => {
    const watched = { ...typed, source: { app: 'example', value: 'progress' } };
    expect(watching(watched, choices, 'example', 'unsaved')).toBe(watched);
    expect(watching(watched, choices, 'player', 'unsaved')).toEqual({ ...typed, source: null });
  });

  test('is unchanged with no application', () => {
    const watched = { ...typed, source: { app: 'example', value: 'progress' } };
    expect(watching(watched, choices, null, null)).toBe(watched);
    expect(watching(typed, choices, null, 'icon')).toBe(typed);
  });
});
