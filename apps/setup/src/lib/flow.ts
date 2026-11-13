// The flows the window shows in place of a page: how one opens another, waits under it, and shows again when it ends,
// and how a flow is saved as a draft and opened again from one.

import type {
  AppSummary,
  AutomationDef,
  AutomationSummary,
  DraftDef,
  DraftSummary,
  ReferenceDef,
  ValueDef,
} from './bindings.ts';
import { type Draft, draftPictures, startingDraft, unfinished, valueChoices } from './automation.ts';
import { type Kept, type Resumed, type Saved, draftOfSaved, isKept, keep } from './drafts.ts';
import { type Held, held } from './unsaved.ts';

/** A saved value to change, and its application's ID. */
export interface Changing {
  readonly app: string;
  readonly value: ValueDef;
  /** Whether it is drawn again at the window's size now, as a fix, or edited as it was first drawn. */
  readonly how: 'fix' | 'edit';
}

/** A saved reference and the application it belongs to. */
export interface SavedRef {
  readonly app: string;
  readonly reference: ReferenceDef;
}

/** Making or changing an automation. It opens the value flow and the application flow for its condition. */
export interface AutomationFlow {
  readonly kind: 'automation';
  /** The saved automation to change, or null for a new one. */
  readonly existing: AutomationDef | null;
  /** The draft as the editor left it to open another flow, or null to start from `existing`. */
  readonly draft: Draft | null;
  /** The application the condition starts with: the one chosen before another flow opened, or the one that flow saved. */
  readonly app: string | null;
  /** What a flow the editor opened just saved for `app`, or null when none did. */
  readonly handed: Handed | null;
  /** The saved draft the flow finishes, or null. */
  readonly resumed: Resumed | null;
}

/**
 * What an automation's flow holds of its automation: the editor's draft and the application its condition has
 * chosen. A waiting flow carries it under the flow it opened, and a saved draft of an automation is this, written
 * down.
 */
export interface AutomationSetup {
  readonly draft: Draft;
  readonly app: string | null;
}

/** What a draft of an automation keeps. */
type AutomationKept = Kept<'automation', AutomationSetup>;

/** What a flow saved for the automation that opened it, as the automation's editor takes it. */
export interface Handed {
  /** The value of the application that the value flow saved, which the condition picks, or null for an application. */
  readonly value: string | null;
  /**
   * The number of the newest read of the saved lists that started before the save was heard. Any later read
   * started after the save, so its answer has what was saved, or shows that it is gone.
   */
  readonly after: number;
}

/**
 * Making a value, fixing a saved one at another window size, or editing a saved one. It opens the application flow
 * from its picker. Only a new value is made for an automation: a fix and an edit open from the Values page alone.
 */
export type ValueFlow =
  | {
      readonly kind: 'value';
      readonly changing: null;
      /** The application the picker starts with, or null to let the user choose. */
      readonly app: string | null;
      readonly opener: AutomationFlow | null;
      /** The saved draft the flow finishes, or null. A value made for an automation finishes none. */
      readonly resumed: Resumed | null;
    }
  | {
      readonly kind: 'value';
      readonly changing: Changing;
      /** The application the picker showed chosen when it opened the application flow, which is the value's own. */
      readonly app: string | null;
      readonly opener: null;
      /** The saved draft of a fix the flow finishes, or null. An edit is never a draft. */
      readonly resumed: Resumed | null;
    };

/** Making or changing a reference. It opens the application flow from its picker. */
export interface RefFlow {
  readonly kind: 'reference';
  readonly existing: SavedRef | null;
  /** The application the picker starts with, or null to let the user choose. */
  readonly app: string | null;
  /** The saved draft the flow finishes, or null. */
  readonly resumed: Resumed | null;
}

/** A flow that opens another flow and waits under it. */
export type Opener = AutomationFlow | ValueFlow | RefFlow;

/** Adding or changing an application. */
export interface ApplicationFlow {
  readonly kind: 'application';
  readonly existing: AppSummary | null;
  readonly opener: Opener | null;
  /** The saved draft the flow finishes, or null. */
  readonly resumed: Resumed | null;
}

/**
 * What the window shows in place of a page: a flow that makes or changes something. A flow opened for another keeps
 * that one as its `opener`, which waits under it and shows again when it ends. At most three are stacked: an
 * automation, a value for it, and an application for that value.
 */
