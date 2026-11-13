import { useState } from 'react';

import type { AppSummary } from '../lib/bindings.ts';
import { restoreDraftScreenshots } from '../lib/api.ts';
import { useCapture } from '../lib/capture.ts';
import { Capture, type CaptureMode, type Restored } from '../components/Capture.tsx';
import { type Choice, Picker } from '../components/Picker.tsx';
import type { Changing, Made, RefFlow, ValueFlow } from '../lib/flow.ts';
import type { Held } from '../lib/unsaved.ts';
import type { ReportDraft } from '../lib/drafts.ts';

interface Props {
  /**
   * What the flow makes or changes: a new value, a saved value to fix or edit, or a reference. Its application is
   * the one a new value or reference starts with, such as one just added for it, and its draft the one it finishes.
   */
  readonly flow: ValueFlow | RefFlow;
  /** The way to this flow, for its breadcrumb. */
  readonly trail: readonly string[];
  /** Whether the flow shows because another flow opened it or ended, so the focus has to land somewhere. */
  readonly continued: boolean;
  readonly onCancel: () => void;
  /** Hears where the value was saved, or the reference's application. */
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

/** How the capture runs for `flow`, the saved application it is locked to, and the picker's heading. */
function subject(flow: ValueFlow | RefFlow): {
  readonly mode: CaptureMode;
  readonly lockedApp: string | null;
  readonly heading: string;
} {
  if (flow.kind === 'reference') {
    const { existing } = flow;
    return {
      mode: { kind: 'reference', existing: existing?.reference ?? null },
      lockedApp: existing?.app ?? null,
      heading: existing === null ? 'New reference' : `Edit ${existing.reference.name}`,
    };
  }
  const { changing } = flow;
  return changing === null
    ? { mode: { kind: 'value' }, lockedApp: null, heading: 'New value' }
    : {
        mode: { kind: changing.how, value: changing.value },
        lockedApp: changing.app,
        heading: HEADINGS[changing.how](changing.value.name),
      };
}

/**
 * Making a value, fixing a saved one at another window size, editing one, or making or changing a reference: choose
 * the application, then capture its window and check what to read or teach the reference its characters.
 */
export function CaptureFlow({
  flow,
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
  const draft = flow.resumed;
  const [choice, setChoice] = useState<Choice | null>(null);
  const [picking, setPicking] = useState(true);
  const [restored, setRestored] = useState<Restored | null>(null);
  // Whether a window was picked before: only the first pick brings a draft's screenshots back.
  const [pickedBefore, setPickedBefore] = useState(false);
  // What the flow started for: its mode stays the same object while it shows.
  const [{ mode, lockedApp, heading }] = useState(() => subject(flow));
  // The flow holds the window's capture, which would otherwise keep copying frames nobody looks at.
  const stop = useCapture();
  if (picking || choice === null) {
    return (
      <Picker
        heading={heading}
        trail={trail}
        continued={continued}
        lockedApp={lockedApp ?? draft?.def.app ?? null}
        startApp={choice?.app ?? flow.app}
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
