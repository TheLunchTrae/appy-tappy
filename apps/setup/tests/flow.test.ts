import { describe, expect, test } from 'bun:test';

import type { AppSummary, AutomationDef, DraftDef, ValueDef } from '../src/lib/bindings.ts';
import { EMPTY_DRAFT, newAction, toDraft } from '../src/lib/automation.ts';
import { type Resumed, keep } from '../src/lib/drafts.ts';
import {
  type ApplicationFlow,
  type AutomationFlow,
  type Ended,
  type Keeping,
  type Made,
  type RefFlow,
  type ValueFlow,
  afterFlow,
  applicationFor,
  automationDraft,
  draftUnder,
  heldInAll,
  holdsAutomation,
  keepStep,
  keptAutomation,
  leaveQuestion,
  resumedFlow,
  savedNote,
  startOf,
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

const clickPicture = {
  kind: 'picture' as const,
  windowSize: { width: 100, height: 50 },
  search: { x: 0, y: 0, width: 10, height: 10 },
  press: { x: 1, y: 1 },
  tolerance: 20,
};

const automation: AutomationFlow = {
  kind: 'automation',
  existing: null,
  draft: null,
  app: null,
  handed: null,
  resumed: null,
};
const typed = { ...EMPTY_DRAFT, name: 'Pause music' };
/** A new automation with a name typed, as it waits under a flow it opened. */
const waits = waiting(automation, typed);
const value: ValueFlow = { kind: 'value', changing: null, app: null, opener: null, resumed: null };
const reference: RefFlow = { kind: 'reference', existing: null, app: null, resumed: null };
const application: ApplicationFlow = { kind: 'application', existing: null, opener: null, resumed: null };

const canceled: Ended = { kind: 'canceled' };
const drafted: Ended = { kind: 'drafted' };
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
      resumed: null,
    });
  });

  test('the flow an application is opened for keeps the application it showed chosen', () => {
    expect(applicationFor(value, 'player', null, null)).toEqual({
      ...application,
      opener: { ...value, app: 'player' },
    });
    expect(applicationFor(waits, 'player', null, null).opener).toEqual({ ...waits, app: 'player' });
    // Nothing chosen stays nothing chosen, whatever the flow started with.
    expect(applicationFor({ ...value, app: 'example' }, null, null, null).opener).toEqual(value);
    expect(applicationFor(value, 'example', example, null)).toEqual({
      kind: 'application',
      existing: example,
      opener: { ...value, app: 'example' },
      resumed: null,
    });
  });
});

