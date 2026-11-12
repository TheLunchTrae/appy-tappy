// Drafts: what a flow keeps of an item the user saved before it was finished, apart from the saved files, so it never
// runs until it is finished.

import { useEffect, useState } from 'react';

import type { AppSummary, DraftDef, DraftKind, DraftSummary } from './bindings.ts';
import { deleteDraft, errorText, saveDraft } from './api.ts';

/**
 * The shape of what each flow keeps in a draft. A draft that keeps another shape was saved by another version of
 * AppyTappy Setup, and cannot be finished in this one.
 */
export const DRAFT_FORMAT = 1;

/** What a flow keeps in a draft: its own state, marked with the format and the kind of flow that reads it. */
export interface Kept<Kind extends DraftKind, State> {
  readonly format: typeof DRAFT_FORMAT;
  readonly kind: Kind;
  readonly state: State;
}

/** `state` as a draft of `kind` keeps it. */
export function keep<Kind extends DraftKind, State>(kind: Kind, state: State): Kept<Kind, State> {
  return { format: DRAFT_FORMAT, kind, state };
}

/** Whether `value` is what a flow of `kind` keeps in a draft of this format. */
export function isKept<K extends Kept<DraftKind, unknown>>(value: unknown, kind: K['kind']): value is K {
  return (
    typeof value === 'object' &&
    value !== null &&
    'format' in value &&
    value.format === DRAFT_FORMAT &&
    'kind' in value &&
    value.kind === kind &&
    'state' in value
  );
}

/** A saved draft that a flow finishes: its ID and what it keeps. */
export interface Resumed {
  readonly id: string;
  readonly def: DraftDef;
}

/** The value, fix, and reference drafts' kinds, which belong to a saved application. */
const OF_APP: readonly DraftKind[] = ['value', 'fix', 'reference'];

/**
 * Why `def` cannot be finished, or null when it can: it was saved by another version, the application it belongs to
 * is gone or cannot be read, or the value it fixes is gone.
 */
export function draftProblem(def: DraftDef, apps: readonly AppSummary[]): string | null {
  if (!isKept(def.state, def.kind)) {
    return 'It was saved by another version of AppyTappy Setup, so it cannot be finished here.';
  }
  if (!OF_APP.includes(def.kind)) {
    return null;
  }
  const app = apps.find((each) => each.id === def.app);
  if (app === undefined) {
    return 'Its application is no longer saved.';
  }
  const { file } = app;
  if (file === null) {
    return "Its application's file cannot be read.";
  }
  if (def.kind === 'fix' && !file.values.some((value) => value.id === def.changes)) {
    return 'The value it fixes is no longer saved.';
  }
  return null;
}

/** What a draft row says is still to do. */
export function unfinishedText(missing: readonly string[]): string {
  return missing.length === 0 ? 'Ready to finish.' : `Not finished. ${missing.join(' ')}`;
}

/** The drafts of `kind`, readable ones only, as the flows that finish them take them. */
export function draftsOf(drafts: readonly DraftSummary[] | null, kind: DraftKind): readonly Resumed[] {
  return (drafts ?? []).flatMap((summary) =>
    summary.draft?.kind === kind ? [{ id: summary.id, def: summary.draft }] : [],
  );
}

/** The draft that changes the saved item `changes` of `kind`, or null when it has none. */
export function draftOfSaved(
  drafts: readonly DraftSummary[] | null,
  kind: DraftKind,
  changes: string,
  app: string | null = null,
): Resumed | null {
  // A save that could not remove an older draft of the same item leaves both; the newest is the one saved last.
  return (
    draftsOf(drafts, kind).findLast((resumed) => resumed.def.changes === changes && resumed.def.app === app) ?? null
  );
}

/** The drafts of `kind` that change no saved item that is still there, which the lists show as their own rows. */
export function newDrafts(
  drafts: readonly DraftSummary[] | null,
  kind: DraftKind,
  saved: (changes: string) => boolean,
): readonly Resumed[] {
  return draftsOf(drafts, kind).filter((resumed) => resumed.def.changes === null || !saved(resumed.def.changes));
}

