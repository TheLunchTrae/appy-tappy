// The check the user is setting up, and when it is complete enough to try.

import {
  type CheckDef,
  type CheckSpec,
  LIMITS,
  type RecordedLabel,
  type RecordingSettings,
  type ReferenceDef,
  type ReferenceRef,
  type SelectStep,
  type StateSpec,
} from './bindings.ts';
import { AUTOMATIC, type PinChoice } from './pin.ts';
import type { Color } from './pixels.ts';
import { type Rect, type Size, sameRect } from './region.ts';
import { DEFAULT_COLOR_MATCH, withStep } from './selection.ts';

/** The characters a reference is taught, which the capture screen tries and the reference editor saves. */
type CharactersSpec = Extract<CheckSpec, { readonly kind: 'characters' }>;

/** The kinds of check, in the order the capture screen offers them. */
export const CHECK_KINDS = ['number', 'barFill', 'color', 'state'] as const;

export type CheckKind = (typeof CHECK_KINDS)[number];

/** What each kind is called on screen. */
export const KIND_NAMES: Readonly<Record<CheckKind, string>> = {
  number: 'Number',
  barFill: 'Bar fill',
  color: 'Color',
  state: 'State',
};

/** The tolerance each kind starts with; a number's is its new reference's. */
export const DEFAULT_TOLERANCE: Readonly<Record<CheckKind, number>> = {
  number: 40,
  barFill: 0,
  color: 24,
  state: 16,
};

/** One look of a state check, as the user sets it up. */
export interface DraftState {
  /** The name it reads as, as typed. */
  readonly name: string;
  /** The screenshot it is taken from. Each state has its own. */
  readonly screenshot: number;
  /** How the user selected the thing on its screenshot; none compares the whole region. */
  readonly steps: readonly SelectStep[];
}

/** Everything the capture screen collects for a check or a reference, complete or not. */
export interface Draft {
  readonly kind: CheckKind;
  readonly region: Rect | null;
  /** The screenshot the region was drawn on, whose window size the region's pixels refer to. */
  readonly drawnOn: number | null;
  /** How a new value's pin is picked. It lives beside the region, since a region drawn anew changes both. */
  readonly pin: PinChoice;
  readonly tolerance: number;
  /** The reference's characters' color. */
  readonly ink: Color | null;
  /** What the region shows on each labeled screenshot, by screenshot index. */
  readonly labels: ReadonlyMap<number, string>;
  /** What each reading kept from the recording shows, by reading index. */
  readonly recorded: ReadonlyMap<number, string>;
  readonly filled: Color | null;
  readonly empty: Color | null;
  readonly color: Color | null;
  /** A state check's looks, in the order they are listed. */
  readonly states: readonly DraftState[];
  /** The names new states take first, in order: a saved value's, when it is drawn again at another size. */
  readonly stateNames: readonly string[];
  /** How far a color may be from another and still count as alike, while selecting. */
  readonly colorMatch: number;
  /** Whether a state check compares only the selected pixels of each state. */
  readonly ignoreBackground: boolean;
  /** The ID of the reference a number check reads with. */
  readonly reference: string | null;
  /** Whether a number check ignores every digit after the decimal point. */
  readonly whole: boolean;
}

export const EMPTY_DRAFT: Draft = {
  kind: 'number',
  region: null,
  drawnOn: null,
  pin: AUTOMATIC,
  tolerance: DEFAULT_TOLERANCE.number,
  ink: null,
  labels: new Map(),
  recorded: new Map(),
  filled: null,
  empty: null,
  color: null,
  states: [],
  stateNames: [],
  reference: null,
  colorMatch: DEFAULT_COLOR_MATCH,
  ignoreBackground: true,
  whole: false,
};

/** Where the region is: its rectangle and the screenshot it was drawn on. */
export type Drawn = Pick<Draft, 'region' | 'drawnOn'>;

/**
 * Whether `draft` would record other than the stopped recording's `settings`: another region, characters' color or
 * tolerance, or a region drawn on a screenshot whose window size `drawnSize` is not the recording's.
 */
export function settingsDiffer(draft: Draft, settings: RecordingSettings, drawnSize: Size | null): boolean {
  return (
    draft.tolerance !== settings.tolerance ||
    !sameRect(draft.region, settings.region) ||
    draft.ink?.red !== settings.ink.red ||
    draft.ink.green !== settings.ink.green ||
    draft.ink.blue !== settings.ink.blue ||
    drawnSize?.width !== settings.windowSize.width ||
    drawnSize.height !== settings.windowSize.height
  );
}

/**
 * The newest of `screenshots`, each a screenshot's index and its window size, that has `size`, or null when none
 * has it.
 */
export function newestOfSize(screenshots: readonly (readonly [number, Size])[], size: Size): number | null {
  const found = screenshots.findLast(([, each]) => each.width === size.width && each.height === size.height);
  return found?.[0] ?? null;
}