describe('a flow opened for another ends', () => {
  test('a saved application is what the flow that opened it starts with', () => {
    expect(afterFlow(applicationFor(value, 'example', null, null), savedPlayer, true)).toEqual({
      kind: 'flow',
      flow: { ...value, app: 'player' },
    });
    expect(afterFlow(applicationFor(reference, null, null, null), savedPlayer, true)).toEqual({
      kind: 'flow',
      flow: { ...reference, app: 'player' },
    });
    expect(afterFlow(applicationFor(waits, 'example', null, null), savedPlayer, true)).toEqual({
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
    expect(afterFlow(applicationFor(forValue, 'example', null, null), savedPlayer, true)).toEqual({
      kind: 'flow',
      flow: { ...forValue, app: 'player' },
    });
  });

  test('a canceled flow shows the flow that opened it as it was left', () => {
    expect(afterFlow(valueFor(waits, 'example'), canceled, true)).toEqual({
      kind: 'flow',
      flow: { ...waits, app: 'example' },
    });
    expect(afterFlow(applicationFor(waits, null, null, null), canceled, true)).toEqual({ kind: 'flow', flow: waits });
    // So does a save that made nothing for it.
    expect(afterFlow(applicationFor(waits, null, null, null), saved(null), true)).toEqual({
      kind: 'flow',
      flow: waits,
    });
    expect(afterFlow(applicationFor(value, 'player', null, null), canceled, true)).toEqual({
      kind: 'flow',
      flow: { ...value, app: 'player' },
    });
  });

  test('a save that finishes after the user left the flow changes nothing', () => {
    expect(afterFlow(valueFor(waits, 'example'), saved({ app: 'example', value: 'progress' }), false)).toEqual({
      kind: 'nothing',
    });
    expect(afterFlow(applicationFor(value, null, null, null), savedPlayer, false)).toEqual({ kind: 'nothing' });
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

  test('shows the page that lists its draft once the draft is saved or deleted', () => {
    expect(afterFlow(automation, drafted, true)).toEqual({ kind: 'listed' });
    expect(afterFlow(value, drafted, true)).toEqual({ kind: 'listed' });
    expect(afterFlow(value, drafted, false)).toEqual({ kind: 'nothing' });
  });
});

describe('a flow opened for another is saved as a draft', () => {
  test('the flow that opened it shows again as it was left, with nothing handed to it', () => {
    expect(afterFlow(valueFor(waits, 'example'), drafted, true)).toEqual({
      kind: 'flow',
      flow: { ...waits, app: 'example' },
    });
    expect(afterFlow(applicationFor(value, 'player', null, null), drafted, true)).toEqual({
      kind: 'flow',
      flow: { ...value, app: 'player' },
    });
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
    expect(trail(applicationFor(forValue, null, null, null))).toEqual(['Automations', 'New automation', 'New value']);
    expect(trail(applicationFor(reference, null, null, null))).toEqual(['References', 'New reference']);
  });

  test('names a waiting flow by what it changes', () => {
    const editing = { ...automation, existing: pauseMusic };
    expect(trail(valueFor(editing, 'example'))).toEqual(['Automations', 'Pause music']);
    const fixing: ValueFlow = {
      kind: 'value',
      changing: { app: 'example', value: progress, how: 'fix' },
      app: null,
      opener: null,
      resumed: null,
    };
    expect(trail(applicationFor(fixing, 'example', example, null))).toEqual(['Values', 'Fix Progress']);
    const edited: ValueFlow = { ...fixing, changing: { app: 'example', value: progress, how: 'edit' } };
    expect(trail(applicationFor(edited, 'example', example, null))).toEqual(['Values', 'Edit Progress']);
    const percent = { id: 'percent', name: 'Percent', ink: { red: 0, green: 0, blue: 0 }, tolerance: 0, samples: [] };
    const changing = { ...reference, existing: { app: 'example', reference: percent } };
    expect(trail(applicationFor(changing, 'example', example, null))).toEqual(['References', 'Percent']);
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
    expect(heldInAll(applicationFor(forChanged, 'example', null, null), 'nothing')).toBe('unsaved');
    expect(heldInAll(applicationFor(waits, null, null, null), 'nothing')).toBe('unsaved');
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

  test('the question names what the open flow loses, and what a draft keeps of it', () => {
    const lost = 'Leave without saving? The screenshots, the region and any recording are lost.';
    expect(leaveQuestion(value, 'unsaved', true)).toEqual({
      text: `${lost} Save as draft keeps this value, without a recording.`,
      drafts: true,
    });
    expect(leaveQuestion(forUntouched, 'unsaved', true).text).toBe(
      `${lost} Save as draft keeps this value, without a recording.`,
    );
    expect(leaveQuestion(reference, 'unsaved', true).text).toBe(
      `Leave without saving? The screenshots, the boxes and any recording are lost. Save as draft keeps this reference, without a recording.`,
    );
    expect(leaveQuestion(automation, 'unsaved', true)).toEqual({
      text: 'Leave without saving? The changes to this automation are lost. Save as draft keeps this automation.',
      drafts: true,
    });
    expect(leaveQuestion(application, 'unsaved', true).text).toBe(
      'Leave without saving? The changes to this application are lost. Save as draft keeps this application.',
    );
    // A flow that cannot save its draft offers none.
    expect(leaveQuestion(value, 'unsaved', false)).toEqual({ text: lost, drafts: false });
  });

  test('a fix can be a draft and an edit cannot, and the question says so', () => {
    const changing = (how: 'fix' | 'edit'): ValueFlow => ({
      kind: 'value',
      changing: { app: 'example', value: progress, how },
      app: null,
      opener: null,
      resumed: null,
    });
    // An edit leaves the saved value as it is, so only its changes go; a fix loses what a new value does.
    for (const savable of [true, false]) {
      expect(leaveQuestion(changing('edit'), 'unsaved', savable)).toEqual({
        text: 'Leave without saving? The screenshots and the changes to this value are lost. An edit cannot be saved as a draft.',
        drafts: false,
      });
    }
    expect(leaveQuestion(changing('fix'), 'unsaved', true)).toEqual({
      text: 'Leave without saving? The screenshots, the region and any recording are lost. Save as draft keeps this fix, without a recording.',
      drafts: true,
    });
  });

  test('the question names the automation that waits too, which a draft of its own keeps', () => {
    expect(leaveQuestion(forChanged, 'unsaved', true)).toEqual({
      text: 'Leave without saving? The screenshots, the region and any recording are lost. The unsaved changes to the automation are lost too. Save as draft keeps both, each as its own draft, without a recording.',
      drafts: true,
    });
    expect(leaveQuestion(applicationFor(forChanged, 'example', null, null), 'unsaved', true).text).toBe(
      'Leave without saving? The changes to this application are lost. The unsaved changes to the automation are lost too. Save as draft keeps both, each as its own draft.',
    );
  });

  test('the question names only the automation while the open flow holds nothing', () => {
    for (const savable of [true, false]) {
      expect(leaveQuestion(forChanged, 'nothing', savable)).toEqual({
        text: 'Leave without saving? The unsaved changes to the automation are lost. Save as draft keeps the automation.',
        drafts: true,
      });
    }
  });
});

describe('an automation saved as a draft', () => {
  /** Every field the editor sets, each away from what a new automation starts with. */
  const full = {
    name: ' Skip intro ',
    enabled: false,
    source: { app: 'example', value: 'progress' },
    is: 'below' as const,
    number: '12.5',
    flag: false,
    state: 'Paused',
    actions: [
      {
        ...newAction(4, 'click'),
        clickApp: 'player',
        button: 'right' as const,
        delivery: 'message' as const,
        returnFocus: false,
        at: { kind: 'spot' as const, corner: 'topLeft' as const, x: 3, y: 4 },
      },
      { ...newAction(7, 'keys'), keys: 'Ctrl+Shift+M' },
    ],
    undo: false,
    cooldown: '2.5',
  };
  const setup = { draft: full, app: 'example' };
  /** A draft as its file gives it back. */
  function stored(def: DraftDef, id = 'd1'): Resumed {
    const state: unknown = JSON.parse(JSON.stringify(def.state));
    return { id, def: { ...def, state } };
  }

  test('sets every field of the editor, so the round trip below misses none', () => {
    const set: Readonly<Record<string, unknown>> = { ...full };
    for (const [key, start] of Object.entries(EMPTY_DRAFT)) {
      expect([key, set[key]]).not.toEqual([key, start]);
    }
    const [click] = full.actions;
    const fresh = newAction(4, 'click');
    for (const key of ['clickApp', 'button', 'delivery', 'returnFocus', 'at'] as const) {
      expect([key, click?.[key]]).not.toEqual([key, fresh[key]]);
    }
  });

  test('comes back as the flow held it: the whole draft and the application chosen', () => {
    const def = automationDraft(automation, setup, []);
    expect(def).toMatchObject({ kind: 'automation', app: null, changes: null, name: 'Skip intro', pictures: [] });
    const flow = resumedFlow(stored(def), [], []);
    expect(flow).toMatchObject({ kind: 'automation', existing: null, draft: null, app: 'example', handed: null });
    expect(flow?.kind === 'automation' ? startOf(flow) : null).toEqual(setup);
    // The Automations page reads the same state for the draft's row.
    expect(keptAutomation(stored(def).def)).toEqual(setup);
    expect(keptAutomation({ ...def, state: { format: 0, kind: 'automation', state: setup } })).toBeNull();
  });

  test('names the saved automation it changes, its click pictures, and what it still needs', () => {
    const picture = { ...full.actions[0]!, at: { ...clickPicture, image: 'click-3.png' } };
    const def = automationDraft(
      { existing: pauseMusic },
      { draft: { ...EMPTY_DRAFT, actions: [picture] }, app: null },
      [],
    );
    expect(def).toMatchObject({ changes: 'pause_music', name: 'Pause music', pictures: ['click-3.png'] });
    expect(def.missing).toEqual(['Name the automation.', 'Pick the value to watch.']);
    const summary = { automation: pauseMusic, problem: null };
    expect(resumedFlow(stored(def), [], [summary])).toMatchObject({ existing: pauseMusic });
    // The saved automation was deleted meanwhile: the draft makes a new one.
    expect(resumedFlow(stored(def), [], [])).toMatchObject({ existing: null });
  });

  test('a flow that finishes a draft starts from what the draft kept, not from the saved automation', () => {
    const resumed = stored(automationDraft({ existing: pauseMusic }, setup, []));
    const flow: AutomationFlow = { ...automation, existing: pauseMusic, resumed };
    expect(startOf(flow)).toEqual(setup);
    // Waiting as the draft kept it loses nothing; waiting as the saved automation is, which the draft changed, does.
    expect(heldInAll(valueFor(waiting(flow, full), 'example'), 'nothing')).toBe('nothing');
    expect(heldInAll(valueFor(waiting(flow, toDraft(pauseMusic)), 'example'), 'nothing')).toBe('unsaved');
    // A draft another version saved is not read: the flow starts from the saved automation.
    const other = { ...resumed, def: { ...resumed.def, state: { format: 0 } } };
    expect(startOf({ existing: pauseMusic, resumed: other })).toEqual({ draft: toDraft(pauseMusic), app: null });
    expect(startOf(automation)).toEqual({ draft: EMPTY_DRAFT, app: null });
  });

  test('leaving a flow it opened keeps it as a draft of its own, over the draft it finishes', () => {
    const forValue = valueFor(waits, 'example');
    expect(draftUnder(forValue, [])).toEqual({
      id: null,
      def: automationDraft(automation, { draft: typed, app: 'example' }, []),
    });
    expect(draftUnder(applicationFor(forValue, 'example', null, null), [])?.def.name).toBe('Pause music');
    const resumed = stored(automationDraft(automation, setup, []), 'd7');
    const finishing = waiting({ ...automation, resumed }, { ...full, cooldown: '9' });
    expect(draftUnder(valueFor(finishing, 'example'), [])?.id).toBe('d7');
    // Nothing waits with work: the automation is as it started, or no automation waits.
    expect(draftUnder(valueFor(waiting({ ...automation, resumed }, full), 'example'), [])).toBeNull();
    expect(draftUnder(valueFor(waiting(automation, EMPTY_DRAFT), 'example'), [])).toBeNull();
    expect(draftUnder(applicationFor(value, null, null, null), [])).toBeNull();
    expect(draftUnder(automation, [])).toBeNull();
  });
});

describe('a draft opens the flow that finishes it', () => {
  const percent = { id: 'percent', name: 'Percent', ink: { red: 0, green: 0, blue: 0 }, tolerance: 0, samples: [] };
  const saved: AppSummary = {
    ...example,
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
      values: [progress],
      references: [percent],
    },
  };
  const of = (kind: DraftDef['kind'], changes: string | null, app: string | null): Resumed => ({
    id: 'd1',
    def: { kind, app, changes, name: 'Draft', missing: [], pictures: [], state: keep(kind, {}) },
  });

  test('a value and a reference open on their application, a reference on the saved one it changes', () => {
    const value = of('value', null, 'example');
    expect(resumedFlow(value, [saved], [])).toEqual({
      kind: 'value',
      changing: null,
      app: 'example',
      opener: null,
      resumed: value,
    });
    const reference = of('reference', 'percent', 'example');
    expect(resumedFlow(reference, [saved], [])).toEqual({
      kind: 'reference',
      existing: { app: 'example', reference: percent },
      app: 'example',
      resumed: reference,
    });
    expect(resumedFlow(of('reference', 'gone', 'example'), [saved], [])).toMatchObject({ existing: null });
    expect(resumedFlow(of('reference', null, 'example'), [saved], [])).toMatchObject({ existing: null });
  });

  test('a fix opens on the saved value it fixes, and not at all once that value is gone', () => {
    const fix = of('fix', 'progress', 'example');
    expect(resumedFlow(fix, [saved], [])).toEqual({
      kind: 'value',
      changing: { app: 'example', value: progress, how: 'fix' },
      app: null,
      opener: null,
      resumed: fix,
    });
    expect(resumedFlow(of('fix', 'gone', 'example'), [saved], [])).toBeNull();
    expect(resumedFlow(fix, [], [])).toBeNull();
    expect(resumedFlow(of('fix', 'progress', null), [saved], [])).toBeNull();
  });

  test('an application opens on the saved one it changes', () => {
    const changes = of('application', 'example', null);
    expect(resumedFlow(changes, [saved], [])).toEqual({
      kind: 'application',
      existing: saved,
      opener: null,
      resumed: changes,
    });
    expect(resumedFlow(of('application', null, null), [saved], [])).toMatchObject({ existing: null });
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
    expect(holdsAutomation(applicationFor(waits, null, null, null))).toBe(true);
    expect(holdsAutomation(applicationFor(forAutomation, 'example', null, null))).toBe(true);
  });

  test('is over once no automation flow is left', () => {
    expect(holdsAutomation(null)).toBe(false);
    expect(holdsAutomation(value)).toBe(false);
    expect(holdsAutomation(reference)).toBe(false);
    expect(holdsAutomation(applicationFor(value, null, null, null))).toBe(false);
  });
});

describe('changing how an application is found, from a picker', () => {
  const draftOf = (id: string, kind: DraftDef['kind'], changes: string | null) => ({
    id,
    draft: { kind, app: null, changes, name: 'Draft', missing: [], pictures: [], state: keep(kind, {}) },
    problem: null,
    screenshots: 0,
  });
  const drafts = [
    draftOf('d1', 'application', 'example'),
    draftOf('d2', 'application', 'player'),
    draftOf('d3', 'automation', 'example'),
    draftOf('d4', 'application', null),
  ];

  test('finishes the draft of changes the application has, so the two never change apart', () => {
    const flow = applicationFor(value, 'example', example, drafts);
    expect(flow.resumed?.id).toBe('d1');
    expect(flow).toMatchObject({ kind: 'application', existing: example, opener: { ...value, app: 'example' } });
    // The newest, when a save could not remove an older draft of the same application.
    expect(
      applicationFor(value, 'example', example, [...drafts, draftOf('d9', 'application', 'example')]).resumed?.id,
    ).toBe('d9');
  });

  test('opens the saved application when it has no draft, and a new one finishes none', () => {
    expect(applicationFor(value, 'example', example, [drafts[1]!, drafts[2]!, drafts[3]!]).resumed).toBeNull();
    expect(applicationFor(value, 'example', example, null).resumed).toBeNull();
    expect(applicationFor(value, 'example', null, drafts).resumed).toBeNull();
  });
});

describe('leaving a stack of flows with Save as draft', () => {
  const forValue = valueFor(waits, 'example');
  const idle: Keeping<string> = { kind: 'idle' };
  const valueDraft: Resumed = {
    id: 'v1',
    def: { kind: 'value', app: 'example', changes: null, name: 'Hull', missing: [], pictures: [], state: null },
  };
  const automationDef = automationDraft(automation, { draft: typed, app: 'example' }, []);
  /** The open flow once it knows the draft it saved. */
  const knows = { ...forValue, resumed: valueDraft };

  test('saves the open flow first, then the automation that waits, then leaves', () => {
    const asked = keepStep(idle, { kind: 'asked', target: 'values', here: true }, forValue, []);
    expect(asked).toEqual({
      state: { kind: 'here', flow: forValue, target: 'values' },
      effects: [{ kind: 'saveHere', flow: forValue }],
    });
    const here = keepStep(asked.state, { kind: 'here', flow: forValue, result: valueDraft }, forValue, []);
    expect(here).toEqual({
      state: { kind: 'under', flow: knows, target: 'values', here: true },
      effects: [
        { kind: 'adopt', flow: knows },
        { kind: 'saveUnder', flow: knows, id: null, def: automationDef },
      ],
    });
    // The answers that follow are for the flow that knows its draft, which is the open one from here on.
    const adopted = here.state.kind === 'under' ? here.state.flow : forValue;
    expect(keepStep(here.state, { kind: 'under', flow: adopted, failed: null }, adopted, [])).toEqual({
      state: idle,
      effects: [{ kind: 'leave', target: 'values' }],
    });
  });

  test('saves only what holds work', () => {
    // The open flow holds nothing a draft keeps: only the automation is saved.
    expect(keepStep(idle, { kind: 'asked', target: 'values', here: false }, forValue, [])).toEqual({
      state: { kind: 'under', flow: forValue, target: 'values', here: false },
      effects: [{ kind: 'saveUnder', flow: forValue, id: null, def: automationDef }],
    });
    // No automation waits with work: the open flow's draft is the only one, and leaving follows it.
    const alone = keepStep(idle, { kind: 'asked', target: 'values', here: true }, value, []);
    expect(keepStep(alone.state, { kind: 'here', flow: value, result: valueDraft }, value, [])).toEqual({
      state: idle,
      effects: [{ kind: 'leave', target: 'values' }],
    });
    // An automation that finishes a draft is saved over that draft.
    const resumed: Resumed = { id: 'a7', def: automationDef };
    const finishing = valueFor(waiting({ ...automation, resumed }, { ...typed, cooldown: '9' }), 'example');
    const [effect] = keepStep(idle, { kind: 'asked', target: 'values', here: false }, finishing, []).effects;
    expect(effect).toMatchObject({ kind: 'saveUnder', id: 'a7' });
  });

  test('a first save that fails keeps the stack and says why', () => {
    const asked = keepStep(idle, { kind: 'asked', target: 'values', here: true }, forValue, []);
    expect(
      keepStep(asked.state, { kind: 'here', flow: forValue, result: { failed: 'The disk is full.' } }, forValue, []),
    ).toEqual({ state: idle, effects: [{ kind: 'fail', text: 'The draft could not be saved. The disk is full.' }] });
  });

  test('a second save that fails says the first is saved, and a retry saves over it', () => {
    const under: Keeping<string> = { kind: 'under', flow: knows, target: 'values', here: true };
    expect(keepStep(under, { kind: 'under', flow: knows, failed: 'The disk is full.' }, knows, [])).toEqual({
      state: idle,
      effects: [
        {
          kind: 'fail',
          text: "The automation's draft could not be saved. The disk is full. The draft of what shows here is saved.",
        },
      ],
    });
    // The open flow is the one that knows its draft now, so its saver names that draft's ID.
    const again = keepStep(idle, { kind: 'asked', target: 'values', here: true }, knows, []);
    expect(again.effects).toEqual([{ kind: 'saveHere', flow: knows }]);
    const [retry] = again.effects;
    expect(retry?.kind === 'saveHere' ? retry.flow.resumed?.id : null).toBe('v1');
    // Without a draft of the open flow on the way, the failure names only the automation's.
    const only: Keeping<string> = { ...under, here: false };
    expect(keepStep(only, { kind: 'under', flow: knows, failed: 'No.' }, knows, []).effects).toEqual([
      { kind: 'fail', text: "The automation's draft could not be saved. No." },
    ]);
  });

  test('an answer for a flow that is no longer the open one is dropped', () => {
    const here: Keeping<string> = { kind: 'here', flow: forValue, target: 'values' };
    const under: Keeping<string> = { kind: 'under', flow: knows, target: 'values', here: true };
    // The flow closed while its draft saved: nothing is left, and nothing more is saved.
    expect(keepStep(here, { kind: 'here', flow: forValue, result: valueDraft }, waits, [])).toEqual({
      state: idle,
      effects: [],
    });
    expect(keepStep(under, { kind: 'under', flow: knows, failed: null }, null, [])).toEqual({
      state: idle,
      effects: [],
    });
    // An answer for another flow than the one the saves are for, or of the other save, changes nothing.
    expect(keepStep(under, { kind: 'under', flow: forValue, failed: null }, knows, [])).toEqual({
      state: under,
      effects: [],
    });
    expect(keepStep(under, { kind: 'here', flow: knows, result: valueDraft }, knows, [])).toEqual({
      state: under,
      effects: [],
    });
    expect(keepStep(idle, { kind: 'under', flow: knows, failed: null }, knows, [])).toEqual({
      state: idle,
      effects: [],
    });
  });

  test('a second press while the saves run, or with no flow open, does nothing', () => {
    const here: Keeping<string> = { kind: 'here', flow: forValue, target: 'values' };
    expect(keepStep(here, { kind: 'asked', target: 'settings', here: true }, forValue, [])).toEqual({
      state: here,
      effects: [],
    });
    expect(keepStep(idle, { kind: 'asked', target: 'settings', here: true }, null, [])).toEqual({
      state: idle,
      effects: [],
    });
  });
});
