// What an automation's condition watches: the application first, then one of its values, and the step still missing.

import type { AppSummary } from './bindings.ts';
import { type Built, type Draft, type ValueChoice, findValue } from './automation.ts';
import type { Handed } from './flow.ts';
import { sentence } from './matching.ts';

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
  /** The saved applications, as the newest read of the saved lists that answered has them. */
  readonly apps: readonly AppSummary[];
  /** Every saved value, of every application, from the same read. */
  readonly choices: readonly ValueChoice[];
  /** The number of that read, or 0 before any answered. */
  readonly listed: number;
  /** Why the newest read of the saved lists failed, or null when it answered. */
  readonly readError: string | null;
  readonly draft: Draft;
  /** The application chosen before a value of it is picked: by the user, or handed over by a flow that saved it. */
  readonly chosen: string | null;
  /** What a flow just saved for `chosen`, until the draft picks its value or the user chooses something else. */
  readonly handed: Handed | null;
}

/**
 * Where choosing what to watch stands. Only `ready` has a value to compare; `value` and `pick` have the
 * application a new value would belong to.
 */
export type WatchStep =
  /** No saved application reads. `unreadable` says some are saved, but none of their files can be read. */
  | { readonly kind: 'no-applications'; readonly unreadable: boolean }
  | { readonly kind: 'choosing' }
  /**
   * A flow just saved the application with the ID `id`, or a value of it, and no read of the saved lists that
   * started since then has answered. `error` is why the newest read failed, or null.
   */
  | { readonly kind: 'arriving'; readonly id: string; readonly error: string | null }
  /**
   * The application with the ID `id` is not among the saved ones that read. `added` says a flow just saved it,
   * and a read since then answered without it.
   */
  | { readonly kind: 'missing'; readonly id: string; readonly added: boolean }
  /**
   * The chosen application has no value. `lost` says a flow just saved one, and a read since then answered
   * without it.
   */
  | { readonly kind: 'value'; readonly app: WatchApp; readonly lost: boolean }
  /** None of the application's values is picked. `lost` is as for `value`. */
  | { readonly kind: 'pick'; readonly app: WatchApp; readonly values: readonly ValueChoice[]; readonly lost: boolean }
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
export function watchStep({ apps, choices, listed, readError, draft, chosen, handed }: Watching): WatchStep {
  const saved = watchApps(apps);
  const [only, ...others] = saved;
  const id = chosen ?? draft.source?.app ?? (others.length === 0 ? only?.id : undefined);
  if (id === undefined) {
    return only === undefined ? { kind: 'no-applications', unreadable: apps.length > 0 } : { kind: 'choosing' };
  }
  // What was handed over is on its way only until a read that started after it answers.
  const awaited = handed !== null && listed <= handed.after;
  const app = saved.find((each) => each.id === id);
  if (app === undefined) {
    return awaited ? { kind: 'arriving', id, error: readError } : { kind: 'missing', id, added: handed !== null };
  }
  const values = choices.filter((choice) => choice.app === id);
  const made = handed?.value ?? null;
  if (made !== null && awaited) {
    return { kind: 'arriving', id, error: readError };
  }
  const lost = made !== null && !values.some((choice) => choice.value.id === made);
  if (values.length === 0) {
    return { kind: 'value', app, lost };
  }
  const choice = draft.source === null ? null : findValue(values, draft.source);
  return choice === null ? { kind: 'pick', app, values, lost } : { kind: 'ready', app, values, choice };
}

/** The value a flow just saved, once the saved values list it for the draft to pick, or null until then. */
export function arrived({
  choices,
  chosen,
  handed,
}: Pick<Watching, 'choices' | 'chosen' | 'handed'>): ValueChoice | null {
  const made = handed?.value ?? null;
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

/**
 * What the editor says at `step` to lead to the next one. Once there is a value to compare it says nothing,
 * unless that value is `picked`, the one a flow just saved, which it names.
 */
export function watchGuide(step: WatchStep, picked: string | null): WatchGuide | null {
  switch (step.kind) {
    case 'no-applications':
      return step.unreadable
        ? {
            text: 'None of the saved applications can be read, so there is nothing to watch yet. Check the Applications page, or add the application again; you come back here with it chosen.',
            opens: 'application',
            warn: true,
          }
        : {
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
      return step.error === null
        ? { text: 'Reading what you saved…', opens: null, warn: false }
        : {
            text: `Couldn't read what you saved: ${sentence(step.error)} AppyTappy tries again every few seconds.`,
            opens: null,
            warn: true,
          };
    case 'missing':
      return {
        text: step.added
          ? "The application you saved isn't among the saved applications: it was removed, or its file can't be read. Choose another, or add it again."
          : "The application this automation watches can't be read any more. Choose another, add it again, or check the Applications page.",
        opens: null,
        warn: true,
      };
    case 'value':
      return step.lost
        ? {
            text: `The value you saved isn't among ${step.app.name}'s values: it was deleted, or ${step.app.name}'s file changed. Make it again.`,
            opens: 'value',
            warn: true,
          }
        : {
            text: `Next, make a value for ${step.app.name}: what AppyTappy reads from its window, such as a number or how full a bar is. You come back here with it picked.`,
            opens: 'value',
            warn: false,
          };
    case 'pick':
      return step.lost
        ? {
            text: `The value you saved isn't among ${step.app.name}'s values: it was deleted, or ${step.app.name}'s file changed. Pick another, or make it again.`,
            opens: null,
            warn: true,
          }
        : { text: `Pick the value of ${step.app.name} to watch, or make a new one.`, opens: null, warn: false };
    case 'ready':
      return step.choice.value.id === picked
        ? { text: `${step.choice.value.name} is picked. Say how it compares.`, opens: null, warn: false }
        : null;
  }
}

/** The draft once its condition watches the application `app`: without a value of another application. */
export function watching(draft: Draft, app: string | null): Draft {
  return app !== null && draft.source !== null && draft.source.app !== app ? { ...draft, source: null } : draft;
}

/**
 * The draft as the editor shows it again for the application `app`. A value that a flow hands over replaces the
 * watched one, so until the draft picks it there is none to save; otherwise only a value of another application
 * goes.
 */
export function returned(draft: Draft, app: string | null, handed: Handed | null): Draft {
  if (handed !== null && handed.value !== null) {
    return { ...draft, source: null };
  }
  return watching(draft, app);
}

/**
 * What the foot of the editor says comes next, and whether the automation can be saved now. While what a flow
 * saved is still read, it says so and nothing is saved, whatever the draft would build.
 */
export function readiness(built: Built, step: WatchStep): { readonly text: string; readonly ready: boolean } {
  if (step.kind === 'arriving') {
    return { text: watchGuide(step, null)?.text ?? '', ready: false };
  }
  return built.ok ? { text: 'Ready to save.', ready: true } : { text: built.next, ready: false };
}