export type Flow = AutomationFlow | ValueFlow | RefFlow | ApplicationFlow;

/**
 * What a flow that saved made for the flow that opened it: the application it belongs to, and the value when it
 * saved one.
 */
export interface Made {
  readonly app: string;
  readonly value: string | null;
}

function openerOf(flow: Flow): Opener | null {
  return 'opener' in flow ? flow.opener : null;
}

/** The flows that wait under `flow`, the one opened first at the start. */
function waitingUnder(flow: Flow): readonly Opener[] {
  const opener = openerOf(flow);
  return opener === null ? [] : [...waitingUnder(opener), opener];
}

/**
 * Whether an automation is being made or changed: its flow is the one open, or waits under a flow it opened. Until
 * then its draft, on screen or waiting, can name click pictures that no saved automation names yet.
 */
export function holdsAutomation(flow: Flow | null): boolean {
  return (
    flow !== null && (flow.kind === 'automation' || waitingUnder(flow).some((under) => under.kind === 'automation'))
  );
}

/**
 * The automation flow as it waits under a flow it opens: with the `draft` it shows again, and nothing handed to
 * it yet.
 */
export function waiting(flow: AutomationFlow, draft: Draft): AutomationFlow {
  return { ...flow, draft, handed: null };
}

/**
 * The value flow opened for the automation flow `opener`, starting with `app`, the application its condition has
 * chosen. The opener keeps that application, so it shows again as the user left it.
 */
export function valueFor(opener: AutomationFlow, app: string): ValueFlow {
  return { kind: 'value', changing: null, app, opener: { ...opener, app }, resumed: null };
}

/**
 * The application flow opened for `opener`, to add an application or to change `existing`. The opener keeps
 * `chosen`, the application it shows chosen, so it shows again as the user left it. When `existing` has a draft of
 * changes among `drafts`, the flow finishes that draft, as **Finish draft** on the Applications page does, so the
 * saved application and its draft never change apart.
 */
export function applicationFor(
  opener: Opener,
  chosen: string | null,
  existing: AppSummary | null,
  drafts: readonly DraftSummary[] | null,
): ApplicationFlow {
  return {
    kind: 'application',
    existing,
    opener: { ...opener, app: chosen },
    resumed: existing === null ? null : draftOfSaved(drafts, 'application', existing.id),
  };
}

/**
 * How a flow ended: it saved, it was saved as a draft or its draft was deleted, or the user canceled it. A save has
 * what it made for a flow that opened it, or null when it made nothing a flow starts with, and the number of the
 * newest read of the saved lists that started before the save was heard.
 */
export type Ended =
  | { readonly kind: 'saved'; readonly made: Made | null; readonly read: number }
  | { readonly kind: 'drafted' }
  | { readonly kind: 'canceled' };

/**
 * What the window shows once a flow ended: nothing else than it shows now, the flow that opened it, the page that
 * lists what it saved, or the page under it.
 */
export type After =
  | { readonly kind: 'nothing' }
  | { readonly kind: 'flow'; readonly flow: Opener }
  | { readonly kind: 'listed' }
  | { readonly kind: 'page' };

/**
 * What follows `flow`. `open` says the flow is still the one the window shows: a save that finishes after the user
 * left it for another changes nothing. A flow that waits under it shows again: with the saved application chosen,
 * and for an automation what was handed to it, or as it was left when the flow was canceled. A flow whose draft
 * was saved or deleted made nothing for the flow that waits, which shows again as it was left; with none waiting,
 * the page that lists the draft shows.
 */
export function afterFlow(flow: Flow, ended: Ended, open: boolean): After {
  if (!open) {
    return { kind: 'nothing' };
  }
  const opener = openerOf(flow);
  if (opener === null) {
    return { kind: ended.kind === 'canceled' ? 'page' : 'listed' };
  }
  if (ended.kind !== 'saved' || ended.made === null) {
    return { kind: 'flow', flow: opener };
  }
  const { app, value } = ended.made;
  return {
    kind: 'flow',
    flow: opener.kind === 'automation' ? { ...opener, app, handed: { value, after: ended.read } } : { ...opener, app },
  };
}

/** The page each kind of flow opens from. */
const PAGE_NAMES: Readonly<Record<Flow['kind'], string>> = {
  automation: 'Automations',
  value: 'Values',
  reference: 'References',
  application: 'Applications',
};

