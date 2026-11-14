import { describe, expect, test } from 'bun:test';

import type { AntiCheatClass, AppSummary, AutomationDef, CheckDef, ValueDef } from '../src/lib/bindings.ts';
import { newAction } from '../src/lib/actions.ts';
import {
  type Draft,
  EMPTY_DRAFT,
  buildAutomation,
  buildWhen,
  findValue,
  readsState,
  readsText,
  targetText,
  toDraft,
  usedBy,
  valueChoices,
  withSource,
} from '../src/lib/automation.ts';

const white = { red: 255, green: 255, blue: 255 };

function value(id: string, name: string, check: CheckDef): ValueDef {
  return {
    id,
    name,
    origin: { kind: 'byHand' },
    screen: {
      windowSize: { width: 100, height: 50 },
      region: { x: 0, y: 0, width: 10, height: 10 },
      check,
      fixes: [],
    },
  };
}

const progress = value('progress', 'Progress', { kind: 'number', reference: 'percent', whole: false });
const iconVisible = value('icon_visible', 'Icon visible', { kind: 'color', color: white, tolerance: 10 });
const status = value('status', 'Status', {
  kind: 'state',
  states: [
    { name: 'Paused', image: 'status-1.png' },
    { name: 'Live', image: 'status-2.png' },
  ],
  tolerance: 16,
});
const loaded = value('loaded', 'Loaded', {
  kind: 'barFill',
  filled: white,
  empty: { red: 0, green: 0, blue: 0 },
});

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
      values: [progress, iconVisible, status, loaded],
      references: [],
    },
    error: null,
    windows: [7],
    running: null,
  },
  { id: 'broken', file: null, error: 'the file is not valid', windows: [], running: null },
];

const choices = valueChoices(apps);

const pauseMusic: AutomationDef = {
  id: 'pause_music',
  name: 'Pause music',
  enabled: true,
  when: { app: 'example', value: 'progress', is: 'atLeast', target: 90 },
  actions: [
    { kind: 'media', command: 'pause', app: 'Player.exe' },
    { kind: 'volume', percent: 20, app: null },
  ],
  undo: true,
  cooldownMs: 1500,
};

describe('values', () => {
  test('a value says what it reads', () => {
    const [number, flag, state, bar] = choices;
    if (number === undefined || flag === undefined || state === undefined || bar === undefined) {
      throw new Error('values exist');
    }
    // How full a bar is reads as a number, like text does.
    expect(readsText(bar)).toBe('Loaded reads a number.');
    expect(readsText(number)).toBe('Progress reads a number.');
    expect(readsText(flag)).toBe('Icon visible reads yes or no.');
    expect(readsText(state)).toBe('Status reads a state: Paused, Live, or none.');
    expect(readsText({ ...state, states: ['Paused'] })).toBe('Status reads a state: Paused or none.');
  });

  test('a state check reads its own states and none', () => {
    const state = findValue(choices, { app: 'example', value: 'status' });
    const flag = findValue(choices, { app: 'example', value: 'icon_visible' });
    if (state === null || flag === null) {
      throw new Error('values exist');
    }
    expect(state.states).toEqual(['Paused', 'Live']);
    expect(readsState(state, 'Live')).toBe(true);
    expect(readsState(state, 'none')).toBe(true);
    expect(readsState(state, 'live')).toBe(false);
    expect(readsState(flag, 'none')).toBe(false);
  });

  test('only readable applications offer their values', () => {
    expect(choices.map((choice) => [choice.app, choice.appName, choice.value.id, choice.kind])).toEqual([
      ['example', 'Example App', 'progress', 'number'],
      ['example', 'Example App', 'icon_visible', 'flag'],
      ['example', 'Example App', 'status', 'state'],
      ['example', 'Example App', 'loaded', 'number'],
    ]);
    expect(findValue(choices, { app: 'example', value: 'icon_visible' })?.value).toBe(iconVisible);
    expect(findValue(choices, { app: 'other', value: 'progress' })).toBeNull();
  });

  test('a value lists the automations that use it', () => {
    const summaries = [
      { automation: pauseMusic, problem: null },
      { automation: { ...pauseMusic, id: 'b', name: 'Lower volume' }, problem: null },
      {
        automation: { ...pauseMusic, id: 'c', name: 'Other', when: { ...pauseMusic.when, value: 'icon_visible' } },
        problem: null,
      },
    ];
    expect(usedBy(summaries, 'example', 'progress')).toEqual(['Pause music', 'Lower volume']);
    expect(usedBy(summaries, 'other', 'progress')).toEqual([]);
  });
});

describe('words', () => {
  test('a target reads as the condition says it', () => {
    expect(targetText(90)).toBe('90');
    expect(targetText(true)).toBe('yes');
    expect(targetText(false)).toBe('no');
    expect(targetText('none')).toBe('none');
  });
});

