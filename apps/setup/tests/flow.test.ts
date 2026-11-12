import { describe, expect, test } from 'bun:test';

import type { AppSummary, AutomationDef, ValueDef } from '../src/lib/bindings.ts';
import { EMPTY_DRAFT, toDraft } from '../src/lib/automation.ts';
import {
  type ApplicationFlow,
  type AutomationFlow,
  type Ended,
  type Made,
  type RefFlow,
  type ValueFlow,
  afterFlow,
  applicationFor,
  heldInAll,
  holdsAutomation,
  lostText,
  savedNote,
  trail,
  valueFor,
  waiting,
} from '../src/lib/flow.ts';

const pauseMusic: AutomationDef = {
  id: 'pause_music',
  name: 'Pause music',
  enabled: true,
  when: { app: 'example', value: 'progress', is: 'atLeast', target: 90 },
  actions: [{ kind: 'media', command: 'pause', app: null }],
  undo: true,
  cooldownMs: 0,
};

const progress: ValueDef = {
  id: 'progress',
  name: 'Progress',
  screen: {
    windowSize: { width: 100, height: 50 },
    region: { x: 0, y: 0, width: 10, height: 10 },
    check: { kind: 'barFill', filled: { red: 0, green: 0, blue: 0 }, empty: { red: 9, green: 9, blue: 9 } },
    fixes: [],
  },
};

const example: AppSummary = { id: 'example', file: null, error: null, windows: [], running: null };

const automation: AutomationFlow = { kind: 'automation', existing: null, draft: null, app: null, handed: null };
const typed = { ...EMPTY_DRAFT, name: 'Pause music' };
/** A new automation with a name typed, as it waits under a flow it opened. */
const waits = waiting(automation, typed);
const value: ValueFlow = { kind: 'value', changing: null, app: null, opener: null };
const reference: RefFlow = { kind: 'reference', existing: null, app: null };
const application: ApplicationFlow = { kind: 'application', existing: null, opener: null };

const canceled: Ended = { kind: 'canceled' };
/** A save heard after the read of the saved lists numbered 4 started. */
function saved(made: Made | null): Ended {
  return { kind: 'saved', made, read: 4 };
}
const savedPlayer = saved({ app: 'player', value: null });

describe('a flow opens another', () => {
  test('an automation waits with its draft, and with nothing an earlier flow handed to it', () => {
    const before = { ...automation, app: 'player', handed: { value: 'volume', after: 2 } };
    expect(waiting(before, typed)).toEqual({ ...automation, draft: typed, app: 'player', handed: null });
  });

  test('a value for an automation starts with the application its condition has, which the automation keeps', () => {
    expect(valueFor(waits, 'example')).toEqual({
      kind: 'value',
      changing: null,
      app: 'example',
      opener: { ...waits, app: 'example' },
    });
  });

  test('the flow an application is opened for keeps the application it showed chosen', () => {
    expect(applicationFor(value, 'player', null)).toEqual({ ...application, opener: { ...value, app: 'player' } });
    expect(applicationFor(waits, 'player', null).opener).toEqual({ ...waits, app: 'player' });
    // Nothing chosen stays nothing chosen, whatever the flow started with.
    expect(applicationFor({ ...value, app: 'example' }, null, null).opener).toEqual(value);
    expect(applicationFor(value, 'example', example)).toEqual({
      kind: 'application',
      existing: example,
      opener: { ...value, app: 'example' },
    });
  });
});