/** A value flow as a breadcrumb names it: a new value, or the fix or the edit of a saved one. */
function changeName(changing: Changing | null): string {
  if (changing === null) {
    return 'New value';
  }
  return `${changing.how === 'fix' ? 'Fix' : 'Edit'} ${changing.value.name}`;
}

/** A waiting flow as a breadcrumb names it. */
function crumbName(flow: Opener): string {
  switch (flow.kind) {
    case 'automation':
      return flow.existing?.name ?? 'New automation';
    case 'value':
      return changeName(flow.changing);
    case 'reference':
      return flow.existing?.reference.name ?? 'New reference';
  }
}

/**
 * The way to `flow`, for its breadcrumb: the page the first flow opened from, then each flow that waits under
 * `flow`, such as Automations, New automation for a value made for a new automation.
 */
export function trail(flow: Flow): readonly string[] {
  const under = waitingUnder(flow);
  return [PAGE_NAMES[(under[0] ?? flow).kind], ...under.map(crumbName)];
}

/**
 * Where a new value of the application named `app` shows once it is saved, as its capture says before the save:
 * `way` is the way to the value's flow, which is longer than its page when an automation waits for the value.
 */
export function savedNote(way: readonly string[], app: string): string {
  const listed = `on the Values page, under ${app}`;
  return way.length > 1
    ? `Saving returns to the automation with this value picked. The value also shows ${listed}.`
    : `Saved values show ${listed}.`;
}

/** What the automation draft `def` set up, or null when another version saved it. */
export function keptAutomation(def: DraftDef): AutomationSetup | null {
  return isKept<AutomationKept>(def.state, 'automation') ? def.state.state : null;
}

/**
 * What the automation flow `flow` starts with, which leaving it loses nothing of: what the draft it finishes kept,
 * or else the saved automation it changes, or a new one.
 */
export function startOf(flow: Pick<AutomationFlow, 'existing' | 'resumed'>): AutomationSetup {
  const kept = flow.resumed === null ? null : keptAutomation(flow.resumed.def);
  return kept ?? { draft: startingDraft(flow.existing), app: null };
}

/**
 * The automation flow `flow` as a draft keeps it, with `setup` as the editor has it now. `apps` are the saved
 * applications, which say what the automation still needs.
 */
export function automationDraft(
  flow: Pick<AutomationFlow, 'existing'>,
  setup: AutomationSetup,
  apps: readonly AppSummary[],
): DraftDef {
  const typed = setup.draft.name.trim();
  return {
    kind: 'automation',
    app: null,
    changes: flow.existing?.id ?? null,
    name: typed === '' ? (flow.existing?.name ?? 'New automation') : typed,
    missing: [...unfinished(setup.draft, valueChoices(apps), apps)],
    pictures: [...draftPictures(setup.draft)],
    state: keep('automation', setup),
  };
}

/**
 * The flow that finishes the draft `resumed`, on the saved item it changes when that is still there, or null when
 * the value it fixes is gone. `apps` and `automations` are the saved lists.
 */
export function resumedFlow(
  resumed: Resumed,
  apps: readonly AppSummary[],
  automations: readonly AutomationSummary[],
): Flow | null {
  const { def } = resumed;
  const changed = def.changes;
  const file = apps.find((app) => app.id === def.app)?.file ?? null;
  switch (def.kind) {
    case 'automation': {
      const existing = automations.find((summary) => summary.automation.id === changed)?.automation ?? null;
      return {
        kind: 'automation',
        existing,
        draft: null,
        app: startOf({ existing, resumed }).app,
        handed: null,
        resumed,
      };
    }
    case 'application':
      return { kind: 'application', existing: apps.find((app) => app.id === changed) ?? null, opener: null, resumed };
    case 'value':
      return { kind: 'value', changing: null, app: def.app, opener: null, resumed };
    case 'fix': {
      const value = file?.values.find((each) => each.id === changed);
      return def.app === null || value === undefined
        ? null
        : { kind: 'value', changing: { app: def.app, value, how: 'fix' }, app: null, opener: null, resumed };
    }
    case 'reference': {
      const reference = file?.references.find((each) => each.id === changed);
      return {
        kind: 'reference',
        existing: def.app === null || reference === undefined ? null : { app: def.app, reference },
        app: def.app,
        resumed,
      };
    }
  }
}

