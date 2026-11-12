import { useEffect, useState } from 'react';

import type { AppSummary, ValueDef } from '../lib/bindings.ts';
import { restoreDraftScreenshots, stopCapture } from '../lib/api.ts';
import { Capture, type CaptureMode, type Restored } from '../components/Capture.tsx';
import { type Choice, Picker } from '../components/Picker.tsx';
import { background } from '../lib/repeat.ts';
import type { Held } from '../lib/unsaved.ts';
import type { ReportDraft, Resumed } from '../lib/drafts.ts';

const VALUE = { kind: 'value' } as const;

/** A saved value to draw again at the window's size now, and its application's ID. */
export interface Fixing {
  readonly app: string;
  readonly value: ValueDef;
}

interface Props {
  readonly apps: readonly AppSummary[];
  /** The saved value to fix at the window's size now, or null to make a new value. */
  readonly fixing: Fixing | null;
  /** The draft the flow finishes, or null. */
  readonly draft: Resumed | null;
  readonly onCancel: () => void;
  readonly onSaved: () => void;
  readonly onAddApplication: () => void;
  /** Hears what leaving the flow would lose. */
  readonly onHeld: (held: Held) => void;
  /** Hears how to save the flow's draft. */
  readonly onDraft: ReportDraft;
}

/** Making a value, or fixing a saved one at another window size: pick the window, then capture and check what to
 * read. */
export function NewValue({ apps, fixing, draft, onCancel, onSaved, onAddApplication, onHeld, onDraft }: Props) {
  const [choice, setChoice] = useState<Choice | null>(null);
  const [picking, setPicking] = useState(true);
  const [restored, setRestored] = useState<Restored | null>(null);
  const [mode] = useState<CaptureMode>(fixing === null ? VALUE : { kind: 'fix', value: fixing.value });
  // Leaving the flow stops the window's capture, which would otherwise keep
  // copying frames nobody looks at.
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
        heading={fixing === null ? 'New value' : `Fix ${fixing.value.name} at this size`}
        lockedApp={fixing?.app ?? draft?.def.app ?? null}
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
