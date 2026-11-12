import { useEffect, useState } from 'react';

import type { AppSummary, ReferenceDef } from '../lib/bindings.ts';
import { restoreDraftScreenshots, stopCapture } from '../lib/api.ts';
import { Capture, type CaptureMode, type Restored } from '../components/Capture.tsx';
import { type Choice, Picker } from '../components/Picker.tsx';
import { background } from '../lib/repeat.ts';
import type { Held } from '../lib/unsaved.ts';
import type { ReportDraft, Resumed } from '../lib/drafts.ts';

/** A saved reference and the application it belongs to. */
export interface SavedRef {
  readonly app: string;
  readonly reference: ReferenceDef;
}

interface Props {
  readonly apps: readonly AppSummary[];
  /** The reference to change, or null to make one. */
  readonly existing: SavedRef | null;
  /** The draft the flow finishes, or null. */
  readonly draft: Resumed | null;
  readonly onCancel: () => void;
  readonly onSaved: () => void;
  /** Leaves to add an application, for when the one wanted is not saved yet. */
  readonly onAddApplication: () => void;
  /** Hears what leaving the flow would lose. */
  readonly onHeld: (held: Held) => void;
  /** Hears how to save the flow's draft. */
  readonly onDraft: ReportDraft;
}

/** Making or changing a reference: pick its application's window, then teach it the characters. */
export function ReferenceFlow({ apps, existing, draft, onCancel, onSaved, onAddApplication, onHeld, onDraft }: Props) {
  const [choice, setChoice] = useState<Choice | null>(null);
  const [picking, setPicking] = useState(true);
  const [restored, setRestored] = useState<Restored | null>(null);
  const [mode] = useState<CaptureMode>({ kind: 'reference', existing: existing?.reference ?? null });
  // Leaving the flow stops the window's capture.
  useEffect(
    () => () => {
      background(stopCapture());
    },
    [],
  );
  if (picking || choice === null) {
    return (
      <Picker
        apps={apps}
        heading={existing === null ? 'New reference' : `Edit ${existing.reference.name}`}
        lockedApp={existing?.app ?? draft?.def.app ?? null}
        initial={choice}
        onCancel={onCancel}
        onAddApplication={onAddApplication}
        onPick={async (picked) => {
          // Picking the window forgets the session's screenshots, so a draft's come back after it, the first time.
          if (draft !== null) {
            setRestored({
              resumed: draft,
              screenshots: restored === null ? await restoreDraftScreenshots(draft.id) : null,
            });
          }
          setChoice(picked);
          setPicking(false);
        }}
      />
    );
  }
  return (
    <Capture
      choice={choice}
      mode={mode}
      restored={restored}
      onCancel={onCancel}
      onChangeWindow={() => {
        setPicking(true);
      }}
      onSaved={onSaved}
      onHeld={onHeld}
      onDraft={onDraft}
    />
  );
}