/**
 * Whether the waiting flow `opener` holds work that leaving would lose. Only an automation's draft can: the value
 * and reference flows open another flow from their picker, before a capture holds anything.
 */
function holdsWork(opener: Opener): opener is AutomationFlow & { readonly draft: Draft } {
  return (
    opener.kind === 'automation' &&
    opener.draft !== null &&
    held(false, startOf(opener).draft, opener.draft) === 'unsaved'
  );
}

/**
 * The automation that waits under `flow` with work that leaving would lose, as a draft keeps it, with the ID of the
 * draft it replaces, or null when none waits with work. Leaving with **Save as draft** saves it beside the open
 * flow's own draft: each flow that holds work becomes a draft of its own, and the automation's is finished later
 * without the flow it had opened.
 */
export function draftUnder(
  flow: Flow,
  apps: readonly AppSummary[],
): { readonly id: string | null; readonly def: DraftDef } | null {
  const under = waitingUnder(flow).find(holdsWork);
  return under === undefined
    ? null
    : {
        id: under.resumed?.id ?? null,
        def: automationDraft(under, { draft: under.draft, app: under.app }, apps),
      };
}

/**
 * What leaving `flow` would lose, counting the flows that wait under it; `reported` is what the open flow says it
 * holds. A save under way comes first.
 */
export function heldInAll(flow: Flow | null, reported: Held): Held {
  if (flow === null || reported !== 'nothing') {
    return reported;
  }
  return waitingUnder(flow).some(holdsWork) ? 'unsaved' : 'nothing';
}

/** What leaving each kind of flow before it is saved loses. */
const LOST: Readonly<Record<Flow['kind'], string>> = {
  value: 'The screenshots, the region and any recording are lost.',
  reference: 'The screenshots, the boxes and any recording are lost.',
  automation: 'The changes to this automation are lost.',
  application: 'The changes to this application are lost.',
};

/** What leaving an edit of a saved value before it is saved loses: the value itself stays as it is saved. */
const EDIT_LOST = 'The screenshots and the changes to this value are lost.';

/** Whether `flow` edits a saved value in place, which is never saved as a draft. */
function isEdit(flow: Flow): boolean {
  return flow.kind === 'value' && flow.changing?.how === 'edit';
}

/** What a draft calls the item `flow` makes or changes. */
function draftNoun(flow: Flow): string {
  return flow.kind === 'value' && flow.changing !== null ? 'fix' : flow.kind;
}

/** The question before leaving a flow: what it says, and whether it offers to save a draft. */
export interface LeaveQuestion {
  readonly text: string;
  /** Whether **Save as draft** is an answer: the open flow, an automation that waits under it, or both are kept. */
  readonly drafts: boolean;
}

/**
 * What the question before leaving `flow` says and offers: what the open flow loses when it `reported` work, the
 * changes to an automation that waits under it, and what **Save as draft** keeps of the two. `savable` says the open
 * flow can save its draft. Whatever a draft would keep is named, so the question never calls it lost for good; a
 * recording and an edit of a saved value are never kept, and the question says so.
 */
export function leaveQuestion(flow: Flow, reported: Held, savable: boolean): LeaveQuestion {
  const here = reported === 'nothing' ? null : isEdit(flow) ? EDIT_LOST : LOST[flow.kind];
  const waits = waitingUnder(flow).some(holdsWork);
  const under = waits ? `The unsaved changes to the automation are lost${here === null ? '' : ' too'}.` : null;
  const keepsHere = here !== null && savable && !isEdit(flow);
  const kept = [keepsHere ? `this ${draftNoun(flow)}` : null, waits ? 'the automation' : null].filter(
    (part) => part !== null,
  );
  // A capture's draft keeps its screenshots but never a recording.
  const recording = keepsHere && (flow.kind === 'value' || flow.kind === 'reference') ? ', without a recording' : '';
  const offer =
    kept.length === 0
      ? null
      : `Save as draft keeps ${kept.length > 1 ? 'both, each as its own draft' : kept.join('')}${recording}.`;
  const refused = here !== null && isEdit(flow) ? 'An edit cannot be saved as a draft.' : null;
  return {
    text: ['Leave without saving?', here, under, offer, refused].filter((part) => part !== null).join(' '),
    drafts: kept.length > 0,
  };
}

