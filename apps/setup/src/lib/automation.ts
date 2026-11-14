// The editor's draft of an automation: the values it can watch, its condition, and what it still needs before it
// can be saved.

import { type ActionDraft, buildAction, draftedAction, inputBlocked, newAction, undoRefused } from './actions.ts';
import {
  type ActionDef,
  type AntiCheatClass,
  type AppSummary,
  type AutomationDef,
  type AutomationSummary,
  type Comparison,
  LIMITS,
  NO_STATE,
  type TargetDef,
  type ValueDef,
  type WhenDef,
} from './bindings.ts';
import { CHECKS, type ValueKind } from './check.ts';
import { typedNumber } from './text.ts';

/** A saved value an automation can watch. */
export interface ValueChoice {
  readonly app: string;
  readonly appName: string;
  /** The anti-cheat class of the value's application. */
  readonly antiCheat: AntiCheatClass;
  readonly value: ValueDef;
  readonly kind: ValueKind;
  /** The names of the states a state check reads, in its order; none for another kind. */
  readonly states: readonly string[];
}

/** Every saved value of the applications whose files read fine, in the order the Values page lists them. */
export function valueChoices(apps: readonly AppSummary[]): readonly ValueChoice[] {
  return apps.flatMap((app) =>
    app.file === null
      ? []
      : app.file.values.map((value) => ({
          app: app.id,
          appName: app.file?.name ?? app.id,
          antiCheat: app.file?.antiCheat ?? 'unknown',
          value,
          kind: CHECKS[value.screen.check.kind].reads,
          states: value.screen.check.kind === 'state' ? value.screen.check.states.map((state) => state.name) : [],
        })),
  );
}

/** The value `when` watches, or null when it is not saved. */
export function findValue(choices: readonly ValueChoice[], when: Pick<WhenDef, 'app' | 'value'>): ValueChoice | null {
  return choices.find((choice) => choice.app === when.app && choice.value.id === when.value) ?? null;
}

/** The comparisons each kind of value offers, the usual one first. */
export const COMPARISONS: Readonly<Record<ValueKind, readonly Comparison[]>> = {
  number: ['atLeast', 'above', 'atMost', 'below', 'equals', 'notEquals'],
  flag: ['equals', 'notEquals'],
  state: ['equals', 'notEquals'],
};

/** Each comparison as the condition reads it, after the value's name. */
export const COMPARISON_NAMES: Readonly<Record<Comparison, string>> = {
  below: 'is below',
  atMost: 'is at most',
  equals: 'is',
  notEquals: 'is not',
  atLeast: 'is at least',
  above: 'is above',
};

/** A target as the condition reads it. */
export function targetText(target: TargetDef): string {
  if (typeof target === 'boolean') {
    return target ? 'yes' : 'no';
  }
  return String(target);
}

/** The names of the automations that watch the value `value` of the application `app`. */
export function usedBy(automations: readonly AutomationSummary[], app: string, value: string): readonly string[] {
  return automations
    .filter(({ automation }) => automation.when.app === app && automation.when.value === value)
    .map(({ automation }) => automation.name);
}

/** An automation as the editor holds it. */
export interface Draft {
  readonly name: string;
  readonly enabled: boolean;
  /** The watched value's application and ID, or null before one is picked. */
  readonly source: { readonly app: string; readonly value: string } | null;
  readonly is: Comparison;
  /** The number to compare with, as typed. */
  readonly number: string;
  /** The yes or no to compare with. */
  readonly flag: boolean;
  /** The state to compare with, or empty before one is picked. */
  readonly state: string;
  readonly actions: readonly ActionDraft[];
  readonly undo: boolean;
  /** The cooldown in seconds, as typed. */
  readonly cooldown: string;
}

/** A new automation: pause whatever is playing, and undo it afterwards. */
export const EMPTY_DRAFT: Draft = {
  name: '',
  enabled: true,
  source: null,
  is: 'atLeast',
  number: '',
  flag: true,
  state: '',
  actions: [newAction(0)],
  undo: true,
  cooldown: '0',
};

/** The editor's draft of a saved automation. */
export function toDraft(automation: AutomationDef): Draft {
  const { when } = automation;
  return {
    name: automation.name,
    enabled: automation.enabled,
    source: { app: when.app, value: when.value },
    is: when.is,
    number: typeof when.target === 'number' ? String(when.target) : '',
    flag: typeof when.target === 'boolean' ? when.target : true,
    state: typeof when.target === 'string' ? when.target : '',
    actions: automation.actions.map(draftedAction),
    undo: automation.undo,
    cooldown: String(automation.cooldownMs / 1000),
  };
}

/** The draft the editor starts with: the saved automation's, or a new one. */
export function startingDraft(existing: AutomationDef | null): Draft {
  return existing === null ? EMPTY_DRAFT : toDraft(existing);
}

/** Whether the state check `choice` reads the state `name`: one of its own, or none. */
export function readsState(choice: ValueChoice, name: string): boolean {
  return choice.kind === 'state' && (name === NO_STATE || choice.states.includes(name));
}

/**
 * The draft with the value `choice` picked, keeping a comparison that suits it, and for a state check a state it
 * reads, its first unless it reads the one picked.
 */
