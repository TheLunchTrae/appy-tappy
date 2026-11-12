// A saved value changed in place: the draft the capture screen opens it with, and the edit it tries and saves.

import type {
  CheckDef,
  CheckSpec,
  EditDrawing,
  EditPreview,
  EditSpec,
  PinDef,
  ScreenDef,
  SizeDef,
  ValueDef,
} from './bindings.ts';
import { type Built, type Draft, buildSpec, fixDraft } from './check.ts';
import { andList } from './reference.ts';
import { sameRect } from './region.ts';
import { guessPin, sameSize, sizeText } from './sizes.ts';
import { sameContent } from './unsaved.ts';

/**
 * The draft that opens `screen`, a saved value's, to edit it: its check's kind and settings, and its region where
 * it is saved, on no screenshot yet. A state check's names wait in `stateNames`, where they can be changed: its
 * states keep their saved pictures until one is taken from a screenshot.
 */
export function editDraft(screen: ScreenDef): Draft {
  return { ...fixDraft(screen.check), region: screen.region };
}

/**
 * `draft` with an edited value's region on the screenshot `shot`, of window size `size`, when `saved` was drawn at
 * that size and the region sits on no screenshot yet; unchanged otherwise. The capture opens an edit on a screenshot
 * it takes itself, so what this makes of the opening draft is what leaving the edit loses nothing of.
 */
export function placedOn(draft: Draft, saved: ScreenDef, shot: number, size: SizeDef): Draft {
  return draft.drawnOn === null && sameSize(size, saved.windowSize) ? { ...draft, drawnOn: shot } : draft;
}

/** The side `screen`'s region stays at when the user picked it by hand, or null while it is the guess from where
 * the region is. */
export function pickedPin(screen: ScreenDef): PinDef | null {
  const pin = screen.pin ?? null;
  return pin === guessPin(screen.region, screen.windowSize) ? null : pin;
}

/**
 * Whether `draft` reads `saved`'s states with the pictures saved for them: a state check on the region as saved,
 * with no state taken from a screenshot. The names and the tolerance may differ.
 */
export function keepsStates(draft: Draft, saved: ScreenDef): boolean {
  return (
    draft.kind === 'state' &&
    saved.check.kind === 'state' &&
    draft.states.length === 0 &&
    sameRect(draft.region, saved.region)
  );
}

/** Says that `value` was drawn at another window size than `shown`, the size of the screenshot the region would
 * show over, where its region cannot be moved. */
export function otherSizeText(value: ValueDef, shown: SizeDef): string {
  const drawn = sizeText(value.screen.windowSize);
  return `${value.name} was drawn at ${drawn}, and the window is ${sizeText(shown)} on this screenshot.`;
}

/** What saving an edit does to the fixes at the window sizes `dropped`, which no longer suit the value. */
export function droppedFixesText(dropped: readonly SizeDef[]): string {
  const fixes = dropped.length === 1 ? 'fix' : 'fixes';
  return (
    `Saving this removes its ${fixes} at ${andList(dropped.map(sizeText))}. A fix reads the value as it is first ` +
    'drawn: with the same kind of check, and the same states in the same order.'
  );
}

/**
 * The window sizes of the fixes the save of the edit that answered `preview` removes with the user's word for it:
 * the ones the edit removes, once the user agreed to lose the fixes at `lost` and those are exactly them. None
 * otherwise, so an edit that comes to remove other fixes asks again.
 */
export function agreedFixes(preview: EditPreview | null, lost: readonly SizeDef[]): readonly SizeDef[] {
  return preview !== null && sameContent(lost, preview.droppedFixes) ? preview.droppedFixes : [];
}

/** Whether the edit that answered `preview` can be saved: no automation stops it, and the user agreed to lose the
 * fixes it removes, if any. Null is an edit not tried yet. */
export function saveable(preview: EditPreview | null, lost: readonly SizeDef[]): boolean {
  return preview?.refused === null && agreedFixes(preview, lost).length === preview.droppedFixes.length;
}

function missing(next: string): { readonly ok: false; readonly next: string } {
  return { ok: false, next };
}

/** `spec` as the file spells it, when it names no screenshot; null for a check that takes pictures. */
function fileCheck(spec: CheckSpec): CheckDef | null {
  switch (spec.kind) {
    case 'number':
      return { kind: 'number', reference: spec.reference.id, whole: spec.whole };
    case 'barFill':
      return { kind: 'barFill', filled: spec.filled, empty: spec.empty };
    case 'color':
      return { kind: 'color', color: spec.color, tolerance: spec.tolerance };
    case 'state':
    case 'characters':
      return null;
  }
}

/** Where and how `draft` reads `saved`, or the next thing it needs. */
function editDrawing(draft: Draft, saved: ValueDef, app: string): Built<EditDrawing> {
  const { screen } = saved;
  if (screen.check.kind === 'state' && keepsStates(draft, screen)) {
    const names = screen.check.states.map((state, index) => draft.stateNames[index]?.trim() ?? state.name);
    if (names.includes('')) {
      return missing('Name each state.');
    }
    const states = screen.check.states.map((state, index) => ({
      name: names[index] ?? state.name,
      image: state.image,
    }));
    return { ok: true, spec: { kind: 'kept', check: { kind: 'state', states, tolerance: draft.tolerance } } };
  }
  const moved = !sameRect(draft.region, screen.region);
  if (draft.kind === 'state' && draft.states.length === 0 && moved) {
    return missing('The region changed, so each state needs a new screenshot. Add each state from one that shows it.');
  }
  // A region that changed, and a state taken from a screenshot, belong to a screenshot of the size it was drawn at.
  const { drawnOn } = draft;
  const shot = missing(
    `Take a screenshot while the window is ${sizeText(screen.windowSize)}, the size ${saved.name} was drawn at.`,
  );
  const built = drawnOn === null && draft.kind === 'state' ? shot : buildSpec(draft, app);
  if (!built.ok) {
    return built;
  }
  const kept = moved ? null : fileCheck(built.spec);
  if (kept !== null) {
    return { ok: true, spec: { kind: 'kept', check: kept } };
  }
  return drawnOn === null ? shot : { ok: true, spec: { kind: 'drawn', check: built.spec, screenshot: drawnOn } };
}

/**
 * The edit of `saved`, a value of the application `app`, that `draft` describes under the name `name`, staying at
 * `pin`; or the next thing it needs. The region as saved needs no screenshot, so the window may have any size. A
 * region that changed, and a state taken from a screenshot, need the screenshot the region is drawn on.
 */
export function buildEdit(draft: Draft, saved: ValueDef, app: string, name: string, pin: PinDef): Built<EditSpec> {
  const valueName = name.trim();
  if (valueName === '') {
    return missing('Name the value.');
  }
  const drawing = editDrawing(draft, saved, app);
  return drawing.ok ? { ok: true, spec: { app, value: saved.id, valueName, pin, drawing: drawing.spec } } : drawing;
}
