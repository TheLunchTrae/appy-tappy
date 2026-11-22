// What a draft of a value, a fix, or a reference keeps of the capture screen, and what it still needs.

import type { AppSummary, DraftDef, ReferenceDef, ValueDef } from './bindings.ts';
import { type Built, type CheckKind, type Draft, EMPTY_DRAFT, NEEDS_REFERENCE, fixDraft } from './check.ts';
import { type Kept, type Resumed, draftProblem, isKept, keep } from './drafts.ts';
import { DIGITS, expectedSet } from './expected.ts';
import { referenceCharacters } from './reference.ts';
import { NAME_THE_REFERENCE } from './teaching.ts';
import { sameContent } from './unsaved.ts';

/** The reference a reference's capture makes or changes, its name as typed, and the characters it reads. */
export interface Editing {
  readonly existing: ReferenceDef | null;
  readonly name: string;
  /**
   * The characters the reference reads, each once, as the editor has them; none for every character it learns. It
   * looks for, records, and reads only these.
   */
  readonly reads: readonly string[];
}

/**
 * The reference editor as it opens on `existing`, or on a new reference. A saved reference reads what it was saved
 * reading, and a new one the digits.
 */
export function startEditing(existing: ReferenceDef | null): Editing {
  const reads = existing === null ? DIGITS.characters : expectedSet(existing.characters ?? '');
  return { existing, name: existing?.name ?? '', reads };
}

/** The value's steps: where it shows, then how to read it, which for a number is its characters, and its save. */
export type Step = 'where' | 'how';

/**
 * The screenshot shown on entering `step`, or null for the live view. Where to read lists only the screenshot the
 * value's region is drawn on, `drawnOn`, so that one shows there; any other step keeps `selected`, the one shown.
 */
export function shownOnStep(step: Step, drawnOn: number | null, selected: number | null): number | null {
  return step === 'where' && drawnOn !== null ? drawnOn : selected;
}

/**
 * Whether the stage outlines the value's region, and lets it be drawn, in `step`. A number's second step picks
 * its characters, so its region shows there only while a recording of looks, started before the check became
 * a number, still runs over it; other checks read colors and looks inside the region in their second step, so it shows
 * there too.
 */
export function regionOnStage(step: Step, kind: CheckKind, recording: boolean): boolean {
  return step === 'where' || kind !== 'number' || recording;
}

/**
 * What the capture screen has set up, apart from its screenshots' pixels, which the draft keeps beside it. The
 * value's side and reading area, its check, and a reference's boxes and expected characters are all
 * here; what belongs to a recording that runs or waits for review is not.
 */
export interface CaptureSetup {
  readonly valueName: string;
  readonly step: Step;
  /** Whether the user said the value's content may move around inside the window. */
  readonly mayMove: boolean;
  /** The reference a reference's capture teaches, and null for every other capture. */
  readonly editing: Editing | null;
  readonly draft: Draft;
}

/** {@link CaptureSetup} as JSON holds it: no recorded readings, which go with the recording. */
export interface StoredSetup extends Omit<CaptureSetup, 'draft'> {
  readonly draft: Omit<Draft, 'recorded'>;
}

/** What a draft of a value, a fix, or a reference keeps. */
export type CaptureKept = Kept<'value' | 'fix' | 'reference', StoredSetup>;

/** `setup` as a draft keeps it. */
export function storeSetup(setup: CaptureSetup): StoredSetup {
  const { recorded: _recorded, ...rest } = setup.draft;
  return { ...setup, draft: rest };
}

/**
 * The setup a draft kept, as the capture screen holds it again. Saved-picture removals need another review:
 * a later save can reuse both a picture's name and its label for different pixels.
 */
export function restoreSetup(stored: StoredSetup, reference?: ReferenceDef | null): CaptureSetup {
  const draft: Draft = { ...stored.draft, recorded: new Map(), dropped: [] };
  if (reference === undefined || stored.editing === null) {
    return { ...stored, draft };
  }
  return {
    ...stored,
    editing: { ...stored.editing, existing: reference },
    draft,
  };
}

/**
 * What the capture still needs before it can be saved, a sentence each: a name and a region, then the first thing
 * the check asks for. `fixing` says it fixes a saved value, which keeps its name.
 */
export function captureMissing(setup: CaptureSetup, built: Built<unknown>, fixing: boolean): readonly string[] {
  const { editing, draft } = setup;
  const named = editing === null ? fixing || setup.valueName.trim() !== '' : editing.name.trim() !== '';
  const missing = [
    named ? null : editing === null ? 'Name the value.' : NAME_THE_REFERENCE,
    // A reference reads where its first box is, which what it needs says.
    editing === null && draft.region === null ? 'Draw the region where it shows.' : null,
  ].filter((part) => part !== null);
  return missing.length > 0 || built.ok ? missing : [built.next];
}

/** What a value, fix, or reference draft `def` set up, or null when it was saved by another version. */
export function keptSetup(def: DraftDef): CaptureSetup | null {
  return def.kind !== 'automation' && def.kind !== 'application' && isKept<CaptureKept>(def.state, def.kind)
    ? restoreSetup(def.state.state)
    : null;
}