describe('a flow opened for another ends', () => {
  test('a saved application is what the flow that opened it starts with', () => {
    expect(afterFlow(applicationFor(value, 'example', null), savedPlayer, true)).toEqual({
      kind: 'flow',
      flow: { ...value, app: 'player' },
    });
    expect(afterFlow(applicationFor(reference, null, null), savedPlayer, true)).toEqual({
      kind: 'flow',
      flow: { ...reference, app: 'player' },
    });
    expect(afterFlow(applicationFor(waits, 'example', null), savedPlayer, true)).toEqual({
      kind: 'flow',
      flow: { ...waits, app: 'player', handed: { value: null, after: 4 } },
    });
  });

  test('a saved value is handed to the automation that opened its flow, with the draft it left', () => {
    expect(afterFlow(valueFor(waits, 'example'), saved({ app: 'player', value: 'volume' }), true)).toEqual({
      kind: 'flow',
      flow: { ...automation, draft: typed, app: 'player', handed: { value: 'volume', after: 4 } },
    });
  });

  test('what is handed over says which read of the saved lists it came after', () => {
    const later: Ended = { kind: 'saved', made: { app: 'player', value: 'volume' }, read: 9 };
    const after = afterFlow(valueFor(waits, 'example'), later, true);
    expect(after.kind === 'flow' && after.flow.kind === 'automation' ? after.flow.handed : null).toEqual({
      value: 'volume',
      after: 9,
    });
  });

  test('an application saved for a value keeps the automation that value is for', () => {
    const forValue = valueFor(waits, 'example');
    expect(afterFlow(applicationFor(forValue, 'example', null), savedPlayer, true)).toEqual({
      kind: 'flow',
      flow: { ...forValue, app: 'player' },
    });
  });

  test('a canceled flow shows the flow that opened it as it was left', () => {
    expect(afterFlow(valueFor(waits, 'example'), canceled, true)).toEqual({
      kind: 'flow',
      flow: { ...waits, app: 'example' },
    });
    expect(afterFlow(applicationFor(waits, null, null), canceled, true)).toEqual({ kind: 'flow', flow: waits });
    // So does a save that made nothing for it.
    expect(afterFlow(applicationFor(waits, null, null), saved(null), true)).toEqual({ kind: 'flow', flow: waits });
    expect(afterFlow(applicationFor(value, 'player', null), canceled, true)).toEqual({
      kind: 'flow',
      flow: { ...value, app: 'player' },
    });
  });

  test('a save that finishes after the user left the flow changes nothing', () => {
    expect(afterFlow(valueFor(waits, 'example'), saved({ app: 'example', value: 'progress' }), false)).toEqual({
      kind: 'nothing',
    });
    expect(afterFlow(applicationFor(value, null, null), savedPlayer, false)).toEqual({ kind: 'nothing' });
    expect(afterFlow(application, savedPlayer, false)).toEqual({ kind: 'nothing' });
    expect(afterFlow(automation, saved(null), false)).toEqual({ kind: 'nothing' });
  });
});

describe('a flow opened from a page ends', () => {
  test('shows the page that lists what it saved, and the page under it when canceled', () => {
    expect(afterFlow(application, savedPlayer, true)).toEqual({ kind: 'listed' });
    expect(afterFlow(value, saved({ app: 'example', value: 'progress' }), true)).toEqual({ kind: 'listed' });
    expect(afterFlow(automation, saved(null), true)).toEqual({ kind: 'listed' });
    expect(afterFlow(application, canceled, true)).toEqual({ kind: 'page' });
    expect(afterFlow(reference, canceled, true)).toEqual({ kind: 'page' });
  });
});

describe('the breadcrumb', () => {
  test('starts at the page a flow opened from', () => {
    expect(trail(automation)).toEqual(['Automations']);
    expect(trail(value)).toEqual(['Values']);
    expect(trail(reference)).toEqual(['References']);
    expect(trail(application)).toEqual(['Applications']);
  });

  test('names each flow that waits, the first one opened first', () => {
    const forValue = valueFor(waits, 'example');
    expect(trail(forValue)).toEqual(['Automations', 'New automation']);
    expect(trail(applicationFor(forValue, null, null))).toEqual(['Automations', 'New automation', 'New value']);
    expect(trail(applicationFor(reference, null, null))).toEqual(['References', 'New reference']);
  });

  test('names a waiting flow by what it changes', () => {
    const editing = { ...automation, existing: pauseMusic };
    expect(trail(valueFor(editing, 'example'))).toEqual(['Automations', 'Pause music']);
    const fixing: ValueFlow = {
      kind: 'value',
      changing: { app: 'example', value: progress, how: 'fix' },
      app: null,
      opener: null,
    };
    expect(trail(applicationFor(fixing, 'example', example))).toEqual(['Values', 'Fix Progress']);
    const edited: ValueFlow = { ...fixing, changing: { app: 'example', value: progress, how: 'edit' } };
    expect(trail(applicationFor(edited, 'example', example))).toEqual(['Values', 'Edit Progress']);
    const percent = { id: 'percent', name: 'Percent', ink: { red: 0, green: 0, blue: 0 }, tolerance: 0, samples: [] };
    const changing = { ...reference, existing: { app: 'example', reference: percent } };
    expect(trail(applicationFor(changing, 'example', example))).toEqual(['References', 'Percent']);
  });
});

