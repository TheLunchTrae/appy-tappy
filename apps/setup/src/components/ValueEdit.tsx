// What the capture screen adds while it edits a saved value: what an edit is, the states that keep their saved
// pictures, and what saving the edit would do.

import type { EditPreview, SizeDef, ValueDef } from '../lib/bindings.ts';
import type { Draft } from '../lib/check.ts';
import { droppedFixesText, otherSizeText } from '../lib/edit.ts';
import { EDIT_HINT } from '../lib/hints.ts';
import { sizeText } from '../lib/sizes.ts';
import { ToleranceField } from './Fields.tsx';
import { Hint } from './Hint.tsx';

/**
 * Says that this edits the value itself, not one size's fix, and what a window of another size than the value was
 * drawn at leaves to change. `shown` is the size of the screenshot the region would show over, when it is not the
 * size the value was drawn at.
 */
export function EditIntro({ value, shown }: { readonly value: ValueDef; readonly shown: SizeDef | null }) {
  const drawn = sizeText(value.screen.windowSize);
  return (
    <>
      <div className="group-title">
        <span>Edit {value.name}</span>
        <Hint topic="Edit">{EDIT_HINT}</Hint>
      </div>
      <p className="note">
        This changes {value.name} itself, as it was drawn at {drawn}. Automations that use it keep using it.
      </p>
      {shown === null ? null : (
        <>
          <p className="warn" role="status">
            {otherSizeText(value, shown)}
          </p>
          <p className="note">
            Its region and its states&apos; pictures stay as they are saved on this screenshot; everything else can
            change. To move the region or take the states again, resize the window to {drawn} and take a screenshot. To
            change how it reads at {sizeText(shown)} only, discard this and use Fix at this size on the Values page.
          </p>
        </>
      )}
    </>
  );
}

interface SavedStatesProps {
  readonly value: ValueDef;
  readonly draft: Draft;
  /** Why the states cannot be taken again from the screenshot shown, or null when they can. */
  readonly retakeBlocked: string | null;
  readonly onDraft: (update: (draft: Draft) => Draft) => void;
  /** Takes the first state again from the screenshot shown. */
  readonly onRetake: () => void;
}

/** A saved state check's states while they keep their saved pictures: their names, and the tolerance. */
export function SavedStates({ value, draft, retakeBlocked, onDraft, onRetake }: SavedStatesProps) {
  const { check } = value.screen;
  const states = check.kind === 'state' ? check.states : [];
  return (
    <>
      <div className="field">
        <span id="saved-states-title">States</span>
        <span className="note">Each state reads with the picture saved for it. Rename a state here.</span>
        <ul className="states" aria-labelledby="saved-states-title">
          {states.map((state, index) => (
            <li key={state.image} className="state saved">
              <input
                className="input"
                aria-label={`Name of state ${index + 1}`}
                placeholder="Name"
                value={draft.stateNames[index] ?? ''}
                onChange={(event) => {
                  const typed = event.target.value;
                  onDraft((current) => ({ ...current, stateNames: current.stateNames.with(index, typed) }));
                }}
              />
              <small className="note">Saved picture</small>
            </li>
          ))}
        </ul>
        <span className="note">
          To change a state&apos;s picture, or to add or remove a state, take the states again: each one from a
          screenshot at {sizeText(value.screen.windowSize)} that shows it.
        </span>
        <div className="choices">
          <button
            type="button"
            className="btn small"
            disabled={retakeBlocked !== null}
            title={retakeBlocked ?? undefined}
            onClick={onRetake}
          >
            Take the states again
          </button>
        </div>
      </div>
      <ToleranceField
        help="How far, on average, the compared pixels may differ from a state's image."
        value={draft.tolerance}
        onChange={(tolerance) => {
          onDraft((current) => ({ ...current, tolerance }));
        }}
      />
    </>
  );
}

interface OutcomeProps {
  readonly preview: EditPreview;
  /** Whether the user agreed to lose the fixes the edit removes. */
  readonly agreed: boolean;
  /** Agrees, or not, to lose the fixes at these window sizes. */
  readonly onAgree: (dropped: readonly SizeDef[]) => void;
}

/** What saving an edit would do beyond the value itself: the automations that stop it, and the fixes it removes,
 * which go only once the user agrees. */
export function EditOutcome({ preview, agreed, onAgree }: OutcomeProps) {
  const { refused, droppedFixes } = preview;
  return (
    <>
      {refused === null ? null : (
        <p className="warn" role="alert">
          {refused}
        </p>
      )}
      {droppedFixes.length === 0 ? null : (
        <div className="field">
          <p className="warn" id="dropped-fixes">
            {droppedFixesText(droppedFixes)}
          </p>
          <label className="toggle">
            <input
              type="checkbox"
              className="switch"
              aria-describedby="dropped-fixes"
              checked={agreed}
              onChange={(event) => {
                onAgree(event.target.checked ? droppedFixes : []);
              }}
            />
            {droppedFixes.length === 1 ? 'Remove its fix' : 'Remove its fixes'}
          </label>
        </div>
      )}
    </>
  );
}