/**
 * The screenshot the region shows over: the one shown, once its pixels are in `full`, or else the one it was drawn
 * on, or else `newest`, the newest screenshot. So while the live view shows, the region keeps the size of the
 * screenshot it was drawn on.
 */
export function regionScreenshot(
  selected: number | null,
  drawnOn: number | null,
  full: ReadonlyMap<number, unknown>,
  newest: number | null,
): number | null {
  if (selected !== null && full.has(selected)) {
    return selected;
  }
  return drawnOn !== null && full.has(drawnOn) ? drawnOn : newest;
}

/** Why the reference editor cannot record `draft`'s region yet, or null when it can. */
export function recordBlocked(draft: Draft): string | null {
  if (draft.region === null || draft.drawnOn === null) {
    return 'Draw the region on a screenshot to record it.';
  }
  if (draft.ink === null) {
    return "Pick the characters' color to record.";
  }
  return null;
}

/** Whether the check being set up was tried on the live window yet, and how it went. */
export type CheckState =
  | { readonly kind: 'idle' }
  | { readonly kind: 'pending' }
  | { readonly kind: 'set' }
  | { readonly kind: 'error'; readonly message: string };

/** The draft's check, or what the user still has to do. */
export type Built<Spec = CheckSpec> =
  | { readonly ok: true; readonly spec: Spec }
  | { readonly ok: false; readonly next: string };

function missing(next: string): { readonly ok: false; readonly next: string } {
  return { ok: false, next };
}

function labelsOf(draft: Draft): readonly { readonly screenshot: number; readonly text: string }[] {
  return [...draft.labels]
    .map(([screenshot, text]) => ({ screenshot, text: text.trim() }))
    .filter((label) => label.text.length > 0)
    .toSorted((left, right) => left.screenshot - right.screenshot);
}

function recordedOf(draft: Draft): readonly RecordedLabel[] {
  return [...draft.recorded]
    .map(([reading, text]) => ({ reading, text: text.trim() }))
    .filter((label) => label.text.length > 0)
    .toSorted((left, right) => left.reading - right.reading);
}

/**
 * The characters review asks about: `draft`'s labeled screenshots and `kept`
 * recorded readings, added to `reference` when it names a saved one, learned
 * the way saving learns them. Null until the draft has a region and the
 * characters' color.
 */
export function reviewSpec(
  draft: Draft,
  kept: readonly RecordedLabel[],
  reference: ReferenceRef | null,
): CharactersSpec | null {
  if (draft.region === null || draft.drawnOn === null || draft.ink === null) {
    return null;
  }
  return {
    kind: 'characters',
    region: draft.region,
    screenshot: draft.drawnOn,
    reference,
    ink: draft.ink,
    tolerance: draft.tolerance,
    labels: labelsOf(draft),
    recorded: kept,
  };
}

/** The characters `draft` teaches the reference `reference`, or a new one when null; or what it still needs. */
export function buildCharacters(draft: Draft, reference: ReferenceRef | null): Built<CharactersSpec> {
  if (draft.region === null || draft.drawnOn === null) {
    return missing('Draw a region around the characters on a screenshot.');
  }
  if (draft.ink === null) {
    return missing("Pick the characters' color.");
  }
  const spec = reviewSpec(draft, recordedOf(draft), reference);
  if (spec === null) {
    return missing("Pick the characters' color.");
  }
  if (reference === null && spec.labels.length === 0 && spec.recorded.length === 0) {
    return missing('Type what the region shows on a screenshot, or record it.');
  }
  return { ok: true, spec };
}

/** The check `draft` describes for the saved application `app`, or the next thing it needs. */
export function buildSpec(draft: Draft, app: string): Built {
  const { region, tolerance } = draft;
  if (region === null) {
    return missing('Draw a region on a screenshot.');
  }
  switch (draft.kind) {
    case 'number': {
      if (draft.reference === null) {
        return missing('Pick a reference to read the number with, or make one from this region.');
      }
      return {
        ok: true,
        spec: { kind: 'number', region, reference: { app, id: draft.reference }, whole: draft.whole },
      };
    }
    case 'barFill': {
      if (draft.filled === null) {
        return missing("Pick the bar's filled color.");
      }
      if (draft.empty === null) {
        return missing("Pick the bar's empty color.");
      }
      return { ok: true, spec: { kind: 'barFill', region, filled: draft.filled, empty: draft.empty } };
    }
    case 'color': {
      if (draft.color === null) {
        return missing('Pick the color to look for.');
      }
      return { ok: true, spec: { kind: 'color', region, color: draft.color, tolerance } };
    }
    case 'state': {
      if (draft.states.length === 0) {
        return missing('Add a state from a screenshot that shows it.');
      }
      if (draft.drawnOn === null) {
        return missing('Draw a region on a screenshot.');
      }
      if (draft.states.some((state) => state.name.trim() === '')) {
        return missing('Name each state.');
      }
      const states = draft.states.map((state): StateSpec => {
        const spec = { name: state.name.trim(), screenshot: state.screenshot };
        return draft.ignoreBackground && state.steps.length > 0
          ? { ...spec, selection: { tolerance: draft.colorMatch, steps: state.steps } }
          : spec;
      });
      return { ok: true, spec: { kind: 'state', region, drawnOn: draft.drawnOn, states, tolerance } };
    }
  }
}

