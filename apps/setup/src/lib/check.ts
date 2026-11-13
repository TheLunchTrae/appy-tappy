// The check the user is setting up, and when it is complete enough to try.

import {
  type CharactersSpec,
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
import { type Box, boxLabels } from './boxes.ts';
import { forReference, savedCharacters } from './charactersRead.ts';
import { AUTOMATIC, type PinChoice } from './pin.ts';
import { knownCharacters, referenceCharacters } from './reference.ts';
import type { Color } from './pixels.ts';
import { type Rect, type Size, sameRect } from './region.ts';
import { sameSize, sizeText } from './sizes.ts';
import { DEFAULT_COLOR_MATCH, withStep } from './selection.ts';

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
  /** The boxes a reference learns from, each around characters on a screenshot, in the order they are listed. */
  readonly boxes: readonly Box[];
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
  /** The characters a number check reads of its reference, each once, or null for every character it knows. */
  readonly characters: readonly string[] | null;
}

export const EMPTY_DRAFT: Draft = {
  kind: 'number',
  region: null,
  drawnOn: null,
  pin: AUTOMATIC,
  tolerance: DEFAULT_TOLERANCE.number,
  ink: null,
  boxes: [],
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
  characters: null,
};

/** Where the region is: its rectangle and the screenshot it was drawn on. */
export type Drawn = Pick<Draft, 'region' | 'drawnOn'>;

/**
 * Where the reference being taught reads, records, and sizes its boxes: the value's region when it is taught from a
 * value, or else its first box. Every box must be on a screenshot of the size that one is on.
 */
export function readingOf(draft: Draft): Drawn {
  if (draft.region !== null && draft.drawnOn !== null) {
    return { region: draft.region, drawnOn: draft.drawnOn };
  }
  const first = draft.boxes[0];
  return first === undefined ? { region: null, drawnOn: null } : { region: first.region, drawnOn: first.screenshot };
}

/**
 * Whether `draft` would record other than the stopped recording's `settings`: another reading region, characters' color or
 * tolerance, or a region drawn on a screenshot whose window size `drawnSize` is not the recording's.
 */
