import { describe, expect, test } from 'bun:test';

import type { AppSummary, AutomationDef, CheckDef, ValueDef } from '../src/lib/api.ts';
import {
  EMPTY_DRAFT,
  actionText,
  buildAutomation,
  buildWhen,
  findValue,
  keysBlocked,
  liveAnnouncement,
  liveText,
  newAction,
  readingText,
  targetText,
  toDraft,
  undoable,
  undoText,
  usedBy,
  valueChoices,
  valueKind,
  withSource,
} from '../src/lib/automation.ts';

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

const progress = value('progress', 'Progress', { kind: 'number', reference: 'percent', whole: false });
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
        mustMatch: { title: false, class: false },
        fallback: 'none',
      },
      antiCheat: 'unknown',
      values: [progress, iconVisible],
      references: [],
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
  test('number and bar fill checks read numbers; color and image checks read yes or no', () => {
    expect(valueKind({ kind: 'number', reference: 'percent', whole: true })).toBe('number');
    expect(valueKind({ kind: 'barFill', filled: white, empty: white })).toBe('number');
    expect(valueKind({ kind: 'color', color: white, tolerance: 0 })).toBe('flag');
    expect(valueKind({ kind: 'image', image: 'a.png', tolerance: 0 })).toBe('flag');
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
    expect(actionText({ kind: 'keys', keys: 'Ctrl+Shift+M' })).toBe('Press Ctrl+Shift+M');
  });

  test('undo says what it puts back, and skips cannot be undone', () => {
    expect(undoable({ kind: 'media', command: 'pause', app: null })).toBe(true);
    expect(undoable({ kind: 'media', command: 'play', app: null })).toBe(true);
    expect(undoable({ kind: 'media', command: 'next', app: null })).toBe(false);
    expect(undoable({ kind: 'media', command: 'previous', app: null })).toBe(false);
    expect(undoable({ kind: 'volume', percent: 5, app: null })).toBe(true);
    expect(undoable({ kind: 'keys', keys: 'M' })).toBe(false);
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

  test('a loaded automation gives each action its own key and target', () => {
    expect(toDraft(pauseMusic).actions.map((action) => [action.key, action.target, action.app])).toEqual([
      [0, 'app', 'Player.exe'],
      [1, 'default', ''],
    ]);
  });
});

