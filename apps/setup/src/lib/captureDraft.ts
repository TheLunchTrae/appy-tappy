// What a draft of a value, a fix, or a reference keeps of the capture screen, and what it still needs.

import type { AppSummary, DraftDef, ReferenceDef, ValueDef } from './bindings.ts';
import { type Built, type Draft, fixDraft } from './check.ts';
import { type Kept, draftProblem, isKept } from './drafts.ts';
import { sameContent } from './unsaved.ts';

/** The reference being made or changed, and its name as typed. */
export interface Editing {
  readonly existing: ReferenceDef | null;
  readonly name: string;
  /** The characters it needs, each once. */
  readonly expects: readonly string[];
}

/**
 * The value's steps: where it shows, then how to read it, which for a number is its characters, then, for a number
 * only, the check of how it reads and its save.
 */
export type Step = 'where' | 'how' | 'check';

/**
 * The screenshot shown on entering `step`, or null for the live view. Where to read lists only the screenshot the
 * value's region is drawn on, `drawnOn`, so that one shows there; any other step keeps `selected`, the one shown.
 */
export function shownOnStep(step: Step, drawnOn: number | null, selected: number | null): number | null {
  return step === 'where' && drawnOn !== null ? drawnOn : selected;
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
  readonly editing: Editing | null;
  readonly draft: Draft;
  /** The screenshots a recording of looks kept, by index, which the strip marks and can remove together. */
  readonly recordedShots: readonly number[];
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

/** The setup a draft kept, as the capture screen holds it again. */
export function restoreSetup(stored: StoredSetup): CaptureSetup {
  return { ...stored, draft: { ...stored.draft, recorded: new Map() } };
}

/**
 * What the capture still needs before it can be saved, a sentence each: a name and a region, then the first thing
 * the check asks for. `fixing` says it fixes a saved value, which keeps its name.
 */
export function captureMissing(setup: CaptureSetup, built: Built<unknown>, fixing: boolean): readonly string[] {
  const { editing, draft } = setup;
  const named = editing === null ? fixing || setup.valueName.trim() !== '' : editing.name.trim() !== '';
  const missing = [
    named ? null : editing === null ? 'Name the value.' : 'Name the reference.',
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
 * value's check, colors, or states were changed since the draft copied them.
 */
export function fixOutdated(def: DraftDef, value: ValueDef): boolean {
  const kept = keptSetup(def)?.draft;
  if (def.kind !== 'fix' || kept === undefined) {
    return false;
  }
  const now = fixDraft(value.screen.check);
  const taken = (draft: Draft): unknown => [draft.kind, draft.filled, draft.empty, draft.color, draft.stateNames];
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
