// What a draft of a value, a fix, or a reference keeps of the capture screen, and what it still needs.

import type { DraftDef, ReferenceDef } from './bindings.ts';
import type { Built, Draft, Drawn } from './check.ts';
import { type Kept, isKept } from './drafts.ts';

/** The reference being made or changed, and its name as typed. */
export interface Editing {
  readonly existing: ReferenceDef | null;
  readonly name: string;
  /** The characters it needs, each once. */
  readonly expects: readonly string[];
  /** The value's region when the editor opened from a value, to put back when it closes. */
  readonly valueRegion: Drawn;
}

/** The value's two steps: where it shows, then how to read it. */
export type Step = 'where' | 'how';

/**
 * What the capture screen has set up, apart from its screenshots' pixels, which the draft keeps beside it. The
 * value's side and reading area, its check, and a reference's labels, found text and expected characters are all
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

/** {@link CaptureSetup} as JSON holds it: the labels as pairs, and no recorded readings, which go with the recording. */
export interface StoredSetup extends Omit<CaptureSetup, 'draft'> {
  readonly draft: Omit<Draft, 'labels' | 'recorded'> & { readonly labels: readonly (readonly [number, string])[] };
}

/** What a draft of a value, a fix, or a reference keeps. */
export type CaptureKept = Kept<'value' | 'fix' | 'reference', StoredSetup>;

/** `setup` as a draft keeps it. */
export function storeSetup(setup: CaptureSetup): StoredSetup {
  const { labels, recorded: _recorded, ...rest } = setup.draft;
  return { ...setup, draft: { ...rest, labels: [...labels] } };
}

/** The setup a draft kept, as the capture screen holds it again. */
export function restoreSetup(stored: StoredSetup): CaptureSetup {
  return { ...stored, draft: { ...stored.draft, labels: new Map(stored.draft.labels), recorded: new Map() } };
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
    draft.region === null ? 'Draw the region where it shows.' : null,
  ].filter((part) => part !== null);
  return missing.length > 0 || built.ok ? missing : [built.next];
}

/** What a value, fix, or reference draft `def` set up, or null when it was saved by another version. */
export function keptSetup(def: DraftDef): CaptureSetup | null {
  return def.kind !== 'automation' && def.kind !== 'application' && isKept<CaptureKept>(def.state, def.kind)
    ? restoreSetup(def.state.state)
    : null;
}

/** The characters a reference draft has labeled so far, each once, in the order they were first labeled. */
export function labeledCharacters(setup: CaptureSetup): string {
  const seen = new Set<string>();
  for (const text of setup.draft.labels.values()) {
    for (const character of text.replaceAll(/\s/gu, '')) {
      seen.add(character);
    }
  }
  return [...seen].join(' ');
}