export function settingsDiffer(draft: Draft, settings: RecordingSettings, drawnSize: Size | null): boolean {
  return (
    draft.tolerance !== settings.tolerance ||
    !sameRect(readingOf(draft).region, settings.region) ||
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

/** `draft` with the characters' color and tolerance of the stopped recording's `settings`, and its place as it is. */
export function withRecordedInk(draft: Draft, settings: RecordingSettings): Draft {
  return { ...draft, ink: settings.ink, tolerance: settings.tolerance };
}

/**
 * The screenshot the reading box goes back on to read where the stopped recording's `settings` read, or null when it
 * stays. `boxOn` is the screenshot the box the recording read at is on now, or null when that box is gone, and
 * `screenshots` each screenshot's index and its window size. The box keeps its screenshot when that has the size
 * recorded at, and otherwise goes on the newest one that has it. A reference taught from a value reads at the
 * value's region, which the editor leaves alone, and with no screenshot of the size the box has nowhere to go.
 */
export function recordedPlace(
  draft: Draft,
  settings: RecordingSettings,
  boxOn: number | null,
  screenshots: readonly (readonly [number, Size])[],
): number | null {
  if (draft.region !== null) {
    return null;
  }
  const own = screenshots.find(([index]) => index === boxOn)?.[1];
  return boxOn !== null && own?.width === settings.windowSize.width && own.height === settings.windowSize.height
    ? boxOn
    : newestOfSize(screenshots, settings.windowSize);
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

/** Why the reference editor cannot record `draft`'s reading region yet, or null when it can. */
export function recordBlocked(draft: Draft): string | null {
  const reading = readingOf(draft);
  if (reading.region === null || reading.drawnOn === null) {
    return 'Draw a box around the characters on a screenshot to record them.';
  }
  if (draft.ink === null) {
    return "Pick the characters' color to record.";
  }
  return null;
}

/** Why Find more like it cannot search for text like `sample`, the selected box, yet, or null when it can. */
export function findBlocked(draft: Draft, sample: Box | null): string | null {
  if (sample === null) {
    return 'Select a box around one sample first.';
  }
  if (draft.ink === null) {
    return "Pick the characters' color first.";
  }
  return null;
}

/**
 * Why a box cannot go on a screenshot of size `shown`, or null when it can: every box must be on a screenshot of the
 * size `reading`, the size the reference reads at, once it has one.
 */
export function boxBlocked(reading: Size | null, shown: Size | null): string | null {
  return reading === null || shown === null || sameSize(reading, shown)
    ? null
    : `Boxes go on screenshots at ${sizeText(reading)}, the size the reference reads at.`;
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

function recordedOf(draft: Draft): readonly RecordedLabel[] {
  return [...draft.recorded]
    .map(([reading, text]) => ({ reading, text: text.trim() }))
    .filter((label) => label.text.length > 0)
    .toSorted((left, right) => left.reading - right.reading);
}

/**
 * The characters review asks about: `draft`'s labeled boxes and `kept`
 * recorded readings, added to `reference` when it names a saved one, learned
 * the way saving learns them. Null until the draft has a place to read and the
 * characters' color.
 */
export function reviewSpec(
  draft: Draft,
  kept: readonly RecordedLabel[],
  reference: ReferenceRef | null,
): CharactersSpec | null {
  const { region, drawnOn } = readingOf(draft);
  if (region === null || drawnOn === null || draft.ink === null) {
    return null;
  }
  return {
    region,
    screenshot: drawnOn,
    reference,
    ink: draft.ink,
    tolerance: draft.tolerance,
    labels: boxLabels(draft.boxes),
    recorded: kept,
  };
}

/**
 * The characters `draft` teaches so far, its kept recorded readings included, added to `reference` when it names a
 * saved one; null until the draft has a region and the characters' color.
 */
export function taughtSpec(draft: Draft, reference: ReferenceRef | null): CharactersSpec | null {
  return reviewSpec(draft, recordedOf(draft), reference);
}

/** The characters `draft` teaches the reference `reference`, or a new one when null; or what it still needs. */
export function buildCharacters(draft: Draft, reference: ReferenceRef | null): Built<CharactersSpec> {
  const reading = readingOf(draft);
  if (reading.region === null || reading.drawnOn === null) {
    return missing('Draw a box around the characters on a screenshot.');
  }
  if (draft.ink === null) {
    return missing("Pick the characters' color.");
  }
  const spec = reviewSpec(draft, recordedOf(draft), reference);
  if (spec === null) {
    return missing("Pick the characters' color.");
  }
  if (reference === null && spec.labels.length === 0 && spec.recorded.length === 0) {
    return missing('Type what a box shows, or record the characters.');
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
      if (draft.characters?.length === 0) {
        return missing('Choose at least one character for the value to read.');
      }
      const spec = { kind: 'number', region, reference: { app, id: draft.reference }, whole: draft.whole } as const;
      return { ok: true, spec: draft.characters === null ? spec : { ...spec, characters: draft.characters.join('') } };
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
 * `draft` opening the reference editor: its characters start as `existing`'s, or a new reference's, with no boxes or
 * recorded readings yet. The value's region stays, and the reference reads there.
 */
export function toReference(draft: Draft, existing: ReferenceDef | null): Draft {
  return {
    ...draft,
    ink: existing?.ink ?? null,
    tolerance: existing?.tolerance ?? DEFAULT_TOLERANCE.number,
    boxes: [],
    recorded: new Map(),
  };
}

/** `draft` leaving the reference editor for the value it was opened from, without the editor's boxes or readings. */
export function fromReference(draft: Draft): Draft {
  return { ...draft, boxes: [], recorded: new Map() };
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
      return {
        ...start,
        reference: check.reference,
        whole: check.whole,
        characters: savedCharacters(check.characters),
      };
    case 'barFill':
      return { ...start, filled: check.filled, empty: check.empty };
    case 'color':
      return { ...start, color: check.color, tolerance: check.tolerance };
    case 'state':
      return { ...start, tolerance: check.tolerance, stateNames: check.states.map((state) => state.name) };
  }
}

/**
 * `draft`, a fix's, with its region at `region` on the screenshot `shot`, where the value reads at that screenshot's
 * size; unchanged when it reads nowhere there, or a region is drawn already. The capture opens a fix on a
 * screenshot it takes itself, so what this makes of the opening draft is what leaving the fix loses nothing of.
 */
export function withFixRegion(draft: Draft, region: Rect | null, shot: number): Draft {
  return region === null || draft.region !== null ? draft : { ...draft, region, drawnOn: shot };
}

/**
 * `draft`, a number value's, reading with `reference` of `references`, its application's, and the characters of it
 * that its choice of characters comes to, as {@link forReference} says. `before` is the references as they were
 * when the draft's own reference was picked, when a save changed them since, such as by teaching that reference
 * more. A reference that `references` do not hold, such as while the file cannot be read, leaves the choice as it
 * is: nothing says what it knows.
 */
export function withReference(
  draft: Draft,
  reference: string,
  references: readonly ReferenceDef[],
  before: readonly ReferenceDef[] = references,
): Draft {
  const known = (id: string | null, among: readonly ReferenceDef[]): readonly string[] | null => {
    const saved = among.find((each) => each.id === id);
    return saved === undefined ? null : referenceCharacters(saved);
  };
  const to = known(reference, references);
  const characters =
    to === null ? draft.characters : forReference(draft.characters, known(draft.reference, before) ?? [], to);
  return { ...draft, reference, characters };
}

/** `characters`, a number's choice, as a live check takes it: null for every character, and until it names one. */
export function readsOf(characters: readonly string[] | null): string | null {
  return characters === null || characters.length === 0 ? null : characters.join('');
}

/**
 * The characters the number `draft` describes reads once the reference `taught` teaches is saved and the number
 * reads with it, or null for every character it knows. `references` are its application's before that save. The
 * number's choice stays the one that was picked, as {@link forReference} says, from what the reference it reads
 * with knows now to what the taught one will know: its saved samples, when it is saved already, and what `taught`
 * labels. So Digits and symbols takes up a symbol that is being taught.
 */
export function charactersOnceTaught(
  draft: Draft,
  references: readonly ReferenceDef[],
  taught: CharactersSpec,
): readonly string[] | null {
  const known = (id: string | null | undefined): readonly string[] => {
    const saved = references.find((each) => each.id === id);
    return saved === undefined ? [] : referenceCharacters(saved);
  };
  const to = knownCharacters([
    ...known(taught.reference?.id),
    ...taught.labels.map((label) => label.text),
    ...taught.recorded.map((label) => label.text),
  ]);
  return forReference(draft.characters, known(draft.reference), to);
}

/** `draft` switched to `kind`, with that kind's starting tolerance. */
export function withKind(draft: Draft, kind: CheckKind): Draft {
  return { ...draft, kind, tolerance: DEFAULT_TOLERANCE[kind] };
}

/**
 * `draft` once screenshot `shot` is the picture the value's region is set on, as a new screenshot taken while its
 * place is set is. `fits` says the screenshot has the size the region is drawn at, so the region moves onto it; on
 * one of another size it is drawn again, unless it is drawn on no screenshot yet, such as an edited value's waiting
 * for a screenshot of its size. `fits` is null while either size is unknown, such as before the screenshot's pixels
 * arrive, and the draft stays as it is.
 */
export function onPicture(draft: Draft, shot: number, fits: boolean | null): Draft {
  if (draft.region === null || fits === null) {
    return draft;
  }
  if (fits) {
    return { ...draft, drawnOn: shot };
  }
  return draft.drawnOn === null ? draft : { ...draft, region: null, drawnOn: null };
}
