import { describe, expect, test } from 'bun:test';

import type { AppSummary, AutomationDef, CheckDef, ValueDef } from '../src/api.ts';
import {
  EMPTY_DRAFT,
  actionText,
  buildAutomation,
  findValue,
  newAction,
  targetText,
  toDraft,
  undoable,
  undoText,
  usedBy,
  valueChoices,
  valueKind,
  withSource,
} from '../src/automation.ts';

const white = { red: 255, green: 255, blue: 255 };

function value(id: string, name: string, check: CheckDef): ValueDef {
  return {
    id,
    name,
    screen: {
      windowSize: { width: 100, height: 50 },
      region: { x: 0, y: 0, width: 10, height: 10 },
      check,
    },
  };
}

const progress = value('progress', 'Progress', {
  kind: 'number',
  ink: white,
  tolerance: 10,
  samples: [{ image: 'progress-1.png', text: '12%' }],
});
const iconVisible = value('icon_visible', 'Icon visible', { kind: 'color', color: white, tolerance: 10 });

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
        priority: 'titleElseExecutable',
      },
      antiCheat: 'unknown',
      values: [progress, iconVisible],
    },
    error: null,
    windows: [7],
  },
  { id: 'broken', file: null, error: 'the file is not valid', windows: [] },
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
  test('number and bar fill checks read numbers; color and picture checks read yes or no', () => {
    expect(valueKind({ kind: 'number', ink: white, tolerance: 0, samples: [] })).toBe('number');
    expect(valueKind({ kind: 'barFill', filled: white, empty: white })).toBe('number');
    expect(valueKind({ kind: 'color', color: white, tolerance: 0 })).toBe('flag');
    expect(valueKind({ kind: 'reference', image: 'a.png', tolerance: 0 })).toBe('flag');
  });

  test('only readable applications offer their values', () => {
    expect(choices.map((choice) => [choice.app, choice.appName, choice.value.id, choice.kind])).toEqual([
      ['example', 'Example App', 'progress', 'number'],
      ['example', 'Example App', 'icon_visible', 'flag'],
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
  test('targets and actions read plainly', () => {
    expect(targetText(90)).toBe('90');
    expect(targetText(true)).toBe('yes');
    expect(targetText(false)).toBe('no');
    expect(actionText({ kind: 'media', command: 'pause', app: 'Player.exe' })).toBe('Pause media · Player.exe');
    expect(actionText({ kind: 'media', command: 'next', app: null })).toBe('Next track · whatever is playing');
    expect(actionText({ kind: 'volume', percent: 20, app: null })).toBe('Volume · system 20%');
    expect(actionText({ kind: 'volume', percent: 0, app: 'Player.exe' })).toBe('Volume · Player.exe 0%');
  });

  test('undo says what it puts back, and skips cannot be undone', () => {
    expect(undoable({ kind: 'media', command: 'pause', app: null })).toBe(true);
    expect(undoable({ kind: 'media', command: 'play', app: null })).toBe(true);
    expect(undoable({ kind: 'media', command: 'next', app: null })).toBe(false);
    expect(undoable({ kind: 'media', command: 'previous', app: null })).toBe(false);
    expect(undoable({ kind: 'volume', percent: 5, app: null })).toBe(true);
    expect(undoText(pauseMusic.actions)).toBe('Paused media plays again and each volume goes back to where it was.');
    expect(undoText([{ kind: 'media', command: 'play', app: null }])).toBe('Started media pauses again.');
    expect(
      undoText([
        { kind: 'media', command: 'pause', app: null },
        { kind: 'media', command: 'play', app: null },
        { kind: 'volume', percent: 5, app: null },
      ]),
    ).toBe('Paused media plays again, started media pauses again and each volume goes back to where it was.');
    expect(undoText([{ kind: 'media', command: 'next', app: null }])).toBeNull();
  });
});

describe('buildAutomation', () => {
  test('a saved automation round-trips through the editor', () => {
    expect(buildAutomation(toDraft(pauseMusic), choices, 'pause_music')).toEqual({ ok: true, automation: pauseMusic });
    const flag: AutomationDef = {
      ...pauseMusic,
      enabled: false,
      when: { app: 'example', value: 'icon_visible', is: 'notEquals', target: false },
      undo: false,
      cooldownMs: 0,
    };
    expect(buildAutomation(toDraft(flag), choices, 'x')).toEqual({ ok: true, automation: { ...flag, id: 'x' } });
  });

  test('says what to fill in next', () => {
    const next = (draft: typeof EMPTY_DRAFT): string => {
      const built = buildAutomation(draft, choices, '');
      return built.ok ? 'ok' : built.next;
    };
    expect(next(EMPTY_DRAFT)).toBe('Name the automation.');
    const named = { ...EMPTY_DRAFT, name: '  Pause music ' };
    expect(next(named)).toBe('Pick the value to watch.');
    expect(next({ ...named, source: { app: 'example', value: 'gone' } })).toBe('Pick the value to watch.');
    const watching = { ...named, source: { app: 'example', value: 'progress' } };
    for (const number of ['', 'ninety', '0x5A', '1e2', '12,5', 'Infinity']) {
      expect(next({ ...watching, number })).toBe('Enter the number Progress is compared with, such as 90 or 12.5.');
    }
    expect(next({ ...watching, number: '-.5' })).toBe('ok');
    const complete = { ...watching, number: ' 90 ' };
    expect(next(complete)).toBe('ok');
    expect(next({ ...complete, actions: [] })).toBe('Add an action.');
    expect(next({ ...complete, actions: [newAction(1), { ...newAction(2), target: 'app', app: '  ' }] })).toBe(
      'Name the application for action 2, such as Player.exe.',
    );
    for (const percent of ['', '101', '-1', '2.5', 'x']) {
      expect(next({ ...complete, actions: [{ ...newAction(1, 'volume'), percent }] })).toBe(
        'Set the volume of action 1 to a whole number from 0 to 100.',
      );
    }
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
    expect(buildAutomation(draft, choices, '')).toEqual({
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
    const built = buildAutomation({ ...picked, flag: false }, choices, '');
    expect(built.ok && built.automation.when).toEqual({
      app: 'example',
      value: 'icon_visible',
      is: 'equals',
      target: false,
    });
    expect(buildAutomation({ ...picked, is: 'atLeast' }, choices, '')).toEqual({
      ok: false,
      next: 'Pick how Icon visible is compared.',
    });
  });

  test('a new draft pauses whatever is playing and undoes it', () => {
    expect(EMPTY_DRAFT.actions).toEqual([newAction(0)]);
    expect(newAction(3)).toEqual({
      key: 3,
      kind: 'media',
      command: 'pause',
      percent: '20',
      target: 'default',
      app: '',
    });
    expect(toDraft(pauseMusic).actions.map((action) => [action.key, action.target, action.app])).toEqual([
      [0, 'app', 'Player.exe'],
      [1, 'default', ''],
    ]);
  });
});
