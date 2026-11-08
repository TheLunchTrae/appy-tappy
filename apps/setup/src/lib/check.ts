// The check the user is setting up, and when it is complete enough to try.

import type { CharactersSpec, CheckSpec, RecordedLabel, ReferenceDef, ReferenceRef } from './api.ts';
import type { Color } from './pixels.ts';
import type { Rect } from './region.ts';
import { DEFAULT_COLOR_MATCH, type SelectStep } from './selection.ts';

/** The kinds of check, in the order the capture screen offers them. */
export const CHECK_KINDS = ['number', 'barFill', 'color', 'image'] as const;

export type CheckKind = (typeof CHECK_KINDS)[number];

/** What each kind is called on screen. */
export const KIND_NAMES: Readonly<Record<CheckKind, string>> = {
  number: 'Number',
  barFill: 'Bar fill',
  color: 'Color',
  image: 'Image',
};

/** The tolerance each kind starts with; a number's is its new reference's. */
export const DEFAULT_TOLERANCE: Readonly<Record<CheckKind, number>> = {
  number: 40,
  barFill: 0,
  color: 24,
  image: 16,
};

/** Everything the capture screen collects for a check or a reference, complete or not. */
export interface Draft {
  readonly kind: CheckKind;
  readonly region: Rect | null;
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
  /** The screenshot an image check compares against. */
  readonly image: number | null;
  /** How the user selected the thing an image check looks for, on its screenshot. */
  readonly steps: readonly SelectStep[];
  /** How far a color may be from another and still count as alike, while selecting. */
  readonly colorMatch: number;
  /** Whether an image check compares only the selected pixels. */
  readonly ignoreBackground: boolean;
  /** The ID of the reference a number check reads with. */
  readonly reference: string | null;
  /** Whether a number check ignores every digit after the decimal point. */
  readonly whole: boolean;
}

export const EMPTY_DRAFT: Draft = {
  kind: 'number',
  region: null,
  tolerance: DEFAULT_TOLERANCE.number,
  ink: null,
  labels: new Map(),
  recorded: new Map(),
  filled: null,
  empty: null,
  color: null,
  image: null,
  reference: null,
  steps: [],
  colorMatch: DEFAULT_COLOR_MATCH,
  ignoreBackground: true,
  whole: false,
};

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
  if (draft.region === null || draft.ink === null) {
    return null;
  }
  return {
    kind: 'characters',
    region: draft.region,
    reference,
    ink: draft.ink,
    tolerance: draft.tolerance,
    labels: labelsOf(draft),
    recorded: kept,
  };
}

/** The characters `draft` teaches the reference `reference`, or a new one when null; or what it still needs. */
export function buildCharacters(draft: Draft, reference: ReferenceRef | null): Built<CharactersSpec> {
  if (draft.region === null) {
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
    case 'image': {
      if (draft.image === null) {
        return missing('Choose the screenshot to compare against.');
      }
      const spec = { kind: 'image', region, screenshot: draft.image, tolerance } as const;
      if (!draft.ignoreBackground || draft.steps.length === 0) {
        return { ok: true, spec };
      }
      return { ok: true, spec: { ...spec, selection: { tolerance: draft.colorMatch, steps: draft.steps } } };
    }
  }
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
 * `draft` leaving the reference editor for the value it was opened from: the value's region comes back, whatever
 * region the reference was taught on, and the editor's labels and recorded readings are dropped.
 */
export function fromReference(draft: Draft, valueRegion: Rect | null): Draft {
  return { ...draft, region: valueRegion, labels: new Map(), recorded: new Map() };
}

/** `draft` switched to `kind`, with that kind's starting tolerance. */
export function withKind(draft: Draft, kind: CheckKind): Draft {
  return { ...draft, kind, tolerance: DEFAULT_TOLERANCE[kind] };
}
