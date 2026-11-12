import { useEffect, useState } from 'react';

import type { AppSummary } from '../lib/bindings.ts';
import { restoreDraftScreenshots, stopCapture } from '../lib/api.ts';
import { Capture, type CaptureMode, type Restored } from '../components/Capture.tsx';
import { type Choice, Picker } from '../components/Picker.tsx';
import type { Changing, Made } from '../lib/flow.ts';
import { background } from '../lib/repeat.ts';
import type { Held } from '../lib/unsaved.ts';
import type { ReportDraft, Resumed } from '../lib/drafts.ts';

const VALUE = { kind: 'value' } as const;

interface Props {
  /** The saved value to fix or edit, or null to make a new value. */
  readonly changing: Changing | null;
  /** The application a new value starts with, such as one just added for it, or null to let the user choose. */
  readonly app: string | null;
  /** The draft the flow finishes, or null. */
  readonly draft: Resumed | null;
  /** The way to this flow, for its breadcrumb. */
  readonly trail: readonly string[];
  /** Whether the flow shows because another flow opened it or ended, so the focus has to land somewhere. */
  readonly continued: boolean;
  readonly onCancel: () => void;
  /** Hears where the value was saved. */
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

/** The picker's heading for each way a saved value changes. */
const HEADINGS: Readonly<Record<Changing['how'], (name: string) => string>> = {
  fix: (name) => `Fix ${name} at this size`,
  edit: (name) => `Edit ${name}`,
};

/** Making a value, fixing a saved one at another window size, or editing one: choose the application, then capture
 * its window and check what to read. */
export function NewValue({
  changing,
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
  const [mode] = useState<CaptureMode>(changing === null ? VALUE : { kind: changing.how, value: changing.value });
  // Leaving the flow stops the window's capture, which would otherwise keep
  // copying frames nobody looks at, and its screenshots and recording go with it.
  useEffect(
    () => () => {
      background(stopCapture());
    },
    [],
  );
  if (picking || choice === null) {
    return (
      <Picker
        heading={changing === null ? 'New value' : HEADINGS[changing.how](changing.value.name)}
        trail={trail}
        continued={continued}
        lockedApp={changing?.app ?? draft?.def.app ?? null}
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
        background(stopCapture());
        setPicking(true);
      }}
      onSaved={onSaved}
      onDrafted={onDrafted}
      onHeld={onHeld}
      onDraft={onDraft}
    />
  );
}
