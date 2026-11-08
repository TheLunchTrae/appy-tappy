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
  readonly onCancel: () => void;
  readonly onSaved: () => void;
  readonly onAddApplication: () => void;
  /** Hears what leaving the flow would lose. */
  readonly onHeld: (held: Held) => void;
}

/** Making a value, or fixing a saved one at another window size: pick the window, then capture and check what to
 * read. */
export function NewValue({ apps, fixing, onCancel, onSaved, onAddApplication, onHeld }: Props) {
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
        initial={choice}
        onCancel={onCancel}
        onAddApplication={onAddApplication}
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