/** How many of `drafts` go with the application `app` when it is removed: those of its values, fixes and references, and its own draft of changes. */
export function draftsWith(drafts: readonly DraftSummary[] | null, app: string): number {
  return (drafts ?? []).filter(
    ({ draft }) => draft !== null && (draft.app === app || (draft.kind === 'application' && draft.changes === app)),
  ).length;
}

/** What removing an application takes with it: how many `values` and how many `drafts`. */
export function removalLoses(values: number, drafts: number): string {
  const parts = [
    values === 0 ? null : values === 1 ? 'value' : `${String(values)} values`,
    drafts === 0 ? null : drafts === 1 ? 'draft' : `${String(drafts)} drafts`,
  ].filter((part) => part !== null);
  if (parts.length === 0) {
    return 'This cannot be undone.';
  }
  const one = parts.length === 1 && (values === 1 || drafts === 1);
  return `Its ${parts.join(' and ')} ${one ? 'goes' : 'go'} with it, and this cannot be undone.`;
}

/** How a draft save ended: the draft as it is saved, with its ID, or why the save failed. */
export type Saved = Resumed | { readonly failed: string };

/** Saves a flow's draft for App, which asks for it when the user leaves the flow. */
export type DraftSaver = () => Promise<Saved>;

/** Hears the flow's way to save its draft, or null once the flow goes. */
export type ReportDraft = (save: DraftSaver | null) => void;

/** What {@link useDraft} gives a flow. */
interface DraftSaving {
  /** Whether the draft is saving or being deleted. */
  readonly saving: boolean;
  /** Why the last save or delete of the draft failed, or null. */
  readonly error: string | null;
  /** Saves the draft and, once that worked, closes the flow. */
  readonly saveAndClose: () => Promise<void>;
  /** Deletes the draft the flow finishes and, once that worked, closes the flow without saving its item. */
  readonly discard: () => Promise<void>;
  /** Removes the draft the flow finished, once its item is saved. */
  readonly finished: () => Promise<void>;
}

/**
 * Saving the draft of a flow that finishes `resumed`, or makes a new item when it is null. `make` gives the draft as
 * the flow has it now, or is null for a flow that is never a draft, such as an edit of a saved value; `screenshots`
 * says whether the capture's screenshots go with it. The flow reports its saver to `report` while it shows, so
 * leaving it can save the draft too, and calls `onClosed` after its own button saved or deleted the draft; the lists
 * are read again then. A flow that stays open after its draft was saved, because leaving stopped at another save,
 * is given that draft as `resumed`, so it saves over it the next time and removes it once its item is saved.
 */
export function useDraft(
  resumed: Resumed | null,
  make: (() => DraftDef) | null,
  screenshots: boolean,
  report: ReportDraft,
  onClosed: () => void,
): DraftSaving {
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const save: DraftSaver = async () => {
    if (make === null) {
      return { failed: 'It cannot be saved as a draft.' };
    }
    setSaving(true);
    try {
      const def = make();
      const id = await saveDraft(resumed?.id ?? null, def, screenshots);
      setError(null);
      return { id, def };
    } catch (failure: unknown) {
      const failed = errorText(failure);
      setError(failed);
      return { failed };
    } finally {
      setSaving(false);
    }
  };
  // The saver changes with each render, so App's ref always holds the newest; it goes with the flow.
  const drafts = make !== null;
  useEffect(() => {
    report(drafts ? save : null);
  });
  useEffect(
    () => () => {
      report(null);
    },
    [report],
  );
  return {
    saving,
    error,
    saveAndClose: async () => {
      if (!('failed' in (await save()))) {
        onClosed();
      }
    },
    discard: async () => {
      if (resumed === null) {
        return;
      }
      setSaving(true);
      try {
        await deleteDraft(resumed.id);
        setError(null);
      } catch (failure: unknown) {
        setError(errorText(failure));
        return;
      } finally {
        setSaving(false);
      }
      onClosed();
    },
    finished: async () => {
      if (resumed === null) {
        return;
      }
      try {
        await deleteDraft(resumed.id);
      } catch {
        // The item is saved; a draft that stays shows on its page, where it can be deleted.
      }
    },
  };
}
