import { useEffect, useState } from 'react';

import type { AppSummary, ValueDef } from '../lib/bindings.ts';
import { stopCapture } from '../lib/api.ts';
import { Capture, type CaptureMode } from '../components/Capture.tsx';
import { type Choice, Picker } from '../components/Picker.tsx';
import { background } from '../lib/repeat.ts';
import type { Held } from '../lib/unsaved.ts';

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
  /** The application a new value starts with, such as one just added for it, or null to let the user choose. */
  readonly app: string | null;
  readonly onCancel: () => void;
  readonly onSaved: () => void;
  /** Leaves to add an application, and comes back with it chosen. */
  readonly onAddApplication: () => void;
  /** Leaves to change how the application with this ID is found, and comes back to it. */
  readonly onEditApplication: (app: string) => void;
  /** Hears what leaving the flow would lose. */
  readonly onHeld: (held: Held) => void;
}

/** Making a value, or fixing a saved one at another window size: choose the application, then capture its window
 * and check what to read. */
export function NewValue({ apps, fixing, app, onCancel, onSaved, onAddApplication, onEditApplication, onHeld }: Props) {
  const [choice, setChoice] = useState<Choice | null>(null);
  const [picking, setPicking] = useState(true);
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
        lockedApp={fixing?.app ?? null}
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
      onCancel={onCancel}
      onChangeWindow={() => {
        setPicking(true);
      }}
      onSaved={onSaved}
      onHeld={onHeld}
    />
  );
}