describe('key presses', () => {
  const free: readonly AppSummary[] = apps.map((app) =>
    app.file === null ? app : { ...app, file: { ...app.file, antiCheat: 'none' } },
  );
  const pressKeys: AutomationDef = {
    ...pauseMusic,
    actions: [{ kind: 'keys', keys: 'Ctrl+Shift+M' }],
    undo: false,
  };

  test('round-trip through the editor for an application without anti-cheat', () => {
    const draft = toDraft(pressKeys);
    expect(draft.actions.map((action) => [action.kind, action.keys, action.target, action.app])).toEqual([
      ['keys', 'Ctrl+Shift+M', 'default', ''],
    ]);
    expect(buildAutomation(draft, valueChoices(free), 'pause_music')).toEqual({ ok: true, automation: pressKeys });
  });

  test('need keys an action can press, in the one spelling the file keeps', () => {
    const draft = toDraft(pressKeys);
    const typed = (keys: string): ReturnType<typeof buildAutomation> =>
      buildAutomation(
        { ...draft, actions: draft.actions.map((action) => ({ ...action, keys })) },
        valueChoices(free),
        'pause_music',
      );
    expect(typed('  ')).toEqual({ ok: false, next: 'Choose the keys for action 1 to press.' });
    expect(typed('Ctrl+Mute')).toEqual({
      ok: false,
      next: 'Spell the keys for action 1 as any of Ctrl, Alt, Shift and Win, then one key such as M, 5, F9, Space or Enter, joined by +, such as Ctrl+Shift+M.',
    });
    expect(typed(' ctrl + m ')).toEqual({
      ok: true,
      automation: { ...pressKeys, actions: [{ kind: 'keys', keys: 'Ctrl+M' }] },
    });
  });

  test("wait for the application's anti-cheat to be set to none", () => {
    const blocked =
      "Anti-cheat can treat pressed keys as cheating, so keys are pressed only once Example App's anti-cheat is set to None on the Applications page.";
    expect(buildAutomation(toDraft(pressKeys), choices, 'pause_music')).toEqual({ ok: false, next: blocked });
    expect(keysBlocked(pressKeys.actions, 'unknown', 'Example App')).toBe(blocked);
    expect(keysBlocked(pressKeys.actions, 'userMode', 'Example App')).toBe(blocked);
    expect(keysBlocked(pressKeys.actions, 'kernelLevel', 'Example App')).toBe(blocked);
    expect(keysBlocked(pressKeys.actions, 'none', 'Example App')).toBeNull();
    expect(keysBlocked(pauseMusic.actions, 'kernelLevel', 'Example App')).toBeNull();
  });

  test('cannot be undone', () => {
    const undone = (actions: AutomationDef['actions']): ReturnType<typeof buildAutomation> =>
      buildAutomation({ ...toDraft({ ...pressKeys, actions }), undo: true }, valueChoices(free), 'pause_music');
    expect(undone(pressKeys.actions)).toEqual({ ok: false, next: 'Turn off undo: pressed keys cannot be unpressed.' });
    expect(undone([{ kind: 'media', command: 'next', app: null }, ...pressKeys.actions])).toEqual({
      ok: false,
      next: 'Turn off undo: a skipped track cannot be unskipped, and pressed keys cannot be unpressed.',
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
    expect(buildWhen(draft, number)).toBe('Enter the number Progress is compared with, such as 90 or 12.5.');
    expect(buildWhen({ ...draft, flag: false, is: 'notEquals' }, icon)).toEqual({
      app: 'example',
      value: 'icon_visible',
      is: 'notEquals',
      target: false,
    });
    expect(buildWhen(draft, icon)).toBe('Pick how Icon visible is compared.');
  });
});

describe('readingText', () => {
  test('shows numbers as read, flags as yes or no', () => {
    expect(readingText({ kind: 'number', value: 87 })).toBe('87');
    expect(readingText({ kind: 'number', value: 12.46 })).toBe('12.46');
    expect(readingText({ kind: 'number', value: 99.95 })).toBe('99.95');
    expect(readingText({ kind: 'number', value: 100 / 3 })).toBe('33.3333');
    expect(readingText({ kind: 'flag', value: true })).toBe('Yes');
    expect(readingText({ kind: 'flag', value: false }, true)).toBe('No');
  });

  test('never rounds a number onto or across the target it is compared with', () => {
    const close = 99.999_996;
    expect(readingText({ kind: 'number', value: close })).toBe('100.0000');
    expect(readingText({ kind: 'number', value: close }, 100)).toBe(String(close));
    expect(readingText({ kind: 'number', value: close }, 99)).toBe('100.0000');
    expect(readingText({ kind: 'number', value: 33.333_333_7 }, 33.3333)).toBe('33.3333337');
    expect(readingText({ kind: 'number', value: 33.333_333_7 }, 33)).toBe('33.3333');
  });
});

describe('liveText', () => {
  const progress = findValue(choices, { app: 'example', value: 'progress' });
  if (progress === null) {
    throw new Error('the value exists');
  }
  const when = { app: 'example', value: 'progress', is: 'atLeast' as const, target: 100 };
  const read = (holds: boolean | null) =>
    ({ kind: 'read', reading: { kind: 'number', value: 99.999_996 }, holds }) as const;

  test('says what the value reads and whether the condition is true', () => {
    expect(liveText(null, progress, when)).toBe('Looking for Example App…');
    expect(liveText({ kind: 'waiting' }, progress, when)).toBe('Waiting for Example App to show Progress…');
    expect(liveText(read(null), progress, null)).toBe('Progress reads 100.0000 now.');
    expect(liveText(read(false), progress, when)).toBe('Progress reads 99.999996 now, so the condition is not true.');
    expect(liveText({ ...read(true), reading: { kind: 'number', value: 100 } }, progress, when)).toBe(
      'Progress reads 100 now, so the condition is true.',
    );
    for (const kind of ['problem', 'lost', 'error'] as const) {
      expect(liveText({ kind, message: 'Resize the window.' }, progress, when)).toBe('Resize the window.');
    }
  });

  test('announces only the verdict or what is wrong', () => {
    expect(liveAnnouncement(null)).toBe('');
    expect(liveAnnouncement({ kind: 'waiting' })).toBe('');
    expect(liveAnnouncement(read(null))).toBe('');
    expect(liveAnnouncement(read(true))).toBe('The condition is true.');
    expect(liveAnnouncement(read(false))).toBe('The condition is not true.');
    expect(liveAnnouncement({ kind: 'lost', message: 'The window closed.' })).toBe('The window closed.');
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