/** The position of the state taken from `screenshot` in `draft`, or -1 when none is. */
export function stateOn(draft: Draft, screenshot: number | null): number {
  return draft.states.findIndex((state) => state.screenshot === screenshot);
}

/** The first of the draft's `stateNames` that no state of `draft` is named, or else the first of `State 1`,
 * `State 2`, … that none is. */
function nextStateName(draft: Draft): string {
  const taken = new Set(draft.states.map((state) => state.name.trim().toLowerCase()));
  const saved = draft.stateNames.find((name) => !taken.has(name.trim().toLowerCase()));
  if (saved !== undefined) {
    return saved;
  }
  let number = draft.states.length + 1;
  while (taken.has(`state ${number}`)) {
    number += 1;
  }
  return `State ${number}`;
}

/**
 * `draft` with a new state taken from `screenshot`, comparing the whole region until something is selected on it;
 * unchanged when a state already comes from that screenshot or the check has as many states as it can hold.
 */
export function withState(draft: Draft, screenshot: number): Draft {
  if (stateOn(draft, screenshot) !== -1 || draft.states.length >= LIMITS.maxStates) {
    return draft;
  }
  return { ...draft, states: [...draft.states, { name: nextStateName(draft), screenshot, steps: [] }] };
}

/**
 * `draft` after the selection step `step` on `screenshot`: the step goes to the state taken from it, or to a new
 * state of that screenshot. Unchanged when the step does nothing, such as taking away from no selection.
 */
export function withStateStep(draft: Draft, screenshot: number, step: SelectStep): Draft {
  const added = withState(draft, screenshot);
  const index = stateOn(added, screenshot);
  const state = added.states[index];
  if (state === undefined) {
    return draft;
  }
  const steps = withStep(state.steps, screenshot, screenshot, step);
  if (steps === null) {
    return draft;
  }
  return { ...added, states: added.states.with(index, { ...state, steps }) };
}

/** `draft` with the state at `index` changed by `update`. */
export function withStateAt(draft: Draft, index: number, update: (state: DraftState) => DraftState): Draft {
  const state = draft.states[index];
  return state === undefined ? draft : { ...draft, states: draft.states.with(index, update(state)) };
}

/** `draft` without the state at `index`. */
export function withoutState(draft: Draft, index: number): Draft {
  return { ...draft, states: draft.states.filter((_, position) => position !== index) };
}

/**
 * `draft` opening the reference editor: its characters start as `existing`'s, or a new reference's, with no labels or
 * recorded readings yet. The region stays, so a new reference starts from the value's.
 */
export function toReference(draft: Draft, existing: ReferenceDef | null): Draft {
  return {
    ...draft,
    ink: existing?.ink ?? null,
    tolerance: existing?.tolerance ?? DEFAULT_TOLERANCE.number,
    labels: new Map(),
    recorded: new Map(),
  };
}

/**
 * `draft` leaving the reference editor for the value it was opened from: the value's region, and the screenshot it
 * was drawn on, come back, whatever region the reference was taught on, and the editor's labels and recorded readings are dropped.
 */
export function fromReference(draft: Draft, value: Drawn): Draft {
  return { ...draft, region: value.region, drawnOn: value.drawnOn, labels: new Map(), recorded: new Map() };
}

/**
 * The draft that draws `check`, a saved value's, again at another window size: the same kind and settings, a bar's
 * or a color's colors and a number's reference, and its states' names for the states taken at the new size. The
 * region starts empty.
 */
export function fixDraft(check: CheckDef): Draft {
  const start = { ...EMPTY_DRAFT, kind: check.kind, tolerance: DEFAULT_TOLERANCE[check.kind] };
  switch (check.kind) {
    case 'number':
      return { ...start, reference: check.reference, whole: check.whole };
    case 'barFill':
      return { ...start, filled: check.filled, empty: check.empty };
    case 'color':
      return { ...start, color: check.color, tolerance: check.tolerance };
    case 'state':
      return { ...start, tolerance: check.tolerance, stateNames: check.states.map((state) => state.name) };
  }
}

/** `draft` switched to `kind`, with that kind's starting tolerance. */
export function withKind(draft: Draft, kind: CheckKind): Draft {
  return { ...draft, kind, tolerance: DEFAULT_TOLERANCE[kind] };
}
