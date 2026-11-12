// What an automation's condition watches: the application first, then one of its values, and the step still missing.

import type { AppSummary } from './bindings.ts';
import { type Draft, type ValueChoice, findValue } from './automation.ts';

/** A saved application an automation can watch. */
export interface WatchApp {
  readonly id: string;
  readonly name: string;
}

/** The saved applications whose files read, in the order they are listed. */
export function watchApps(apps: readonly AppSummary[]): readonly WatchApp[] {
  return apps.flatMap((app) => (app.file === null ? [] : [{ id: app.id, name: app.file.name }]));
}

/** What the editor knows of what to watch. */
export interface Watching {
  readonly saved: readonly WatchApp[];
  /** Every saved value, of every application. */
  readonly choices: readonly ValueChoice[];
  readonly draft: Draft;
  /** The application chosen before a value of it is picked: by the user, or handed over by a flow that saved it. */
  readonly chosen: string | null;
  /** The value of `chosen` that a flow just saved, until the saved values list it and the draft picks it. */
  readonly made: string | null;
}

/**
 * Where choosing what to watch stands. Only `ready` has a value to compare; `value` and `pick` have the
 * application a new value would belong to.
 */
export type WatchStep =
  | { readonly kind: 'no-applications' }
  | { readonly kind: 'choosing' }
  /** What a flow just saved is not picked yet: the application with the ID `id` is not listed, or its new value. */
  | { readonly kind: 'arriving'; readonly id: string }
  /** The watched value's application, with the ID `id`, is not among the saved ones that read. */
  | { readonly kind: 'missing'; readonly id: string }
  /** The chosen application has no value yet. */
  | { readonly kind: 'value'; readonly app: WatchApp }
  | { readonly kind: 'pick'; readonly app: WatchApp; readonly values: readonly ValueChoice[] }
  | {
      readonly kind: 'ready';
      readonly app: WatchApp;
      readonly values: readonly ValueChoice[];
      readonly choice: ValueChoice;
    };

/**
 * Where choosing what to watch stands. The application is the one chosen, then the watched value's, then the only
 * one saved.
 */
export function watchStep({ saved, choices, draft, chosen, made }: Watching): WatchStep {
  const [only, ...others] = saved;
  const id = chosen ?? draft.source?.app ?? (others.length === 0 ? only?.id : undefined);
  if (id === undefined) {
    return only === undefined ? { kind: 'no-applications' } : { kind: 'choosing' };
  }
  const app = saved.find((each) => each.id === id);
  if (app === undefined) {
    // The watched value's application cannot be read. Any other is one a flow just saved, on its way into the lists.
    return draft.source?.app === id ? { kind: 'missing', id } : { kind: 'arriving', id };
  }
  if (made !== null) {
    return { kind: 'arriving', id };
  }
  const values = choices.filter((choice) => choice.app === id);
  if (values.length === 0) {
    return { kind: 'value', app };
  }
  const choice = draft.source === null ? null : findValue(values, draft.source);
  return choice === null ? { kind: 'pick', app, values } : { kind: 'ready', app, values, choice };
}

/** The value a flow just saved, once the saved values list it for the draft to pick, or null until then. */
export function arrived(watching: Pick<Watching, 'choices' | 'chosen' | 'made'>): ValueChoice | null {
  const { choices, chosen, made } = watching;
  return chosen === null || made === null ? null : findValue(choices, { app: chosen, value: made });
}

/** What the editor says a step needs, and the flow that makes it when the next thing to do is to open one. */
export interface WatchGuide {
  readonly text: string;
  /** The flow to open next: for an application, for a value, or none when a choice in the editor is next. */
  readonly opens: 'application' | 'value' | null;
  /** Whether it is a problem with what is saved rather than a step to take. */
  readonly warn: boolean;
}

/** What the editor says at `step` to lead to the next one, or null once there is a value to compare. */
export function watchGuide(step: WatchStep): WatchGuide | null {
  switch (step.kind) {
    case 'no-applications':
      return {
        text: "First, choose what to watch. An automation watches a value that AppyTappy reads from an application's window, and no application is saved yet. Add the application; you come back here with it chosen.",
        opens: 'application',
        warn: false,
      };
    case 'choosing':
      return {
        text: 'First, choose the application to watch, or add it if it is not in the list.',
        opens: null,
        warn: false,
      };
    case 'arriving':
      return { text: 'Reading what you saved…', opens: null, warn: false };
    case 'missing':
      return {
        text: "The application this automation watches can't be read any more. Choose another, or check the Applications page.",
        opens: null,
        warn: true,
      };
    case 'value':
      return {
        text: `Next, make a value for ${step.app.name}: what AppyTappy reads from its window, such as a number or how full a bar is. You come back here with it picked.`,
        opens: 'value',
        warn: false,
      };
    case 'pick':
      return { text: `Pick the value of ${step.app.name} to watch, or make a new one.`, opens: null, warn: false };
    case 'ready':
      return null;
  }
}

/** The draft once its condition watches the application `app`: without a value of another application. */
export function watching(draft: Draft, app: string | null): Draft {
  return app !== null && draft.source !== null && draft.source.app !== app ? { ...draft, source: null } : draft;
}
