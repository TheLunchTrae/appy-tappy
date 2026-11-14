// The actions an automation runs: the editor's row of one, and what the page knows of each kind.

import {
  type ActionDef,
  ACTIONS,
  type AppSummary,
  type ClickAtDef,
  type ClickButtonDef,
  type ClickDeliveryDef,
  LIMITS,
  type MediaCommand,
} from './bindings.ts';
import { type Guarded, clickBlocked, keysBlocked } from './antiCheat.ts';
import { clickApps, clickName } from './click.ts';
import {
  appName,
  hasControl,
  openTarget,
  soundFile,
  spellKeys,
  trimmed,
  typedNumber,
  typedPath,
  utf8Bytes,
} from './text.ts';

/** The kinds of action, as the saved files name them. */
export type ActionKind = ActionDef['kind'];

/** A saved action of one kind. */
type SavedAction<Kind extends ActionKind> = Extract<ActionDef, { readonly kind: Kind }>;

/** Whose media or volume an action changes: the current player or the system, or one application. */
type ActionTarget = 'default' | 'app';

/**
 * An action as the editor holds it, with text fields kept as typed. A row holds every kind's fields, so what was
 * typed for one kind is still there after the row was another kind for a while.
 */
export interface ActionDraft {
  /** Tells the rows apart while they are added, moved and removed. */
  readonly key: number;
  readonly kind: ActionKind;
  readonly command: MediaCommand;
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

/** A new action row of `kind`. */
export function newAction(key: number, kind: ActionKind = 'media'): ActionDraft {
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

/** The keys of `table` in its order, with their type, which `Object.keys` leaves as `string`. */
function keysOf<Key extends string>(table: Readonly<Record<Key, unknown>>): readonly Key[] {
  return Object.keys(table).filter((key): key is Key => key in table);
}

/** What each media command does, as an action's name, in the order the action menu offers them. */
const MEDIA_NAMES: Readonly<Record<MediaCommand, string>> = {
  pause: 'Pause media',
  play: 'Play media',
  next: 'Next track',
  previous: 'Previous track',
};

/** One choice of the action menu. */
export interface ActionChoice {
  readonly label: string;
  /** What picking it sets on a row, which keeps everything else. */
  readonly picks: { readonly kind: ActionKind; readonly command?: MediaCommand };
}

/** The last part of a Windows path, such as `Ding.wav` for `C:\Sounds\Ding.wav`. */
function fileName(path: string): string {
  return path.split(/[\\/]/u).at(-1) ?? path;
}

/** What a row holds of the application a saved media or volume action names, or of none. */
function draftedApp(app: string | null): Partial<ActionDraft> {
  return app === null ? {} : { target: 'app', app };
}

/**
 * The application the media or volume row `draft` names, null for whatever is playing or the whole system, or what
 * to fix in action `position`.
 */
function namedApp(draft: ActionDraft, position: number): { readonly app: string | null } | string {
  if (draft.target !== 'app') {
    return { app: null };
  }
  const app = trimmed(draft.app);
  if (app === '') {
    return `Name the application for action ${position}, such as Player.exe.`;
  }
  if (hasControl(app)) {
    return `The application name in action ${position} has a tab or line break in it. Remove it.`;
  }
  return appName(app) === null
    ? `The application name in action ${position} is too long to save. Shorten it.`
    : { app };
}

/**
 * The path or link the row typed as `text` keeps, as `read` reads it, or what to fix in action `position`: `empty`
 * when nothing is typed, and `refused` when `read` refuses it.
 */
function typedTarget(
  text: string,
  position: number,
  read: (text: string) => string | null,
  words: { readonly empty: string; readonly refused: string },
): { readonly kept: string } | string {
  const typed = typedPath(text);
  if (typed === '') {
    return words.empty;
  }
  if (hasControl(typed)) {
    return `The path or link in action ${position} has a tab or line break in it. Remove it.`;
  }
  if (utf8Bytes(typed) > LIMITS.maxTargetBytes) {
    return `The path or link in action ${position} is too long to save. Shorten it.`;
  }
  const kept = read(typed);
  return kept === null ? words.refused : { kept };
}

/** Everything the setup app knows of one kind of action. A new kind is one more entry of {@link ACTION_KINDS}. */
interface ActionEntry<Kind extends ActionKind> {
  /** The kind's choices in the action menu. */
  readonly choices: readonly ActionChoice[];
  /** What a row of the saved `action` holds beyond a new row of its kind. */
  readonly drafted: (action: SavedAction<Kind>) => Partial<ActionDraft>;
  /** The action the row `draft` describes, the automation's action `position`, or the next thing it needs. */
  readonly build: (draft: ActionDraft, position: number) => SavedAction<Kind> | string;
  /** A saved action in a few words; `named` names a saved application by its ID. */
  readonly text: (action: SavedAction<Kind>, named: (id: string) => string) => string;
}

/**
 * What anti-cheat has to say of a kind: a kind that sends input, as the runtime's `ACTIONS.injectsInput` says, says
 * why the row `draft` cannot run for the application `watched`, or null when it can. No other kind says anything.
 */
type InputPolicy<Kind extends ActionKind> = (typeof ACTIONS.injectsInput)[Kind] extends true
  ? { readonly blocked: (draft: ActionDraft, watched: Guarded, apps: readonly AppSummary[]) => string | null }
  : { readonly blocked?: never };

/**
 * Each kind of action, in the order the action menu offers them. The keys are the saved files' kinds, so a kind the
 * Rust side adds does not compile until it has an entry here, and one that sends input until it says what anti-cheat
 * blocks. A kind's fields are the automation editor's, and what undo does about it is in {@link UNDO_WORDS}.
 */
const ACTION_KINDS: { readonly [Kind in ActionKind]: ActionEntry<Kind> & InputPolicy<Kind> } = {
  media: {
    choices: keysOf(MEDIA_NAMES).map((command) => ({ label: MEDIA_NAMES[command], picks: { kind: 'media', command } })),
    drafted: (action) => ({ command: action.command, ...draftedApp(action.app) }),
    build: (draft, position) => {
      const named = namedApp(draft, position);
      return typeof named === 'string' ? named : { kind: 'media', command: draft.command, app: named.app };
    },
    text: (action) => `${MEDIA_NAMES[action.command]} · ${action.app ?? 'whatever is playing'}`,
  },
  volume: {
    choices: [{ label: 'Set volume', picks: { kind: 'volume' } }],
    drafted: (action) => ({ percent: String(action.percent), ...draftedApp(action.app) }),
    build: (draft, position) => {
      const named = namedApp(draft, position);
      if (typeof named === 'string') {
        return named;
      }
      const percent = typedNumber(draft.percent);
      return percent === null || !Number.isInteger(percent) || percent < 0 || percent > 100
        ? `Set the volume of action ${position} to a whole number from 0 to 100.`
        : { kind: 'volume', percent, app: named.app };
    },
    text: (action) => `Volume · ${action.app ?? 'system'} ${action.percent}%`,
  },
  keys: {
    choices: [{ label: 'Press keys', picks: { kind: 'keys' } }],
    drafted: (action) => ({ keys: action.keys }),
    build: (draft, position) => {
      if (trimmed(draft.keys) === '') {
        return `Choose the keys for action ${position} to press.`;
      }
      const keys = spellKeys(draft.keys);
      return keys === null
        ? `Spell the keys for action ${position} as any of Ctrl, Alt, Shift and Win, then one key such as M, 5, F9, Space or Enter, joined by +, such as Ctrl+Shift+M.`
        : { kind: 'keys', keys };
    },
    text: (action) => `Press ${action.keys}`,
    blocked: (_, watched) => keysBlocked(watched),
  },
  open: {
    choices: [{ label: 'Open a program, file, or link', picks: { kind: 'open' } }],
    drafted: (action) => ({ opens: action.target }),
    build: (draft, position) => {
      const target = typedTarget(draft.opens, position, openTarget, {
        empty: `Enter what action ${position} opens: a program's full path, such as C:\\Tools\\Timer.exe, or a web link.`,
        refused: `Action ${position} opens a program or file by its full path, such as C:\\Tools\\Timer.exe, or a link that starts with http:// or https://.`,
      });
      return typeof target === 'string' ? target : { kind: 'open', target: target.kept };
    },
    text: (action) => `Open ${action.target}`,
  },
  sound: {
    choices: [{ label: 'Play a sound', picks: { kind: 'sound' } }],
    drafted: (action) => ({ sound: action.file }),
    build: (draft, position) => {
      const file = typedTarget(draft.sound, position, soundFile, {
        empty: `Enter the full path of the WAV file action ${position} plays, such as C:\\Sounds\\Ding.wav.`,
        refused: `Action ${position} plays a WAV file by its full path, such as C:\\Sounds\\Ding.wav.`,
      });
      return typeof file === 'string' ? file : { kind: 'sound', file: file.kept };
    },
    text: (action) => `Play ${fileName(action.file)}`,
  },
  click: {
    choices: [{ label: 'Click in another application', picks: { kind: 'click' } }],
    drafted: ({ app, button, delivery, returnFocus, at }) => ({ clickApp: app, button, delivery, returnFocus, at }),
    build: ({ clickApp, button, delivery, returnFocus, at }, position) => {
      if (clickApp === '') {
        return `Pick the application action ${position} clicks in.`;
      }
      return at === null
        ? `Set up where action ${position} clicks.`
        : { kind: 'click', app: clickApp, button, delivery, returnFocus, at };
    },
    text: (action, named) => `${clickName(action.button)} · ${named(action.app)}`,
    // A click is blocked by the application it clicks in too, so a row says nothing before one is picked.
    blocked: (draft, watched, apps) =>
      draft.clickApp === ''
        ? null
        : clickBlocked(watched, clickApps(apps).find((app) => app.id === draft.clickApp) ?? null),
  },
};

/** The action menu's choices, in its order. */
export const ACTION_CHOICES: readonly ActionChoice[] = keysOf(ACTION_KINDS).flatMap(
  (kind) => ACTION_KINDS[kind].choices,
);

/** Whether the row `action` is what `choice` picks, so the action menu shows that choice for it. */
export function isPicked(choice: ActionChoice, action: ActionDraft): boolean {
  return choice.picks.kind === action.kind && (choice.picks.command ?? action.command) === action.command;
}

/**
 * Hands the saved `action` to `use` with its kind's entry: the one place a saved action meets its entry, since the
 * compiler cannot tell which entry an action of any kind indexes.
 */
function described<Result>(
  action: ActionDef,
  use: <Kind extends ActionKind>(entry: ActionEntry<Kind>, action: SavedAction<Kind>) => Result,
): Result {
  switch (action.kind) {
    case 'media':
      return use(ACTION_KINDS.media, action);
    case 'volume':
      return use(ACTION_KINDS.volume, action);
    case 'keys':
      return use(ACTION_KINDS.keys, action);
    case 'open':
      return use(ACTION_KINDS.open, action);
    case 'sound':
      return use(ACTION_KINDS.sound, action);
    case 'click':
      return use(ACTION_KINDS.click, action);
  }
}

/** An action in a few words, such as `Pause media · Player.exe`; `named` names a saved application by its ID. */
export function actionText(action: ActionDef, named: (id: string) => string): string {
  return described(action, (entry, saved) => entry.text(saved, named));
}

/** The editor's row of the saved `action`: a new row of its kind, with what the action says. */
export function draftedAction(action: ActionDef, key: number): ActionDraft {
  return described(action, (entry, saved) => ({ ...newAction(key, saved.kind), ...entry.drafted(saved) }));
}

/** The action the row `draft` describes, the automation's action `position`, or the next thing to fill in. */
export function buildAction(draft: ActionDraft, position: number): ActionDef | string {
  return ACTION_KINDS[draft.kind].build(draft, position);
}

/**
 * Why anti-cheat keeps `rows` from running for the application `watched`, a sentence for each kind of action it
 * blocks, in the menu's order; empty when it blocks none. `apps` are the saved applications a click may click in.
 */
export function inputBlocked(
  rows: readonly ActionDraft[],
  watched: Guarded,
  apps: readonly AppSummary[],
): readonly string[] {
  return keysOf(ACTION_KINDS).flatMap((kind) => {
    const { blocked } = ACTION_KINDS[kind];
    return blocked === undefined
      ? []
      : rows
          .flatMap((row) => (row.kind === kind ? [blocked(row, watched, apps)] : []))
          .filter((why) => why !== null)
          .slice(0, 1);
  });
}

/** What the runtime's `ACTIONS.undoable` tells apart: a kind of action, or a media action's command. */
type UndoKey = keyof typeof ACTIONS.undoable;

/**
 * What undo does about each action, as a clause: what it puts back where `ACTIONS.undoable` says it can, and why it
 * cannot where it says not. Sentences list the clauses in this order.
 */
const UNDO_WORDS: Readonly<Record<UndoKey, string>> = {
  'media.pause': 'paused media plays again',
  'media.play': 'started media pauses again',
  'media.next': 'a skipped track cannot be unskipped',
  'media.previous': 'a skipped track cannot be unskipped',
  volume: 'each volume goes back to where it was',
  keys: 'pressed keys cannot be unpressed',
  open: 'what opened stays open',
  sound: 'a played sound cannot be unplayed',
  click: 'a click cannot be undone',
};

function undoKey(action: ActionDef): UndoKey {
  return action.kind === 'media' ? (`media.${action.command}` as const) : action.kind;
}

/** The clauses of {@link UNDO_WORDS} for `keys`, each once: those undo can put back, or those it cannot. */
function undoClauses(keys: readonly UndoKey[], undoable: boolean): readonly string[] {
  return [
    ...new Set(
      keysOf(UNDO_WORDS)
        .filter((key) => keys.includes(key) && ACTIONS.undoable[key] === undoable)
        .map((key) => UNDO_WORDS[key]),
    ),
  ];
}

/** `clauses` as a sentence's list of what undo puts back, such as `a, b and c`. */
function putBack(clauses: readonly string[]): string {
  return clauses.length < 2 ? clauses.join('') : `${clauses.slice(0, -1).join(', ')} and ${clauses.at(-1) ?? ''}`;
}

/** What undo does for `actions`, as a sentence, or null when none of them can be undone. */
export function undoText(actions: readonly ActionDef[]): string | null {
  const joined = putBack(undoClauses(actions.map(undoKey), true));
  return joined === '' ? null : `${joined.charAt(0).toUpperCase()}${joined.slice(1)}.`;
}

/** Everything undo can put back, as a sentence, for an automation whose actions are not all filled in. */
export const UNDO_PUTS_BACK = `Undo puts back what it can: ${putBack(undoClauses(keysOf(UNDO_WORDS), true))}.`;

/** Why undo has to be turned off for `actions`, as a sentence, or null when undo can put one of them back. */
export function undoRefused(actions: readonly ActionDef[]): string | null {
  const keys = actions.map(undoKey);
  if (keys.some((key) => ACTIONS.undoable[key])) {
    return null;
  }
  const why = [...undoClauses(keys, false)];
  // Each part is a clause, so a comma comes before the last one's `and`.
  const last = why.pop() ?? '';
  return `Turn off undo: ${[why.join(', '), last].filter((part) => part !== '').join(', and ')}.`;
}
