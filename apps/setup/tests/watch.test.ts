import { describe, expect, test } from 'bun:test';

import type { AppSummary, CheckDef, ValueDef } from '../src/lib/bindings.ts';
import { EMPTY_DRAFT, valueChoices } from '../src/lib/automation.ts';
import { arrived, watchApps, watchGuide, watchStep, watching } from '../src/lib/watch.ts';

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

function step(
  apps: readonly AppSummary[],
  draft = EMPTY_DRAFT,
  chosen: string | null = null,
  made: string | null = null,
) {
  return watchStep({ saved: watchApps(apps), choices: valueChoices(apps), draft, chosen, made });
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

  test('asks for a value again when the watched one is not saved any more', () => {
    const gone = { ...EMPTY_DRAFT, source: { app: 'example', value: 'gone' } };
    expect(step([example, player], gone).kind).toBe('pick');
    expect(step([fresh, player], { ...EMPTY_DRAFT, source: { app: 'fresh', value: 'gone' } }).kind).toBe('value');
  });

  test("says so when the watched value's application cannot be read", () => {
    const draft = { ...EMPTY_DRAFT, source: { app: 'broken', value: 'progress' } };
    expect(step([example, broken], draft)).toEqual({ kind: 'missing', id: 'broken' });
    // Also once the editor showed again with that application chosen.
    expect(step([example, broken], draft, 'broken')).toEqual({ kind: 'missing', id: 'broken' });
  });
});

describe('what a flow just saved', () => {
  const choices = valueChoices([example, player]);

  test('is awaited while the saved lists lack the application', () => {
    expect(step([example], EMPTY_DRAFT, 'player')).toEqual({ kind: 'arriving', id: 'player' });
    expect(step([], EMPTY_DRAFT, 'player')).toEqual({ kind: 'arriving', id: 'player' });
  });

  test('is awaited until its value is picked, whatever the draft watched before', () => {
    const watched = { ...EMPTY_DRAFT, source: { app: 'example', value: 'progress' } };
    expect(step([example], EMPTY_DRAFT, 'example', 'hull')).toEqual({ kind: 'arriving', id: 'example' });
    expect(step([example], watched, 'example', 'hull')).toEqual({ kind: 'arriving', id: 'example' });
    expect(step([example], watched, 'example', 'status')).toEqual({ kind: 'arriving', id: 'example' });
    expect(step([example], watched, 'example').kind).toBe('ready');
  });

  test('is picked once the saved values list it', () => {
    expect(arrived({ choices, chosen: 'player', made: 'icon' })?.value.name).toBe('Icon');
    expect(arrived({ choices, chosen: 'example', made: 'status' })?.states).toEqual(['Paused', 'Live']);
  });

  test('is not picked before the lists have it, nor from another application', () => {
    expect(arrived({ choices, chosen: 'player', made: 'hull' })).toBeNull();
    expect(arrived({ choices, chosen: 'player', made: 'progress' })).toBeNull();
    expect(arrived({ choices: [], chosen: 'player', made: 'icon' })).toBeNull();
  });

  test('is nothing when no flow saved a value', () => {
    expect(arrived({ choices, chosen: 'player', made: null })).toBeNull();
    expect(arrived({ choices, chosen: null, made: 'icon' })).toBeNull();
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

  test('says what it waits for, and leads nowhere meanwhile', () => {
    expect(guide([example], 'player')).toEqual({ text: 'Reading what you saved…', opens: null, warn: false });
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
  const typed = { ...EMPTY_DRAFT, name: 'Pause music', number: '90' };
  const watched = { ...typed, source: { app: 'example', value: 'progress' } };

  test('drops a value of another application', () => {
    expect(watching(watched, 'player')).toEqual({ ...typed, source: null });
  });

  test("keeps the application's own value", () => {
    expect(watching(watched, 'example')).toBe(watched);
  });

  test('is unchanged with no application, or with no value yet', () => {
    expect(watching(watched, null)).toBe(watched);
    expect(watching(typed, 'player')).toBe(typed);
  });
});