describe('buildAutomation', () => {
  test('a saved automation round-trips through the editor', () => {
    expect(buildAutomation(toDraft(pauseMusic), choices, 'pause_music', apps)).toEqual({
      ok: true,
      automation: pauseMusic,
    });
    const flag: AutomationDef = {
      ...pauseMusic,
      enabled: false,
      when: { app: 'example', value: 'icon_visible', is: 'notEquals', target: false },
      undo: false,
      cooldownMs: 0,
    };
    expect(buildAutomation(toDraft(flag), choices, 'x', apps)).toEqual({ ok: true, automation: { ...flag, id: 'x' } });
  });

  test('says what to fill in next', () => {
    const next = (draft: Draft): string => {
      const built = buildAutomation(draft, choices, '', apps);
      return built.ok ? 'ok' : built.next;
    };
    expect(next(EMPTY_DRAFT)).toBe('Name the automation.');
    const named = { ...EMPTY_DRAFT, name: '  Pause music ' };
    expect(next(named)).toBe('Pick the value to watch.');
    expect(next({ ...named, source: { app: 'example', value: 'gone' } })).toBe('Pick the value to watch.');
    const watching = { ...named, source: { app: 'example', value: 'progress' } };
    for (const number of ['', 'ninety', '0x5A', '1e2', '12,5', '1,23', '1234,567', ',500', '1,234,5', 'Infinity']) {
      expect(next({ ...watching, number })).toBe('Enter the number Progress is compared with, such as 90 or 12.5.');
    }
    expect(next({ ...watching, number: '-.5' })).toBe('ok');
    expect(next({ ...watching, number: '-1,234.5' })).toBe('ok');
    const complete = { ...watching, number: ' 90 ' };
    expect(next(complete)).toBe('ok');
    expect(next({ ...complete, actions: [] })).toBe('Add an action.');
    expect(next({ ...complete, actions: [newAction(1), { ...newAction(2), target: 'app', app: '  ' }] })).toBe(
      'Name the application for action 2, such as Player.exe.',
    );
    expect(next({ ...complete, actions: [{ ...newAction(1), command: 'next' }] })).toBe(
      'Turn off undo: a skipped track cannot be unskipped.',
    );
    expect(next({ ...complete, undo: false, actions: [{ ...newAction(1), command: 'next' }] })).toBe('ok');
    for (const cooldown of ['', '-1', '86401', 'soon']) {
      expect(next({ ...complete, cooldown })).toBe('Set the wait before it runs again in seconds, from 0 to 86400.');
    }
    expect(next({ ...complete, cooldown: '86400' })).toBe('ok');
  });

  test('builds what the draft says, trimming and rounding as typed', () => {
    const draft = {
      ...EMPTY_DRAFT,
      name: '  Lower volume ',
      source: { app: 'example', value: 'progress' },
      is: 'below' as const,
      number: '12.5',
      actions: [{ ...newAction(4, 'volume'), percent: ' 30 ', target: 'app' as const, app: ' Player.exe ' }],
      cooldown: '1.2345',
    };
    expect(buildAutomation(draft, choices, '', apps)).toEqual({
      ok: true,
      automation: {
        id: '',
        name: 'Lower volume',
        enabled: true,
        when: { app: 'example', value: 'progress', is: 'below', target: 12.5 },
        actions: [{ kind: 'volume', percent: 30, app: 'Player.exe' }],
        undo: true,
        cooldownMs: 1235,
      },
    });
  });

  test('a yes-or-no value compares with yes or no, keeping only is and is not', () => {
    const icon = findValue(choices, { app: 'example', value: 'icon_visible' });
    const number = findValue(choices, { app: 'example', value: 'progress' });
    if (icon === null || number === null) {
      throw new Error('values exist');
    }
    const picked = withSource({ ...EMPTY_DRAFT, name: 'x', is: 'above' }, icon);
    expect(picked.is).toBe('equals');
    expect(picked.source).toEqual({ app: 'example', value: 'icon_visible' });
    expect(withSource({ ...picked, is: 'notEquals' }, icon).is).toBe('notEquals');
    expect(withSource({ ...picked, is: 'notEquals' }, number).is).toBe('notEquals');
    expect(withSource({ ...picked, is: 'atMost' }, number).is).toBe('atMost');
    const built = buildAutomation({ ...picked, flag: false }, choices, '', apps);
    expect(built.ok && built.automation.when).toEqual({
      app: 'example',
      value: 'icon_visible',
      is: 'equals',
      target: false,
    });
    expect(buildAutomation({ ...picked, is: 'atLeast' }, choices, '', apps)).toEqual({
      ok: false,
      next: 'Pick how Icon visible is compared.',
    });
  });

  test('a loaded automation gives each action a row with its own key', () => {
    expect(toDraft(pauseMusic).actions.map((action) => [action.key, action.kind])).toEqual([
      [0, 'media'],
      [1, 'volume'],
    ]);
  });
});

