import { useState } from 'react';

import type { AppSummary } from '../lib/bindings.ts';
import { restoreDraftScreenshots } from '../lib/api.ts';
import { useCapture } from '../lib/capture.ts';
import { Capture, type CaptureMode, type Restored } from '../components/Capture.tsx';
import { type Choice, Picker } from '../components/Picker.tsx';
import type { Made, SavedRef } from '../lib/flow.ts';
import type { Held } from '../lib/unsaved.ts';
import type { ReportDraft, Resumed } from '../lib/drafts.ts';

interface Props {
  /** The reference to change, or null to make one. */
  readonly existing: SavedRef | null;
  /** The application a new reference starts with, such as one just added for it, or null to let the user choose. */
  readonly app: string | null;
  /** The draft the flow finishes, or null. */
  readonly draft: Resumed | null;
  /** The way to this flow, for its breadcrumb. */
  readonly trail: readonly string[];
  /** Whether the flow shows because another flow opened it or ended, so the focus has to land somewhere. */
  readonly continued: boolean;
  readonly onCancel: () => void;
  /** Hears where the reference was saved. */
  readonly onSaved: (made: Made) => void;
  /** Closes the flow once its draft is saved or deleted. */
  readonly onDrafted: () => void;
  /** Leaves to add an application, and comes back with it chosen, or with `chosen` when that is canceled. */
  readonly onAddApplication: (chosen: string | null) => void;
  /** Leaves to change how `app` is found, and comes back to it. */
  readonly onEditApplication: (app: AppSummary) => void;
  /** The saved applications that have a draft of changes. */
  readonly draftedApps: ReadonlySet<string>;
  /** Hears what leaving the flow would lose. */
  readonly onHeld: (held: Held) => void;
  /** Hears how to save the flow's draft. */
  readonly onDraft: ReportDraft;
}

/** Making or changing a reference: choose its application, then capture its window and teach it the characters. */
export function ReferenceFlow({
  existing,
  app,
  draft,
  trail,
  continued,
  onCancel,
  onSaved,
  onDrafted,
  onAddApplication,
  onEditApplication,
  draftedApps,
  onHeld,
  onDraft,
}: Props) {
  const [choice, setChoice] = useState<Choice | null>(null);
  const [picking, setPicking] = useState(true);
  const [restored, setRestored] = useState<Restored | null>(null);
  // Whether a window was picked before: only the first pick brings a draft's screenshots back.
  const [pickedBefore, setPickedBefore] = useState(false);
  const [mode] = useState<CaptureMode>({ kind: 'reference', existing: existing?.reference ?? null });
  // The flow holds the window's capture, which would otherwise keep copying frames nobody looks at.
  const stop = useCapture();
  if (picking || choice === null) {
    return (
      <Picker
        heading={existing === null ? 'New reference' : `Edit ${existing.reference.name}`}
        trail={trail}
        continued={continued}
        lockedApp={existing?.app ?? draft?.def.app ?? null}
        startApp={choice?.app ?? app}
        startWindow={choice?.window.handle ?? null}
        onCancel={onCancel}
        onAddApplication={onAddApplication}
        onEditApplication={onEditApplication}
        draftedApps={draftedApps}
        onPick={async (picked) => {
          // Picking the window forgets the session's screenshots, so a draft's come back after it, the first time.
          if (draft !== null) {
            setRestored({ screenshots: pickedBefore ? null : await restoreDraftScreenshots(draft.id) });
          }
          setPickedBefore(true);
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
      resumed={draft}
      restored={restored}
      trail={trail}
      onCancel={onCancel}
      onChangeWindow={() => {
        // The capture ends here, not when another window is picked, so nothing keeps its screenshots meanwhile.
        stop();
        setPicking(true);
      }}
      onSaved={onSaved}
      onDrafted={onDrafted}
      onHeld={onHeld}
      onDraft={onDraft}
    />
  );
}