export function withSource(draft: Draft, choice: ValueChoice): Draft {
  const is = COMPARISONS[choice.kind].includes(draft.is) ? draft.is : (COMPARISONS[choice.kind][0] ?? 'equals');
  const state = readsState(choice, draft.state) ? draft.state : (choice.states[0] ?? '');
  return { ...draft, source: { app: choice.app, value: choice.value.id }, is, state };
}

/** Joins words with commas and a last `or`, such as `Paused, Live, or none`. */
const OR_LIST = new Intl.ListFormat('en', { type: 'disjunction' });

/** What `choice` reads, in a sentence, such as `Status reads a state: Paused, Live, or none.` */
export function readsText(choice: ValueChoice): string {
  const name = choice.value.name;
  switch (choice.kind) {
    case 'number':
      return `${name} reads a number.`;
    case 'flag':
      return `${name} reads yes or no.`;
    case 'state':
      return `${name} reads a state: ${OR_LIST.format([...choice.states, NO_STATE])}.`;
  }
}

/** The longest cooldown, in seconds. */
const MAX_COOLDOWN_SECONDS = LIMITS.maxCooldownMs / 1000;

/** The condition `draft` describes on the value `choice`, or the next thing to fill in. */
export function buildWhen(draft: Draft, choice: ValueChoice): WhenDef | string {
  let target: TargetDef = draft.flag;
  if (choice.kind === 'number') {
    const number = typedNumber(draft.number);
    if (number === null) {
      return `Enter the number ${choice.value.name} is compared with, such as 90 or 12.5.`;
    }
    target = number;
  }
  if (choice.kind === 'state') {
    if (!readsState(choice, draft.state)) {
      return `Pick the state ${choice.value.name} is compared with.`;
    }
    target = draft.state;
  }
  if (!COMPARISONS[choice.kind].includes(draft.is)) {
    return `Pick how ${choice.value.name} is compared.`;
  }
  return { app: choice.app, value: choice.value.id, is: draft.is, target };
}

/** The automation a draft describes, or the next thing to fill in. */
export type Built =
  | { readonly ok: true; readonly automation: AutomationDef }
  | { readonly ok: false; readonly next: string };

/**
 * Builds the automation `draft` describes, watching a value among `choices`; `id` is the saved one's, or empty.
 * `apps` are the saved applications a click may click in.
 */
export function buildAutomation(
  draft: Draft,
  choices: readonly ValueChoice[],
  id: string,
  apps: readonly AppSummary[],
): Built {
  const name = draft.name.trim();
  if (name === '') {
    return { ok: false, next: 'Name the automation.' };
  }
  const choice = draft.source === null ? null : findValue(choices, draft.source);
  if (choice === null) {
    return { ok: false, next: 'Pick the value to watch.' };
  }
  const when = buildWhen(draft, choice);
  if (typeof when === 'string') {
    return { ok: false, next: when };
  }
  if (draft.actions.length === 0) {
    return { ok: false, next: 'Add an action.' };
  }
  const actions: ActionDef[] = [];
  for (const [index, action] of draft.actions.entries()) {
    const built = buildAction(action, index + 1);
    if (typeof built === 'string') {
      return { ok: false, next: built };
    }
    actions.push(built);
  }
  const refused = draft.undo ? undoRefused(actions) : null;
  if (refused !== null) {
    return { ok: false, next: refused };
  }
  const [blocked] = inputBlocked(draft.actions, { name: choice.appName, antiCheat: choice.antiCheat }, apps);
  if (blocked !== undefined) {
    return { ok: false, next: blocked };
  }
  for (const [index, action] of actions.entries()) {
    if (action.kind === 'click' && !apps.some((app) => app.id === action.app && app.file !== null)) {
      return { ok: false, next: `The application action ${index + 1} clicks in is not saved. Pick another.` };
    }
  }
  const cooldown = typedNumber(draft.cooldown);
  if (cooldown === null || cooldown < 0 || cooldown > MAX_COOLDOWN_SECONDS) {
    return { ok: false, next: `Set the wait before it runs again in seconds, from 0 to ${MAX_COOLDOWN_SECONDS}.` };
  }
  return {
    ok: true,
    automation: {
      id,
      name,
      enabled: draft.enabled,
      when,
      actions,
      undo: draft.undo,
      cooldownMs: Math.round(cooldown * 1000),
    },
  };
}

/**
 * What `draft` still needs before it can be saved, a sentence each: a name, a value and an action, or else the
 * first thing {@link buildAutomation} asks for. Empty once it can be saved.
 */
export function unfinished(
  draft: Draft,
  choices: readonly ValueChoice[],
  apps: readonly AppSummary[],
): readonly string[] {
  const missing = [
    draft.name.trim() === '' ? 'Name the automation.' : null,
    draft.source === null || findValue(choices, draft.source) === null ? 'Pick the value to watch.' : null,
    draft.actions.length === 0 ? 'Add an action.' : null,
  ].filter((part) => part !== null);
  if (missing.length > 0) {
    return missing;
  }
  const built = buildAutomation(draft, choices, '', apps);
  return built.ok ? [] : [built.next];
}

/** The click pictures `draft`'s actions click on, which a draft of it keeps. */
export function draftPictures(draft: Draft): readonly string[] {
  return draft.actions.flatMap((action) =>
    action.kind === 'click' && action.at?.kind === 'picture' ? [action.at.image] : [],
  );
}
