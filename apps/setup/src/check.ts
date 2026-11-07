// The check the user is setting up, and when it is complete enough to try.

import type { CheckSpec, RecordedLabel } from './api.ts';
import type { Color } from './pixels.ts';
import type { Rect } from './region.ts';

/** The kinds of check, in the order the capture screen offers them. */
export const CHECK_KINDS = ['number', 'barFill', 'color', 'reference'] as const;

export type CheckKind = (typeof CHECK_KINDS)[number];

/** What each kind is called on screen. */
export const KIND_NAMES: Readonly<Record<CheckKind, string>> = {
  number: 'Number',
  barFill: 'Bar fill',
  color: 'Color',
  reference: 'Picture',
};

/** The tolerance each kind starts with. */
export const DEFAULT_TOLERANCE: Readonly<Record<CheckKind, number>> = {
  number: 40,
  barFill: 0,
  color: 24,
  reference: 16,
};

/** Everything the capture screen collects for a check, complete or not. */
export interface Draft {
  readonly kind: CheckKind;
  readonly region: Rect | null;
  readonly tolerance: number;
  /** The number's characters' color. */
  readonly ink: Color | null;
  /** What the region shows on each labeled screenshot, by screenshot index. */
  readonly labels: ReadonlyMap<number, string>;
  /** What each reading kept from the recording shows, by reading index. */
  readonly recorded: ReadonlyMap<number, string>;
  readonly filled: Color | null;
  readonly empty: Color | null;
  readonly color: Color | null;
  /** The screenshot a picture check compares against. */
  readonly reference: number | null;
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
  reference: null,
};

/** The draft's check, or what the user still has to do. */
export type Built = { readonly ok: true; readonly spec: CheckSpec } | { readonly ok: false; readonly next: string };

function missing(next: string): Built {
  return { ok: false, next };
}

function labelsOf(draft: Draft): readonly { readonly screenshot: number; readonly text: string }[] {
  return [...draft.labels]
    .map(([screenshot, text]) => ({ screenshot, text: text.trim() }))
    .filter((label) => label.text.length > 0)
    .toSorted((left, right) => left.screenshot - right.screenshot);
}

/**
 * The number check review asks about: `draft`'s labeled screenshots and
 * `kept` recorded readings, learned the way the saved check learns them.
 * Null until the draft has a region and the characters' color.
 */
export function reviewSpec(
  draft: Draft,
  kept: readonly RecordedLabel[],
): Extract<CheckSpec, { readonly kind: 'number' }> | null {
  if (draft.region === null || draft.ink === null) {
    return null;
  }
  return {
    kind: 'number',
    region: draft.region,
    ink: draft.ink,
    tolerance: draft.tolerance,
    labels: labelsOf(draft),
    recorded: kept,
  };
}

/** The check `draft` describes, or the next thing it needs. */
export function buildSpec(draft: Draft): Built {
  const { region, tolerance } = draft;
  if (region === null) {
    return missing('Draw a region on a screenshot.');
  }
  switch (draft.kind) {
    case 'number': {
      if (draft.ink === null) {
        return missing("Pick the color of the number's characters.");
      }
      const labels = labelsOf(draft);
      const recorded = [...draft.recorded]
        .map(([reading, text]) => ({ reading, text: text.trim() }))
        .filter((label) => label.text.length > 0)
        .toSorted((left, right) => left.reading - right.reading);
      if (labels.length === 0 && recorded.length === 0) {
        return missing('Type what the region shows on a screenshot, or record it.');
      }
      return { ok: true, spec: { kind: 'number', region, ink: draft.ink, tolerance, labels, recorded } };
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
    case 'reference': {
      if (draft.reference === null) {
        return missing('Choose the screenshot to compare against.');
      }
      return { ok: true, spec: { kind: 'reference', region, screenshot: draft.reference, tolerance } };
    }
  }
}

/** `draft` switched to `kind`, with that kind's starting tolerance. */
export function withKind(draft: Draft, kind: CheckKind): Draft {
  return { ...draft, kind, tolerance: DEFAULT_TOLERANCE[kind] };
}
