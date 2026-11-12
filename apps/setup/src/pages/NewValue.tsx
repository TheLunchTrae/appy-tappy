import { useEffect, useState } from 'react';

import type { AppSummary } from '../lib/bindings.ts';
import { stopCapture } from '../lib/api.ts';
import { Capture, type CaptureMode } from '../components/Capture.tsx';
import { type Choice, Picker } from '../components/Picker.tsx';
import type { Changing, Made } from '../lib/flow.ts';
import { background } from '../lib/repeat.ts';
import type { Held } from '../lib/unsaved.ts';

const VALUE = { kind: 'value' } as const;

interface Props {
  /** The saved value to fix or edit, or null to make a new value. */
  readonly changing: Changing | null;
  /** The application a new value starts with, such as one just added for it, or null to let the user choose. */
  readonly app: string | null;
  /** The way to this flow, for its breadcrumb. */
  readonly trail: readonly string[];
  /** Whether the flow shows because another flow opened it or ended, so the focus has to land somewhere. */
  readonly continued: boolean;
  readonly onCancel: () => void;
  /** Hears where the value was saved. */
  readonly onSaved: (made: Made) => void;
  /** Leaves to add an application, and comes back with it chosen, or with `chosen` when that is canceled. */
  readonly onAddApplication: (chosen: string | null) => void;
  /** Leaves to change how `app` is found, and comes back to it. */
  readonly onEditApplication: (app: AppSummary) => void;
  /** Hears what leaving the flow would lose. */
  readonly onHeld: (held: Held) => void;
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
  trail,
  continued,
  onCancel,
  onSaved,
  onAddApplication,
  onEditApplication,
  onHeld,
}: Props) {
  const [choice, setChoice] = useState<Choice | null>(null);
  const [picking, setPicking] = useState(true);
  const [mode] = useState<CaptureMode>(changing === null ? VALUE : { kind: changing.how, value: changing.value });
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
        heading={changing === null ? 'New value' : HEADINGS[changing.how](changing.value.name)}
        trail={trail}
        continued={continued}
        lockedApp={changing?.app ?? null}
        startApp={choice?.app ?? app}
        startWindow={choice?.window.handle ?? null}
        onCancel={onCancel}
        onAddApplication={onAddApplication}
        onEditApplication={onEditApplication}
        onPick={(picked) => {
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
      trail={trail}
      onCancel={onCancel}
      onChangeWindow={() => {
        setPicking(true);
      }}
      onSaved={onSaved}
      onHeld={onHeld}
    />
  );
}