describe('input', () => {
  const streaming: AppSummary = {
    id: 'streaming',
    file: {
      format: 1,
      name: 'Streaming app',
      window: {
        title: 'Streaming',
        class: '',
        executable: 'Streaming.exe',
        mustMatch: { title: false, class: false },
        fallback: 'none',
      },
      antiCheat: 'none',
      values: [],
      references: [],
      sizeChanges: 'often',
    },
    error: null,
    windows: [],
    running: null,
  };
  const withAntiCheat = (watched: AntiCheatClass): readonly AppSummary[] => [
    ...apps.map((app) => (app.file === null ? app : { ...app, file: { ...app.file, antiCheat: watched } })),
    streaming,
  ];
  const clicks: AutomationDef = {
    ...pauseMusic,
    actions: [
      { kind: 'keys', keys: 'Ctrl+Shift+M' },
      {
        kind: 'click',
        app: 'streaming',
        button: 'left',
        delivery: 'message',
        returnFocus: true,
        at: {
          kind: 'picture',
          image: 'click-1.png',
          windowSize: { width: 1280, height: 720 },
          search: { x: 0, y: 300, width: 188, height: 120 },
          press: { x: 62, y: 10 },
          tolerance: 20,
        },
      },
    ],
    undo: false,
  };
  const build = (draft: Draft, summaries: readonly AppSummary[]): ReturnType<typeof buildAutomation> =>
    buildAutomation(draft, valueChoices(summaries), 'pause_music', summaries);

  test('round-trips through the editor when neither application has anti-cheat', () => {
    expect(build(toDraft(clicks), withAntiCheat('none'))).toEqual({ ok: true, automation: clicks });
  });

  test("waits for the watched application's anti-cheat to be set to none, saying the first reason", () => {
    expect(build(toDraft(clicks), withAntiCheat('userMode'))).toEqual({
      ok: false,
      next: "Anti-cheat can treat pressed keys as cheating, so keys are pressed only once Example App's anti-cheat is set to None on the Applications page.",
    });
  });

  test('cannot be undone, which is said before anti-cheat', () => {
    expect(build({ ...toDraft(clicks), undo: true }, withAntiCheat('userMode'))).toEqual({
      ok: false,
      next: 'Turn off undo: pressed keys cannot be unpressed, and a click cannot be undone.',
    });
  });

  test('a click needs a saved application to click in', () => {
    const draft = toDraft(clicks);
    const gone = { ...draft, actions: draft.actions.map((action) => ({ ...action, clickApp: 'gone' })) };
    expect(build(gone, withAntiCheat('none'))).toEqual({
      ok: false,
      next: 'The application action 2 clicks in is not saved. Pick another.',
    });
  });
});

describe('buildWhen', () => {
  test('builds the condition the live reading tests, or says what to fill in', () => {
    const number = findValue(choices, { app: 'example', value: 'progress' });
    const icon = findValue(choices, { app: 'example', value: 'icon_visible' });
    if (number === null || icon === null) {
      throw new Error('values exist');
    }
    const draft = { ...EMPTY_DRAFT, source: { app: 'example', value: 'progress' } };
    expect(buildWhen({ ...draft, number: ' 90 ' }, number)).toEqual({
      app: 'example',
      value: 'progress',
      is: 'atLeast',
      target: 90,
    });
    expect(buildWhen({ ...draft, number: '-1,234.5' }, number)).toEqual({
      app: 'example',
      value: 'progress',
      is: 'atLeast',
      target: -1234.5,
    });
    expect(buildWhen(draft, number)).toBe('Enter the number Progress is compared with, such as 90 or 12.5.');
    expect(buildWhen({ ...draft, flag: false, is: 'notEquals' }, icon)).toEqual({
      app: 'example',
      value: 'icon_visible',
      is: 'notEquals',
      target: false,
    });
    expect(buildWhen(draft, icon)).toBe('Pick how Icon visible is compared.');
  });

  test('a state value compares with one of its states, or none', () => {
    const state = findValue(choices, { app: 'example', value: 'status' });
    if (state === null) {
      throw new Error('values exist');
    }
    const draft = withSource({ ...EMPTY_DRAFT, is: 'atLeast' }, state);
    expect(draft.is).toBe('equals');
    expect(draft.state).toBe('Paused');
    expect(buildWhen(draft, state)).toEqual({ app: 'example', value: 'status', is: 'equals', target: 'Paused' });
    expect(buildWhen({ ...draft, state: 'none', is: 'notEquals' }, state)).toEqual({
      app: 'example',
      value: 'status',
      is: 'notEquals',
      target: 'none',
    });
    expect(buildWhen({ ...draft, state: 'Gone' }, state)).toBe('Pick the state Status is compared with.');
    expect(buildWhen({ ...draft, is: 'above' }, state)).toBe('Pick how Status is compared.');
    expect(withSource({ ...draft, state: 'Live' }, state).state).toBe('Live');
    expect(withSource({ ...draft, state: 'Gone' }, state).state).toBe('Paused');
  });
});

describe('typed numbers', () => {
  test('a number too large to hold is not a target', () => {
    const progress = findValue(choices, { app: 'example', value: 'progress' });
    if (progress === null) {
      throw new Error('the value exists');
    }
    const draft = { ...EMPTY_DRAFT, source: { app: 'example', value: 'progress' }, number: '9'.repeat(400) };
    expect(buildWhen(draft, progress)).toBe('Enter the number Progress is compared with, such as 90 or 12.5.');
  });
});