/** The characters a reference draft has labeled so far, each once, in the order its boxes list them. */
export function labeledCharacters(setup: CaptureSetup): string {
  const seen = new Set<string>();
  for (const { text } of setup.draft.boxes) {
    for (const character of text.replaceAll(/\s/gu, '')) {
      seen.add(character);
    }
  }
  return [...seen].join(' ');
}

/**
 * `editing`, the reference editor a draft kept open, on the reference as `references` have it saved now: what the
 * draft copied may have been taught more, or deleted, since. A reference that is gone is made as a new one.
 */
export function refreshedEditing(editing: Editing | null, references: readonly ReferenceDef[]): Editing | null {
  const id = editing?.existing?.id;
  return editing === null || id === undefined
    ? editing
    : { ...editing, existing: references.find((reference) => reference.id === id) ?? null };
}

/**
 * Whether the fix draft `def` no longer fits `value`: a fix reads its value the way that was first drawn, and the
 * value's check, colors, states, or number settings were changed since the draft copied them.
 */
export function fixOutdated(def: DraftDef, value: ValueDef): boolean {
  const kept = keptSetup(def)?.draft;
  if (def.kind !== 'fix' || kept === undefined) {
    return false;
  }
  const now = fixDraft(value.screen.check);
  const taken = (draft: Draft): unknown => [
    draft.kind,
    draft.filled,
    draft.empty,
    draft.color,
    draft.kind === 'color' || draft.kind === 'state' ? draft.tolerance : null,
    draft.stateNames,
    draft.whole,
    draft.characters,
  ];
  return !sameContent(taken(now), taken(kept));
}

/**
 * Why the value, fix, or reference draft `def` cannot be finished, or null when it can: what {@link draftProblem}
 * finds, or a fix whose value was changed since.
 */
export function captureProblem(def: DraftDef, apps: readonly AppSummary[]): string | null {
  const problem = draftProblem(def, apps);
  if (problem !== null || def.kind !== 'fix') {
    return problem;
  }
  const value = apps.find((app) => app.id === def.app)?.file?.values.find((each) => each.id === def.changes);
  return value !== undefined && fixOutdated(def, value)
    ? 'The value it fixes was changed since, so this fix no longer fits it.'
    : null;
}

/**
 * What a value draft kept without a reference waits for: a number kept without one either waits for its
 * application to have a reference it can read with, or is ready to finish with the one it has.
 */
export type ReferenceWait =
  | { readonly kind: 'none' }
  | { readonly kind: 'waiting' }
  | { readonly kind: 'ready'; readonly reference: ReferenceDef };

/**
 * Whether **Save value** keeps a new value as a draft for want of a reference alone: it has its side and its
 * screenshot, and of what `missing` lists, the reference is all that is left, since the capture asks for a name and
 * a region first.
 */
export function keptForReference(hasSide: boolean, drawnOn: number | null, missing: readonly string[]): boolean {
  return hasSide && drawnOn !== null && missing.includes(NEEDS_REFERENCE);
}

/**
 * What the draft `def` waits for of `references`, its application's saved references. A number kept without a
 * reference, and nothing else the capture asks for first, is ready with the newest that has learned some
 * characters and every character the value needs; any other draft waits for none.
 */
export function referenceWait(def: DraftDef, references: readonly ReferenceDef[]): ReferenceWait {
  const draft = def.kind === 'value' ? keptSetup(def)?.draft : undefined;
  if (draft?.kind !== 'number' || draft.reference !== null || !def.missing.includes(NEEDS_REFERENCE)) {
    return { kind: 'none' };
  }
  const reference = references.findLast((each) => {
    const reads = referenceCharacters(each);
    return reads.length > 0 && draft.characters.every((character) => reads.includes(character));
  });
  return reference === undefined ? { kind: 'waiting' } : { kind: 'ready', reference };
}

/**
 * The value draft `resumed` as it opens once the reference `reference` is saved: with that reference picked, on the
 * step that picks it, reads the live window, and saves.
 */
export function finishedWith(resumed: Resumed, reference: string): Resumed {
  const { def } = resumed;
  if (def.kind !== 'value' || !isKept<CaptureKept>(def.state, 'value')) {
    return resumed;
  }
  const stored = def.state.state;
  return {
    ...resumed,
    def: {
      ...def,
      state: keep('value', { ...stored, step: 'how', draft: { ...stored.draft, reference } }),
    },
  };
}

/**
 * The draft of a new reference that **New reference** opens for the value `setup` sets up for the
 * application `app`: one box at the value's region, on the screenshot it is drawn on, to read the characters the
 * value needs, or the digits while it needs none in particular. Null while the value has no region.
 */
export function referenceForValue(app: string, setup: CaptureSetup): DraftDef | null {
  const { region, drawnOn, characters } = setup.draft;
  if (region === null || drawnOn === null) {
    return null;
  }
  const reads = characters.length > 0 ? characters : DIGITS.characters;
  const reference: CaptureSetup = {
    valueName: '',
    step: 'where',
    mayMove: false,
    editing: { existing: null, name: '', reads },
    draft: { ...EMPTY_DRAFT, boxes: [{ id: 0, screenshot: drawnOn, region, text: '' }] },
  };
  return {
    kind: 'reference',
    app,
    changes: null,
    name: 'New reference',
    missing: [NAME_THE_REFERENCE],
    pictures: [],
    state: keep('reference', storeSetup(reference)),
  };
}
