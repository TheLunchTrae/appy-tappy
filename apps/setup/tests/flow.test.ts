import { describe, expect, test } from 'bun:test';

import type { AppSummary, AutomationDef, ValueDef } from '../src/lib/bindings.ts';
import { EMPTY_DRAFT, toDraft } from '../src/lib/automation.ts';
import {
  type ApplicationFlow,
  type AutomationFlow,
  type ReferenceFlow,
  type ValueFlow,
  afterSave,
  applicationFor,
  heldInAll,
  lostText,
  resume,
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

const automation: AutomationFlow = { kind: 'automation', existing: null, draft: null, app: null, value: null };
const typed = { ...EMPTY_DRAFT, name: 'Pause music' };
/** A new automation with a name typed, waiting under a flow it opened with the application `example` chosen. */
const waits = waiting(automation, typed, 'example');
const value: ValueFlow = { kind: 'value', fixing: null, app: null, opener: null };
const reference: ReferenceFlow = { kind: 'reference', existing: null, app: null };
const application: ApplicationFlow = { kind: 'application', existing: null, opener: null };

describe('a flow opens another', () => {
  test('an automation waits with its draft and chosen application, and no value from an earlier flow', () => {
    const before = { ...automation, app: 'player', value: 'volume' };
    expect(waiting(before, typed, 'example')).toEqual({ ...automation, draft: typed, app: 'example', value: null });
  });

  test('a value for an automation starts with the application of its condition', () => {
    expect(valueFor(waits)).toEqual({ kind: 'value', fixing: null, app: 'example', opener: waits });
  });

  test('an added application keeps its opener as it is, and a changed one is what the opener starts with', () => {
    expect(applicationFor(value, null)).toEqual({ ...application, opener: value });
    expect(applicationFor(value, example)).toEqual({
      kind: 'application',
      existing: example,
      opener: { ...value, app: 'example' },
    });
  });
});

describe('a flow ends', () => {
  test('a saved application is what the flow that opened it starts with', () => {
    const made = { app: 'player', value: null };
    expect(resume(applicationFor(value, null), made)).toEqual({ ...value, app: 'player' });
    expect(resume(applicationFor(reference, null), made)).toEqual({ ...reference, app: 'player' });
    expect(resume(applicationFor(waits, null), made)).toEqual({ ...waits, app: 'player' });
  });

  test('a saved value is picked in the automation that opened its flow, with the draft it left', () => {
    expect(resume(valueFor(waits), { app: 'player', value: 'volume' })).toEqual({
      ...automation,
      draft: typed,
      app: 'player',
      value: 'volume',
    });
  });

  test('an application saved for a value keeps the automation that value is for', () => {
    const forValue = applicationFor(valueFor(waits), null);
    expect(resume(forValue, { app: 'player', value: null })).toEqual({ ...valueFor(waits), app: 'player' });
  });

  test('a canceled flow shows the flow that opened it as it was', () => {
    expect(resume(valueFor(waits), null)).toBe(waits);
    expect(resume(applicationFor(waits, null), null)).toBe(waits);
  });

  test('a flow opened from a page returns to the page', () => {
    expect(resume(value, { app: 'example', value: 'progress' })).toBeNull();
    expect(resume(application, { app: 'example', value: null })).toBeNull();
    expect(resume(reference, null)).toBeNull();
    expect(resume(automation, null)).toBeNull();
  });
});

describe('a save', () => {
  const forAutomation = valueFor(waits);
  const made = { app: 'example', value: 'progress' };

  test('shows the flow that opened the saved one, with what it made', () => {
    expect(afterSave(forAutomation, forAutomation, made)).toEqual({
      kind: 'flow',
      flow: { ...waits, app: 'example', value: 'progress' },
    });
  });

  test('opens the page when no flow opened the saved one', () => {
    expect(afterSave(value, value, made)).toEqual({ kind: 'page' });
    expect(afterSave(automation, automation, null)).toEqual({ kind: 'page' });
  });

  test('that finishes after the user moved on changes nothing', () => {
    expect(afterSave(null, forAutomation, made)).toEqual({ kind: 'stay' });
    expect(afterSave(waits, forAutomation, made)).toEqual({ kind: 'stay' });
    // Another flow of the same content is still another flow.
    expect(afterSave(valueFor(waits), forAutomation, made)).toEqual({ kind: 'stay' });
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
    expect(trail(valueFor(waits))).toEqual(['Automations', 'New automation']);
    expect(trail(applicationFor(valueFor(waits), null))).toEqual(['Automations', 'New automation', 'New value']);
    expect(trail(applicationFor(reference, null))).toEqual(['References', 'New reference']);
  });

  test('names a waiting flow by what it changes', () => {
    const editing = { ...automation, existing: pauseMusic };
    expect(trail(valueFor(editing))).toEqual(['Automations', 'Pause music']);
    const fixing = { ...value, fixing: { app: 'example', value: progress } };
    expect(trail(applicationFor(fixing, null))).toEqual(['Values', 'Fix Progress']);
    const percent = { id: 'percent', name: 'Percent', ink: { red: 0, green: 0, blue: 0 }, tolerance: 0, samples: [] };
    const changing = { ...reference, existing: { app: 'example', reference: percent } };
    expect(trail(applicationFor(changing, null))).toEqual(['References', 'Percent']);
  });
});

describe('leaving a stack of flows', () => {
  const untouched = waiting(automation, EMPTY_DRAFT, null);

  test('loses what the open flow reports', () => {
    expect(heldInAll(value, 'unsaved')).toBe('unsaved');
    expect(heldInAll(value, 'nothing')).toBe('nothing');
    expect(heldInAll(null, 'nothing')).toBe('nothing');
  });

  test('loses the changed draft of an automation that waits, at any depth', () => {
    expect(heldInAll(valueFor(waits), 'nothing')).toBe('unsaved');
    expect(heldInAll(applicationFor(valueFor(waits), null), 'nothing')).toBe('unsaved');
    expect(heldInAll(applicationFor(waits, null), 'nothing')).toBe('unsaved');
  });

  test('loses nothing of an automation that waits as it started', () => {
    expect(heldInAll(valueFor(untouched), 'nothing')).toBe('nothing');
    expect(heldInAll(valueFor(automation), 'nothing')).toBe('nothing');
    const saved = waiting({ ...automation, existing: pauseMusic }, toDraft(pauseMusic), 'example');
    expect(heldInAll(valueFor(saved), 'nothing')).toBe('nothing');
    const changed = waiting(saved, { ...toDraft(pauseMusic), cooldown: '5' }, 'example');
    expect(heldInAll(valueFor(changed), 'nothing')).toBe('unsaved');
  });

  test('waits for a save under way whatever waits under it', () => {
    expect(heldInAll(valueFor(waits), 'saving')).toBe('saving');
    expect(heldInAll(valueFor(untouched), 'saving')).toBe('saving');
  });

  test('the question names what the open flow loses', () => {
    expect(lostText(value, 'unsaved')).toBe(
      'Leave without saving? The screenshots, the region and any recording are lost.',
    );
    expect(lostText(valueFor(untouched), 'unsaved')).toBe(
      'Leave without saving? The screenshots, the region and any recording are lost.',
    );
    expect(lostText(reference, 'unsaved')).toBe(
      'Leave without saving? The screenshots, the labels and any recording are lost.',
    );
    expect(lostText(automation, 'unsaved')).toBe('Leave without saving? The changes to this automation are lost.');
    expect(lostText(application, 'unsaved')).toBe('Leave without saving? The changes to this application are lost.');
  });

  test('the question names the automation that waits too', () => {
    expect(lostText(valueFor(waits), 'unsaved')).toBe(
      'Leave without saving? The screenshots, the region and any recording are lost. The unsaved changes to the automation are lost too.',
    );
    expect(lostText(applicationFor(valueFor(waits), null), 'unsaved')).toBe(
      'Leave without saving? The changes to this application are lost. The unsaved changes to the automation are lost too.',
    );
  });

  test('the question names only the automation while the open flow holds nothing', () => {
    expect(lostText(valueFor(waits), 'nothing')).toBe(
      'Leave without saving? The unsaved changes to the automation are lost.',
    );
  });
});
