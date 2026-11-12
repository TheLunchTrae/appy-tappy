// The flows the window shows in place of a page: how one opens another, waits under it, and shows again when it ends.

import type { AppSummary, AutomationDef, ReferenceDef, ValueDef } from './bindings.ts';
import { type Draft, startingDraft } from './automation.ts';
import { type Held, held } from './unsaved.ts';

/** A saved value to draw again at the window's size now, and its application's ID. */
export interface Fixing {
  readonly app: string;
  readonly value: ValueDef;
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
  /** The value of `app` that the value flow saved, which the condition picks once the saved values list it. */
  readonly value: string | null;
}

/** Making a value, or fixing a saved one at another window size. It opens the application flow from its picker. */
export interface ValueFlow {
  readonly kind: 'value';
  readonly fixing: Fixing | null;
  /** The application the picker starts with, or null to let the user choose. */
  readonly app: string | null;
  readonly opener: AutomationFlow | null;
}

/** Making or changing a reference. It opens the application flow from its picker. */
export interface ReferenceFlow {
  readonly kind: 'reference';
  readonly existing: SavedRef | null;
  /** The application the picker starts with, or null to let the user choose. */
  readonly app: string | null;
}

/** A flow that opens another flow and waits under it. */
export type Opener = AutomationFlow | ValueFlow | ReferenceFlow;

/** Adding or changing an application. */
export interface ApplicationFlow {
  readonly kind: 'application';
  readonly existing: AppSummary | null;
  readonly opener: Opener | null;
}

/**
 * What the window shows in place of a page: a flow that makes or changes something. A flow opened for another keeps
 * that one as its `opener`, which waits under it and shows again when it ends. At most three are stacked: an
 * automation, a value for it, and an application for that value.
 */
export type Flow = AutomationFlow | ValueFlow | ReferenceFlow | ApplicationFlow;

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
 * The automation flow as it waits under a flow it opens: with the `draft` it shows again, and no value to pick
 * yet.
 */
export function waiting(flow: AutomationFlow, draft: Draft): AutomationFlow {
  return { ...flow, draft, value: null };
}

/**
 * The value flow opened for the automation flow `opener`, starting with `app`, the application its condition has
 * chosen. The opener keeps that application, so it shows again as the user left it.
 */
export function valueFor(opener: AutomationFlow, app: string): ValueFlow {
  return { kind: 'value', fixing: null, app, opener: { ...opener, app } };
}

/**
 * The application flow opened for `opener`, to add an application or to change `existing`. The opener keeps
 * `chosen`, the application it shows chosen, so it shows again as the user left it.
 */
export function applicationFor(opener: Opener, chosen: string | null, existing: AppSummary | null): ApplicationFlow {
  return { kind: 'application', existing, opener: { ...opener, app: chosen } };
}

/**
 * How a flow ended: it saved, with what it made for a flow that opened it or null when it made nothing a flow
 * starts with, or the user canceled it.
 */
export type Ended = { readonly kind: 'saved'; readonly made: Made | null } | { readonly kind: 'canceled' };

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
 * and for an automation the saved value to pick, or as it was left when the flow was canceled.
 */
export function afterFlow(flow: Flow, ended: Ended, open: boolean): After {
  if (!open) {
    return { kind: 'nothing' };
  }
  const opener = openerOf(flow);
  if (opener === null) {
    return { kind: ended.kind === 'saved' ? 'listed' : 'page' };
  }
  if (ended.kind === 'canceled' || ended.made === null) {
    return { kind: 'flow', flow: opener };
  }
  const { app, value } = ended.made;
  return { kind: 'flow', flow: opener.kind === 'automation' ? { ...opener, app, value } : { ...opener, app } };
}

/** The page each kind of flow opens from. */
const PAGE_NAMES: Readonly<Record<Flow['kind'], string>> = {
  automation: 'Automations',
  value: 'Values',
  reference: 'References',
  application: 'Applications',
};

/** A waiting flow as a breadcrumb names it. */
function crumbName(flow: Opener): string {
  switch (flow.kind) {
    case 'automation':
      return flow.existing?.name ?? 'New automation';
    case 'value':
      return flow.fixing === null ? 'New value' : `Fix ${flow.fixing.value.name}`;
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
 * Whether the waiting flow `opener` holds work that leaving would lose. Only an automation's draft can: the value
 * and reference flows open another flow from their picker, before a capture holds anything.
 */
function holdsWork(opener: Opener): boolean {
  return (
    opener.kind === 'automation' &&
    opener.draft !== null &&
    held(false, startingDraft(opener.existing), opener.draft) === 'unsaved'
  );
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
  reference: 'The screenshots, the labels and any recording are lost.',
  automation: 'The changes to this automation are lost.',
  application: 'The changes to this application are lost.',
};

/**
 * What the question before leaving `flow` says: what the open flow loses when it `reported` work, and the changes
 * to an automation that waits under it.
 */
export function lostText(flow: Flow, reported: Held): string {
  const here = reported === 'nothing' ? null : LOST[flow.kind];
  const under = waitingUnder(flow).some(holdsWork)
    ? `The unsaved changes to the automation are lost${here === null ? '' : ' too'}.`
    : null;
  return ['Leave without saving?', here, under].filter((part) => part !== null).join(' ');
}
