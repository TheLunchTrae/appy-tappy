import { DRAFT_HINT } from '../../lib/hints.ts';
import { DeleteDraft } from '../Drafts.tsx';
import { Hint } from '../Hint.tsx';

interface Props {
  /** Whether the capture finishes a draft, which it can then delete. */
  readonly resumed: boolean;
  /** Whether the draft changes something saved, rather than making something new. */
  readonly changes: boolean;
  readonly busy: boolean;
  /** Whether there is unsaved work for a draft to keep. */
  readonly unsaved: boolean;
  /** Whether a recording runs or kept readings, which a draft does not keep. */
  readonly losesRecording: boolean;
  readonly onDelete: () => Promise<void>;
  readonly onSave: () => void;
}

/** The capture's draft buttons: delete the draft it finishes, and save what is set up as a draft. */
export function DraftButtons({ resumed, changes, busy, unsaved, losesRecording, onDelete, onSave }: Props) {
  return (
    <>
      {resumed ? <DeleteDraft changes={changes} disabled={busy} onDelete={onDelete} /> : null}
      <span className="with-hint">
        <button type="button" className="btn" disabled={!unsaved} aria-describedby="hint-draft" onClick={onSave}>
          Save as draft
        </button>
        <Hint topic="Save as draft" id="hint-draft">
          {DRAFT_HINT}
          {losesRecording
            ? ' A draft keeps the screenshots, those a recording kept too, but not a recording that runs or the readings kept from one.'
            : ''}
        </Hint>
      </span>
    </>
  );
}
