// What a draft of a value, a fix, or a reference keeps of the capture screen, and what it still needs.

import type { AppSummary, DraftDef, ReferenceDef, ValueDef } from './bindings.ts';
import { type Built, type CheckKind, type Draft, fixDraft } from './check.ts';
import { type Kept, draftProblem, isKept } from './drafts.ts';
import { sameContent } from './unsaved.ts';

/** The reference being made or changed, and its name as typed. */
export interface Editing {
  readonly existing: ReferenceDef | null;
  readonly name: string;
  /**
   * The characters the value it is taught for needs, as the editor has them: the value's own when the editor opens,
   * which the value takes back only when the reference is saved. None for a reference made by itself.
   */
  readonly needs: readonly string[];
}

/** What the capture makes or changes: a value, a saved value's edit or fix, or a reference by itself. */
export type CaptureFlow = 'value' | 'edit' | 'fix' | 'reference';

/**
 * How the reference editor shows the characters the value needs. A reference made by itself is taught for no value,
 * so it shows none. A fix needs what its value does, so it shows them and changes none.
 */
export type NeedsField = 'hidden' | 'editable' | 'readOnly';

/** How the reference editor of `flow` shows the characters the value needs. */
export function needsField(flow: CaptureFlow): NeedsField {
  if (flow === 'reference') {
    return 'hidden';
  }
  return flow === 'fix' ? 'readOnly' : 'editable';
}

/** The reference editor of `flow` as it opens on `existing`, or on a new reference, from the check `draft`. */
export function startEditing(flow: CaptureFlow, existing: ReferenceDef | null, draft: Draft): Editing {
  return { existing, name: existing?.name ?? '', needs: flow === 'reference' ? [] : draft.characters };
}

/**
 * `draft`, a value's, once the reference editor `editing` saved `reference` for it: a number that reads with it and
 * needs the characters the editor left. Leaving the editor any other way keeps the characters the value had.
 */
export function withTaught(draft: Draft, reference: string, editing: Editing | null): Draft {
  return { ...draft, kind: 'number', reference, characters: editing?.needs ?? draft.characters };
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
 * Whether the stage outlines the value's region, and lets it be drawn, in `step`. A number's later steps pick and
 * check its characters, so its region shows there only while a recording of looks, started before the check became
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
 * `draft`, kept with the reference editor open on the reference as `copied` has it, going on from the reference as
 * `now` has it saved. The saved pictures it left out are named by file, and a reference saved since can give a new
 * picture a removed one's file name, so they stay left out only while the reference has the very pictures the draft
 * copied.
 */
export function refreshedDraft(draft: Draft, copied: ReferenceDef | null, now: ReferenceDef | null): Draft {
  const same = JSON.stringify(copied?.samples ?? []) === JSON.stringify(now?.samples ?? []);
  return same || draft.dropped.length === 0 ? draft : { ...draft, dropped: [] };
}

/**
 * The setup a draft kept, on `current` when its saved reference is already known. Refresh the sample removals
 * against the copied reference before replacing it; with no current file yet, keep the copy until it arrives.
 */
export function restoreSetup(stored: StoredSetup, current?: ReferenceDef | null): CaptureSetup {
  const draft = { ...stored.draft, recorded: new Map<number, string>() };
  return current === undefined || stored.editing === null
    ? { ...stored, draft }
    : {
        ...stored,
        editing: { ...stored.editing, existing: current },
        draft: refreshedDraft(draft, stored.editing.existing, current),
      };
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
    draft.stateNames,
    draft.whole,
    draft.characters,
    draft.kind === 'color' || draft.kind === 'state' ? draft.tolerance : null,
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