/**
 * Leaving a stack of flows with **Save as draft**: nothing under way, the open flow's own draft saving, or the draft
 * of the automation that waits under it saving. `flow` is the open flow the saves are for, and `here` says the open
 * flow's own draft was saved on the way.
 */
export type Keeping<Page> =
  | { readonly kind: 'idle' }
  | { readonly kind: 'here'; readonly flow: Flow; readonly target: Page }
  | { readonly kind: 'under'; readonly flow: Flow; readonly target: Page; readonly here: boolean };

/** What happens while leaving with drafts: the answer is pressed, or one of the two saves answers for `flow`. */
export type KeepEvent<Page> =
  | { readonly kind: 'asked'; readonly target: Page; readonly here: boolean }
  | { readonly kind: 'here'; readonly flow: Flow; readonly result: Saved }
  | { readonly kind: 'under'; readonly flow: Flow; readonly failed: string | null };

/**
 * What a step asks of the window: save the open flow's draft, save the waiting automation's, show `flow` in place of
 * the open one because it now knows the draft it saved, show why a save failed, or leave for the page.
 */
type KeepEffect<Page> =
  | { readonly kind: 'saveHere'; readonly flow: Flow }
  | { readonly kind: 'saveUnder'; readonly flow: Flow; readonly id: string | null; readonly def: DraftDef }
  | { readonly kind: 'adopt'; readonly flow: Flow }
  | { readonly kind: 'fail'; readonly text: string }
  | { readonly kind: 'leave'; readonly target: Page };

/** A step's outcome: the state that follows and what to do. */
export interface KeepOutcome<Page> {
  readonly state: Keeping<Page>;
  readonly effects: readonly KeepEffect<Page>[];
}

/**
 * One step of leaving with **Save as draft**. `open` is the flow the window shows now, and `apps` the saved
 * applications. The open flow's draft is saved first, when `here` says it holds work that a draft keeps, and then
 * the draft of the automation that waits under it. The open flow comes to know the draft it saved (`resumed`), so
 * when the automation's save fails and the user tries again, or saves the flow's item after all, the same draft is
 * saved over or finished. An answer for a flow that is no longer the open one, or that no save is waiting for, is
 * dropped.
 */
export function keepStep<Page>(
  state: Keeping<Page>,
  event: KeepEvent<Page>,
  open: Flow | null,
  apps: readonly AppSummary[],
): KeepOutcome<Page> {
  const idle: Keeping<Page> = { kind: 'idle' };
  const under = (flow: Flow, target: Page, here: boolean, first: readonly KeepEffect<Page>[]): KeepOutcome<Page> => {
    const waits = draftUnder(flow, apps);
    return waits === null
      ? { state: idle, effects: [{ kind: 'leave', target }] }
      : {
          state: { kind: 'under', flow, target, here },
          effects: [...first, { kind: 'saveUnder', flow, id: waits.id, def: waits.def }],
        };
  };
  if (event.kind === 'asked') {
    if (state.kind !== 'idle' || open === null) {
      return { state, effects: [] };
    }
    return event.here
      ? { state: { kind: 'here', flow: open, target: event.target }, effects: [{ kind: 'saveHere', flow: open }] }
      : under(open, event.target, false, []);
  }
  if (state.kind !== event.kind || state.flow !== event.flow) {
    return { state, effects: [] };
  }
  if (open !== state.flow) {
    // The flow the saves were for is gone, so there is nothing to leave and no one to tell.
    return { state: idle, effects: [] };
  }
  if (event.kind === 'here') {
    if ('failed' in event.result) {
      return { state: idle, effects: [{ kind: 'fail', text: `The draft could not be saved. ${event.result.failed}` }] };
    }
    const knows: Flow = { ...state.flow, resumed: event.result };
    return under(knows, state.target, true, [{ kind: 'adopt', flow: knows }]);
  }
  if (event.failed !== null) {
    const here = state.kind === 'under' && state.here ? ' The draft of what shows here is saved.' : '';
    return {
      state: idle,
      effects: [{ kind: 'fail', text: `The automation's draft could not be saved. ${event.failed}${here}` }],
    };
  }
  return { state: idle, effects: [{ kind: 'leave', target: state.target }] };
}
