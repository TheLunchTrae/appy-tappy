// What an automation means in words, and the editor's draft of one.

import type {
  ActionDef,
  AppSummary,
  AutomationDef,
  AutomationSummary,
  CheckDef,
  Comparison,
  MediaCommand,
  ValueDef,
  WhenDef,
} from './api.ts';

/** What a value reads: a number, or yes or no. */
export type ValueKind = 'number' | 'flag';

/** The kind of value a check reads. */
export function valueKind(check: CheckDef): ValueKind {
  return check.kind === 'number' || check.kind === 'barFill' ? 'number' : 'flag';
}

/** A saved value an automation can watch. */
export interface ValueChoice {
  readonly app: string;
  readonly appName: string;
  readonly value: ValueDef;
  readonly kind: ValueKind;
}

/** Every saved value of the applications whose files read fine, in the order the Values page lists them. */
export function valueChoices(apps: readonly AppSummary[]): readonly ValueChoice[] {
  return apps.flatMap((app) =>
    app.file === null
      ? []
      : app.file.values.map((value) => ({
          app: app.id,
          appName: app.file?.name ?? app.id,
          value,
          kind: valueKind(value.screen.check),
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
export function targetText(target: number | boolean): string {
  if (typeof target === 'boolean') {
    return target ? 'yes' : 'no';
  }
  return String(target);
}

/** What each media command does, as an action's name. */
export const MEDIA_NAMES: Readonly<Record<MediaCommand, string>> = {
  pause: 'Pause media',
  play: 'Play media',
  next: 'Next track',
  previous: 'Previous track',
};

/** An action in a few words, such as `Pause media · Player.exe`. */
export function actionText(action: ActionDef): string {
  if (action.kind === 'media') {
    return `${MEDIA_NAMES[action.command]} · ${action.app ?? 'whatever is playing'}`;
  }
  return `Volume · ${action.app ?? 'system'} ${action.percent}%`;
}

/** Whether undo can put back what `action` changed: a skipped track cannot be unskipped. */
export function undoable(action: ActionDef): boolean {
  return action.kind === 'volume' || action.command === 'play' || action.command === 'pause';
}

/** What undo does for `actions`, as a sentence, or null when none of them can be undone. */
export function undoText(actions: readonly ActionDef[]): string | null {
  const parts = [
    actions.some((action) => action.kind === 'media' && action.command === 'pause') ? 'paused media plays again' : null,
    actions.some((action) => action.kind === 'media' && action.command === 'play')
      ? 'started media pauses again'
      : null,
    actions.some((action) => action.kind === 'volume') ? 'each volume goes back to where it was' : null,
  ].filter((part) => part !== null);
  if (parts.length === 0) {
    return null;
  }
  const joined = parts.length === 1 ? parts.join('') : `${parts.slice(0, -1).join(', ')} and ${parts.at(-1) ?? ''}`;
  return `${joined.charAt(0).toUpperCase()}${joined.slice(1)}.`;
}

/** The names of the automations that watch the value `value` of the application `app`. */
export function usedBy(automations: readonly AutomationSummary[], app: string, value: string): readonly string[] {
  return automations
    .filter(({ automation }) => automation.when.app === app && automation.when.value === value)
    .map(({ automation }) => automation.name);
}

/** Whose media or volume an action changes: the current player or the system, or one application. */
type ActionTarget = 'default' | 'app';

/** An action as the editor holds it, with text fields kept as typed. */
export interface ActionDraft {
  /** Tells the rows apart while they are added, moved and removed. */
  readonly key: number;
  readonly kind: 'media' | 'volume';
  readonly command: MediaCommand;
  readonly percent: string;
  readonly target: ActionTarget;
  /** The application's executable, such as `Player.exe`, when `target` is `app`. */
  readonly app: string;
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
  readonly actions: readonly ActionDraft[];
  readonly undo: boolean;
  /** The cooldown in seconds, as typed. */
  readonly cooldown: string;
}

/** A new action row of `kind`. */
export function newAction(key: number, kind: ActionDraft['kind'] = 'media'): ActionDraft {
  return { key, kind, command: 'pause', percent: '20', target: 'default', app: '' };
}

/** A new automation: pause whatever is playing, and undo it afterwards. */
export const EMPTY_DRAFT: Draft = {
  name: '',
  enabled: true,
  source: null,
  is: 'atLeast',
  number: '',
  flag: true,
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
    actions: automation.actions.map((action, key) => ({
      key,
      kind: action.kind,
      command: action.kind === 'media' ? action.command : 'pause',
      percent: action.kind === 'volume' ? String(action.percent) : '20',
      target: action.app === null ? 'default' : 'app',
      app: action.app ?? '',
    })),
    undo: automation.undo,
    cooldown: String(automation.cooldownMs / 1000),
  };
}

/** The draft with the value `choice` picked, keeping a comparison that suits it. */
export function withSource(draft: Draft, choice: ValueChoice): Draft {
  const is = COMPARISONS[choice.kind].includes(draft.is) ? draft.is : (COMPARISONS[choice.kind][0] ?? 'equals');
  return { ...draft, source: { app: choice.app, value: choice.value.id }, is };
}

/** The longest cooldown, in seconds: a day. */
const MAX_COOLDOWN_SECONDS = 86_400;

/** A plain decimal number, such as `90`, `-3` or `12.5`. */
const DECIMAL = /^-?(?:\d+(?:\.\d*)?|\.\d+)$/u;

/** A number typed into a field, or null when it is not a plain decimal number. */
function typedNumber(text: string): number | null {
  const trimmed = text.trim();
  return DECIMAL.test(trimmed) ? Number(trimmed) : null;
}

function buildAction(draft: ActionDraft, position: number): ActionDef | string {
  const app = draft.target === 'app' ? draft.app.trim() : null;
  if (app === '') {
    return `Name the application for action ${position}, such as Player.exe.`;
  }
  if (draft.kind === 'media') {
    return { kind: 'media', command: draft.command, app };
  }
  const percent = typedNumber(draft.percent);
  if (percent === null || !Number.isInteger(percent) || percent < 0 || percent > 100) {
    return `Set the volume of action ${position} to a whole number from 0 to 100.`;
  }
  return { kind: 'volume', percent, app };
}

/** The automation a draft describes, or the next thing to fill in. */
export type Built =
  | { readonly ok: true; readonly automation: AutomationDef }
  | { readonly ok: false; readonly next: string };

/** Builds the automation `draft` describes, watching a value among `choices`; `id` is the saved one's, or empty. */
export function buildAutomation(draft: Draft, choices: readonly ValueChoice[], id: string): Built {
  const name = draft.name.trim();
  if (name === '') {
    return { ok: false, next: 'Name the automation.' };
  }
  const choice = draft.source === null ? null : findValue(choices, draft.source);
  if (choice === null) {
    return { ok: false, next: 'Pick the value to watch.' };
  }
  let target: number | boolean = draft.flag;
  if (choice.kind === 'number') {
    const number = typedNumber(draft.number);
    if (number === null) {
      return { ok: false, next: `Enter the number ${choice.value.name} is compared with, such as 90 or 12.5.` };
    }
    target = number;
  }
  if (!COMPARISONS[choice.kind].includes(draft.is)) {
    return { ok: false, next: `Pick how ${choice.value.name} is compared.` };
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
  if (draft.undo && !actions.some(undoable)) {
    return { ok: false, next: 'Turn off undo: a skipped track cannot be unskipped.' };
  }
  const cooldown = typedNumber(draft.cooldown);
  if (cooldown === null || cooldown < 0 || cooldown > MAX_COOLDOWN_SECONDS) {
    return { ok: false, next: 'Set the wait before it runs again in seconds, from 0 to 86400.' };
  }
  return {
    ok: true,
    automation: {
      id,
      name,
      enabled: draft.enabled,
      when: { app: choice.app, value: choice.value.id, is: draft.is, target },
      actions,
      undo: draft.undo,
      cooldownMs: Math.round(cooldown * 1000),
    },
  };
}
