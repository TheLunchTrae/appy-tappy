// What an automation's condition watches: the application first, then one of its values, and the step still missing.

import { type Draft, type ValueChoice, findValue, withSource } from './automation.ts';
import { type PickerApp, startingApp } from './picker.ts';

/**
 * Where choosing what to watch stands. Only `ready` has a value to compare; `value` and `pick` have the
 * application a new value would belong to.
 */
export type WatchStep =
  | { readonly kind: 'no-applications' }
  | { readonly kind: 'choosing' }
  /** The application with the ID `id` is chosen, but it is not among the saved ones that read. */
  | { readonly kind: 'missing'; readonly id: string }
  /** The chosen application has no value yet. */
  | { readonly kind: 'value'; readonly app: PickerApp }
  | { readonly kind: 'pick'; readonly app: PickerApp; readonly values: readonly ValueChoice[] }
  | {
      readonly kind: 'ready';
      readonly app: PickerApp;
      readonly values: readonly ValueChoice[];
      readonly choice: ValueChoice;
    };

/**
 * Where choosing what `draft` watches stands. Its application is the watched value's, then the one `chosen`, then
 * the only one saved.
 */
export function watchStep(
  saved: readonly PickerApp[],
  choices: readonly ValueChoice[],
  draft: Draft,
  chosen: string | null,
): WatchStep {
  const id = startingApp(saved, draft.source?.app ?? null, chosen);
  if (id === null) {
    return saved.length === 0 ? { kind: 'no-applications' } : { kind: 'choosing' };
  }
  const app = saved.find((each) => each.id === id);
  if (app === undefined) {
    return { kind: 'missing', id };
  }
  const values = choices.filter((choice) => choice.app === id);
  if (values.length === 0) {
    return { kind: 'value', app };
  }
  const choice = draft.source === null ? null : findValue(values, draft.source);
  return choice === null ? { kind: 'pick', app, values } : { kind: 'ready', app, values, choice };
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

/**
 * The draft once its condition watches the application `app`: with the value `value` picked when that names a
 * saved one of it, and otherwise without a value of another application. With no `app`, the draft as it is.
 */
export function watching(
  draft: Draft,
  choices: readonly ValueChoice[],
  app: string | null,
  value: string | null,
): Draft {
  if (app === null) {
    return draft;
  }
  const made = value === null ? null : findValue(choices, { app, value });
  if (made !== null) {
    return withSource(draft, made);
  }
  return draft.source !== null && draft.source.app !== app ? { ...draft, source: null } : draft;
}