describe('leaving a stack of flows', () => {
  const forChanged = valueFor(waits, 'example');
  const forUntouched = valueFor(waiting(automation, EMPTY_DRAFT), 'example');

  test('loses what the open flow reports', () => {
    expect(heldInAll(value, 'unsaved')).toBe('unsaved');
    expect(heldInAll(value, 'nothing')).toBe('nothing');
    expect(heldInAll(null, 'nothing')).toBe('nothing');
  });

  test('loses the changed draft of an automation that waits, at any depth', () => {
    expect(heldInAll(forChanged, 'nothing')).toBe('unsaved');
    expect(heldInAll(applicationFor(forChanged, 'example', null), 'nothing')).toBe('unsaved');
    expect(heldInAll(applicationFor(waits, null, null), 'nothing')).toBe('unsaved');
  });

  test('loses nothing of an automation that waits as it started', () => {
    expect(heldInAll(forUntouched, 'nothing')).toBe('nothing');
    expect(heldInAll(valueFor(automation, 'example'), 'nothing')).toBe('nothing');
    const unchanged = waiting({ ...automation, existing: pauseMusic }, toDraft(pauseMusic));
    expect(heldInAll(valueFor(unchanged, 'example'), 'nothing')).toBe('nothing');
    const changed = waiting(unchanged, { ...toDraft(pauseMusic), cooldown: '5' });
    expect(heldInAll(valueFor(changed, 'example'), 'nothing')).toBe('unsaved');
  });

  test('waits for a save under way whatever waits under it', () => {
    expect(heldInAll(forChanged, 'saving')).toBe('saving');
    expect(heldInAll(forUntouched, 'saving')).toBe('saving');
  });

  test('the question names what the open flow loses', () => {
    expect(lostText(value, 'unsaved')).toBe(
      'Leave without saving? The screenshots, the region and any recording are lost.',
    );
    expect(lostText(forUntouched, 'unsaved')).toBe(
      'Leave without saving? The screenshots, the region and any recording are lost.',
    );
    expect(lostText(reference, 'unsaved')).toBe(
      'Leave without saving? The screenshots, the labels and any recording are lost.',
    );
    // An edit leaves the saved value as it is, so only its changes go; a fix loses what a new value does.
    const changing = (how: 'fix' | 'edit'): ValueFlow => ({
      kind: 'value',
      changing: { app: 'example', value: progress, how },
      app: null,
      opener: null,
    });
    expect(lostText(changing('edit'), 'unsaved')).toBe(
      'Leave without saving? The screenshots and the changes to this value are lost.',
    );
    expect(lostText(changing('fix'), 'unsaved')).toBe(
      'Leave without saving? The screenshots, the region and any recording are lost.',
    );
    expect(lostText(automation, 'unsaved')).toBe('Leave without saving? The changes to this automation are lost.');
    expect(lostText(application, 'unsaved')).toBe('Leave without saving? The changes to this application are lost.');
  });

  test('the question names the automation that waits too', () => {
    expect(lostText(forChanged, 'unsaved')).toBe(
      'Leave without saving? The screenshots, the region and any recording are lost. The unsaved changes to the automation are lost too.',
    );
    expect(lostText(applicationFor(forChanged, 'example', null), 'unsaved')).toBe(
      'Leave without saving? The changes to this application are lost. The unsaved changes to the automation are lost too.',
    );
  });

  test('the question names only the automation while the open flow holds nothing', () => {
    expect(lostText(forChanged, 'nothing')).toBe(
      'Leave without saving? The unsaved changes to the automation are lost.',
    );
  });
});

test('a capture says where its value shows once saved: back in the automation that waits, or on its page', () => {
  expect(savedNote(trail(value), 'Example App')).toBe('Saved values show on the Values page, under Example App.');
  expect(savedNote(trail(valueFor(waits, 'example')), 'Example App')).toBe(
    'Saving returns to the automation with this value picked. The value also shows on the Values page, under Example App.',
  );
});

describe('an automation being made', () => {
  test('lasts while its flow is open or waits under the flows it opened', () => {
    const forAutomation = valueFor(waits, 'example');
    expect(holdsAutomation(automation)).toBe(true);
    expect(holdsAutomation(forAutomation)).toBe(true);
    expect(holdsAutomation(applicationFor(waits, null, null))).toBe(true);
    expect(holdsAutomation(applicationFor(forAutomation, 'example', null))).toBe(true);
  });

  test('is over once no automation flow is left', () => {
    expect(holdsAutomation(null)).toBe(false);
    expect(holdsAutomation(value)).toBe(false);
    expect(holdsAutomation(reference)).toBe(false);
    expect(holdsAutomation(applicationFor(value, null, null))).toBe(false);
  });
});
