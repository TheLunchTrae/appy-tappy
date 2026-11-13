// What an automation means in words, and the editor's draft of one.

import {
  type ActionDef,
  ACTIONS,
  type AntiCheatDef,
  type AppSummary,
  type AutomationDef,
  type AutomationSummary,
  type ClickAtDef,
  type ClickButtonDef,
  type ClickDeliveryDef,
  type ComparisonDef,
  LIMITS,
  type Live,
  type MediaCommandDef,
  NO_STATE,
  type Reading,
  type TargetDef,
  type ValueDef,
  type WhenDef,
} from './bindings.ts';
import { CHECKS, type ValueKind } from './check.ts';
import { clickBlocked, clickName } from './click.ts';
import { spellKeys } from './shortcut.ts';
import { asciiLower, hasControl, trimmed, utf8Bytes } from './text.ts';

/** A saved value an automation can watch. */
export interface ValueChoice {
  readonly app: string;
  readonly appName: string;
  /** The anti-cheat class of the value's application. */
  readonly antiCheat: AntiCheatDef;
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
export const COMPARISONS: Readonly<Record<ValueKind, readonly ComparisonDef[]>> = {
  number: ['atLeast', 'above', 'atMost', 'below', 'equals', 'notEquals'],
  flag: ['equals', 'notEquals'],
  state: ['equals', 'notEquals'],
};

/** Each comparison as the condition reads it, after the value's name. */
export const COMPARISON_NAMES: Readonly<Record<ComparisonDef, string>> = {
  below: 'is below',
  atMost: 'is at most',
  equals: 'is',
  notEquals: 'is not',
  atLeast: 'is at least',
  above: 'is above',
};

/** The most decimals a reading shows when that is enough to tell it from the target. */
const READING_DECIMALS = 4;

/**
 * A number as read, such as `87`, `12.46` or `33.3333`: exact up to four
 * decimals, and rounded beyond that only while the rounded number still falls
 * on the same side of `target` as the reading, so it never contradicts
 * whether a condition holds.
 */
function numberText(value: number, target?: number): string {
  for (let decimals = 0; decimals <= READING_DECIMALS; decimals += 1) {
    const shown = value.toFixed(decimals);
    if (Number(shown) === value) {
      return shown;
    }
  }
  const rounded = value.toFixed(READING_DECIMALS);
  if (target !== undefined && Math.sign(Number(rounded) - target) !== Math.sign(value - target)) {
    return String(value);
  }
  return rounded;
}

/**
 * A reading as the UI shows it, such as `87`, `12.46`, `Yes` or `Paused`, never contradicting a comparison with
 * `target`.
 */
export function readingText(reading: Reading, target?: TargetDef): string {
  switch (reading.kind) {
    case 'number':
      return numberText(reading.value, typeof target === 'number' ? target : undefined);
    case 'flag':
      return reading.value ? 'Yes' : 'No';
    case 'state':
      return reading.value;
  }
}

/** A live reading, or an error that stopped one, or null before the first answer arrives. */
export type LiveState = Live | { readonly kind: 'error'; readonly message: string } | null;

/** A live reading in a sentence, such as `Progress reads 87 now, so the condition is true.` */
export function liveText(live: LiveState, choice: ValueChoice, when: WhenDef | null): string {
  const name = choice.value.name;
  if (live === null) {
    return `Looking for ${choice.appName}…`;
  }
  switch (live.kind) {
    case 'waiting':
      return `Waiting for ${choice.appName} to show ${name}…`;
    case 'error':
    case 'problem':
    case 'lost':
      return live.message;
    case 'read': {
      const reads = `${name} reads ${readingText(live.reading, when?.target)} now`;
      if (live.holds === null) {
        return `${reads}.`;
      }
      return live.holds ? `${reads}, so the condition is true.` : `${reads}, so the condition is not true.`;
    }
  }
}

/**
 * What a screen reader announces of a live reading: only whether the
 * condition holds, or what is wrong, so a moving number is not announced
 * twice a second.
 */
export function liveAnnouncement(live: LiveState): string {
  if (live === null || live.kind === 'waiting') {
    return '';
  }
  if (live.kind === 'read') {
    if (live.holds === null) {
      return '';
    }
    return live.holds ? 'The condition is true.' : 'The condition is not true.';
  }
  return live.message;
}

/** A target as the condition reads it. */
export function targetText(target: TargetDef): string {
  if (typeof target === 'boolean') {
    return target ? 'yes' : 'no';
  }
  return String(target);
}

/** What each media command does, as an action's name. */
export const MEDIA_NAMES: Readonly<Record<MediaCommandDef, string>> = {
  pause: 'Pause media',
  play: 'Play media',
  next: 'Next track',
  previous: 'Previous track',
};

/** The last part of a Windows path, such as `Ding.wav` for `C:\Sounds\Ding.wav`. */
function fileName(path: string): string {
  return path.split(/[\\/]/u).at(-1) ?? path;
}

/**
 * An action in a few words, such as `Pause media · Player.exe`; `appName` names a saved application by its ID, for a
 * click.
 */
export function actionText(action: ActionDef, appName: (id: string) => string = (id) => id): string {
  switch (action.kind) {
    case 'media':
      return `${MEDIA_NAMES[action.command]} · ${action.app ?? 'whatever is playing'}`;
    case 'volume':
      return `Volume · ${action.app ?? 'system'} ${action.percent}%`;
    case 'keys':
      return `Press ${action.keys}`;
    case 'open':
      return `Open ${action.target}`;
    case 'sound':
      return `Play ${fileName(action.file)}`;
    case 'click':
      return `${clickName(action.button)} · ${appName(action.app)}`;
  }
}

/**
 * Whether undo can put back what `action` changed, as the runtime decides: a skipped track cannot be unskipped,
 * pressed keys cannot be unpressed, and what was opened or played has nothing to put back.
 */
export function undoable(action: ActionDef): boolean {
  return ACTIONS.undoable[action.kind === 'media' ? (`media.${action.command}` as const) : action.kind];
}

/**
 * Why `actions` cannot run for an application whose anti-cheat is `antiCheat`, or null when they can: keys
 * are pressed only for an application without anti-cheat, as the runtime also checks. A click sends input too, but
 * {@link clicksBlocked} says why it cannot run, since it also depends on the application it clicks in.
 */
export function keysBlocked(
  actions: readonly { readonly kind: ActionDef['kind'] }[],
  antiCheat: AntiCheatDef,
  appName: string,
): string | null {
  if (antiCheat === 'none' || !actions.some((action) => action.kind !== 'click' && ACTIONS.injectsInput[action.kind])) {
    return null;
  }
  return `Anti-cheat can treat pressed keys as cheating, so keys are pressed only once ${appName}'s anti-cheat is set to None on the Applications page.`;
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
  readonly kind: ActionDef['kind'];
  readonly command: MediaCommandDef;
  readonly percent: string;
  readonly target: ActionTarget;
  /** The application's executable, such as `Player.exe`, when `target` is `app`. */
  readonly app: string;
  /** The keys to press, such as `Ctrl+Shift+M`. */
  readonly keys: string;
  /** What an open action opens, as typed: a full path or a web link. */
  readonly opens: string;
  /** The WAV file a sound action plays, as typed. */
  readonly sound: string;
  /** The ID of the saved application a click clicks in, or empty before one is picked. */
  readonly clickApp: string;
  readonly button: ClickButtonDef;
  /** How a click reaches its window. */
  readonly delivery: ClickDeliveryDef;
  /** Whether a click with the cursor gives the focus back to the window that was in front. */
  readonly returnFocus: boolean;
  /** Where a click lands, or null before its target is set up. */
  readonly at: ClickAtDef | null;
}

/** An automation as the editor holds it. */
export interface Draft {
  readonly name: string;
  readonly enabled: boolean;
  /** The watched value's application and ID, or null before one is picked. */
  readonly source: { readonly app: string; readonly value: string } | null;
  readonly is: ComparisonDef;
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

/** A new action row of `kind`. */
export function newAction(key: number, kind: ActionDraft['kind'] = 'media'): ActionDraft {
  return {
    key,
    kind,
    command: 'pause',
    percent: '20',
    target: 'default',
    app: '',
    keys: '',
    opens: '',
    sound: '',
    clickApp: '',
    button: 'left',
    delivery: 'cursor',
    returnFocus: true,
    at: null,
  };
}

/** A key no row of `actions` has, for a row added to them. */
export function unusedKey(actions: readonly ActionDraft[]): number {
  return actions.reduce((next, action) => Math.max(next, action.key + 1), 0);
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
    actions: automation.actions.map((action, key) => ({
      key,
      kind: action.kind,
      command: action.kind === 'media' ? action.command : 'pause',
      percent: action.kind === 'volume' ? String(action.percent) : '20',
      target: (action.kind === 'media' || action.kind === 'volume') && action.app !== null ? 'app' : 'default',
      app: action.kind === 'media' || action.kind === 'volume' ? (action.app ?? '') : '',
      keys: action.kind === 'keys' ? action.keys : '',
      opens: action.kind === 'open' ? action.target : '',
      sound: action.kind === 'sound' ? action.file : '',
      clickApp: action.kind === 'click' ? action.app : '',
      button: action.kind === 'click' ? action.button : 'left',
      delivery: action.kind === 'click' ? action.delivery : 'cursor',
      returnFocus: action.kind === 'click' ? action.returnFocus : true,
      at: action.kind === 'click' ? action.at : null,
    })),
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

/**
 * A plain decimal number, such as `90`, `-3`, `12.5` or `1,234`: as a number
 * check reads one, `.` is the decimal point and `,` only separates groups of
 * three digits.
 */
const DECIMAL = /^-?(?:\d{1,3}(?:,\d{3})+(?:\.\d*)?|\d+(?:\.\d*)?|\.\d+)$/u;

/** A number typed into a field, or null when it is not a plain decimal number or too large to hold. */
function typedNumber(text: string): number | null {
  const typed = text.trim();
  const number = DECIMAL.test(typed) ? Number(typed.replaceAll(',', '')) : Number.NaN;
  return Number.isFinite(number) ? number : null;
}

/**
 * A path or link as typed, without spaces around it or the quotes that File Explorer's Copy as path puts around
 * a path.
 */
function typedPath(text: string): string {
  const path = trimmed(text);
  return path.length >= 2 && path.startsWith('"') && path.endsWith('"') ? trimmed(path.slice(1, -1)) : path;
}

/**
 * Whether `text` is a full Windows path on a drive, such as `C:\Tools\Timer.exe`. A relative one would depend on a
 * folder the user never sees, and a network path, such as `\\server\share\file`, is refused, as the runtime
 * refuses it.
 */
function isFullPath(text: string): boolean {
  return /^[A-Za-z]:[\\/]/u.test(text);
}

/** Whether `text` fits in an open or sound action: not empty, not too long, and no control characters. */
function fitsTarget(text: string): boolean {
  return text !== '' && !hasControl(text) && utf8Bytes(text) <= LIMITS.maxTargetBytes;
}

/** Why the path or link `text` of action `position` can't be saved whatever its kind, or `null` when it can be. */
function targetProblem(text: string, position: number): string | null {
  if (hasControl(text)) {
    return `The path or link in action ${position} has a tab or line break in it. Remove it.`;
  }
  if (utf8Bytes(text) > LIMITS.maxTargetBytes) {
    return `The path or link in action ${position} is too long to save. Shorten it.`;
  }
  return null;
}

/** Whether `text` is a web link: `http` or `https` only, as the runtime opens no other kind. */
function isWebLink(text: string): boolean {
  return /^https?:\/\/\P{White_Space}+$/u.test(asciiLower(text));
}

/**
 * What an open action keeps of `text`, as the Rust side reads it: a full path or a web link without the spaces
 * around it, or null when it is neither.
 */
export function openTarget(text: string): string | null {
  const target = trimmed(text);
  return fitsTarget(target) && (isFullPath(target) || isWebLink(target)) ? target : null;
}

/** What a sound action keeps of `text`, as the Rust side reads it: a WAV file's full path, or null. */
export function soundFile(text: string): string | null {
  const file = trimmed(text);
  return fitsTarget(file) && isFullPath(file) && asciiLower(file).endsWith('.wav') ? file : null;
}

/** `text` as a media or volume action's app name, as the Rust side reads it, or null when it can't be one. */
export function appName(text: string): string | null {
  return text !== '' && !hasControl(text) && utf8Bytes(text) <= LIMITS.maxActionAppBytes ? text : null;
}

function buildAction(draft: ActionDraft, position: number): ActionDef | string {
  if (draft.kind === 'click') {
    if (draft.clickApp === '') {
      return `Pick the application action ${position} clicks in.`;
    }
    if (draft.at === null) {
      return `Set up where action ${position} clicks.`;
    }
    return {
      kind: 'click',
      app: draft.clickApp,
      button: draft.button,
      delivery: draft.delivery,
      returnFocus: draft.returnFocus,
      at: draft.at,
    };
  }
  if (draft.kind === 'open') {
    const target = typedPath(draft.opens);
    if (target === '') {
      return `Enter what action ${position} opens: a program's full path, such as C:\\Tools\\Timer.exe, or a web link.`;
    }
    const problem = targetProblem(target, position);
    if (problem !== null) {
      return problem;
    }
    const opens = openTarget(target);
    return opens !== null
      ? { kind: 'open', target: opens }
      : `Action ${position} opens a program or file by its full path, such as C:\\Tools\\Timer.exe, or a link that starts with http:// or https://.`;
  }
  if (draft.kind === 'sound') {
    const file = typedPath(draft.sound);
    if (file === '') {
      return `Enter the full path of the WAV file action ${position} plays, such as C:\\Sounds\\Ding.wav.`;
    }
    const problem = targetProblem(file, position);
    if (problem !== null) {
      return problem;
    }
    const plays = soundFile(file);
    return plays !== null
      ? { kind: 'sound', file: plays }
      : `Action ${position} plays a WAV file by its full path, such as C:\\Sounds\\Ding.wav.`;
  }
  if (draft.kind === 'keys') {
    if (trimmed(draft.keys) === '') {
      return `Choose the keys for action ${position} to press.`;
    }
    const keys = spellKeys(draft.keys);
    return keys === null
      ? `Spell the keys for action ${position} as any of Ctrl, Alt, Shift and Win, then one key such as M, 5, F9, Space or Enter, joined by +, such as Ctrl+Shift+M.`
      : { kind: 'keys', keys };
  }
  const app = draft.target === 'app' ? trimmed(draft.app) : null;
  if (app === '') {
    return `Name the application for action ${position}, such as Player.exe.`;
  }
  if (app !== null && hasControl(app)) {
    return `The application name in action ${position} has a tab or line break in it. Remove it.`;
  }
  if (app !== null && appName(app) === null) {
    return `The application name in action ${position} is too long to save. Shorten it.`;
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
 * Why clicks in the applications `clickedIn`, by their IDs, cannot run for the value `choice`, or null when they
 * can: anti-cheat can see a click, so the watched application and each one clicked in need none.
 */
export function clicksBlocked(
  clickedIn: readonly string[],
  choice: ValueChoice,
  apps: readonly AppSummary[],
): string | null {
  if (clickedIn.length === 0) {
    return null;
  }
  for (const id of clickedIn) {
    const clicked = apps.find((app) => app.id === id)?.file ?? null;
    const blocked = clickBlocked(
      { name: choice.appName, antiCheat: choice.antiCheat },
      clicked === null ? null : { name: clicked.name, antiCheat: clicked.antiCheat },
    );
    if (blocked !== null) {
      return blocked;
    }
  }
  return null;
}

/**
 * Builds the automation `draft` describes, watching a value among `choices`; `id` is the saved one's, or empty.
 * `apps` are the saved applications a click may click in.
 */
export function buildAutomation(
  draft: Draft,
  choices: readonly ValueChoice[],
  id: string,
  apps: readonly AppSummary[] = [],
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
  if (draft.undo && !actions.some(undoable)) {
    const has = (kind: ActionDef['kind']): boolean => actions.some((action) => action.kind === kind);
    const why = [
      has('media') ? 'a skipped track cannot be unskipped' : null,
      has('keys') ? 'pressed keys cannot be unpressed' : null,
      has('open') ? 'what opened stays open' : null,
      has('sound') ? 'a played sound cannot be unplayed' : null,
      has('click') ? 'a click cannot be undone' : null,
    ].filter((part) => part !== null);
    // Each part is a clause, so a comma comes before the last one's `and`.
    const last = why.pop() ?? '';
    return {
      ok: false,
      next: `Turn off undo: ${[why.join(', '), last].filter((part) => part !== '').join(', and ')}.`,
    };
  }
  const blocked =
    keysBlocked(actions, choice.antiCheat, choice.appName) ??
    clicksBlocked(
      actions.flatMap((action) => (action.kind === 'click' ? [action.app] : [])),
      choice,
      apps,
    );
  if (blocked !== null) {
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
