import { useEffect, useState } from 'react';

import type { AppSummary, ValueDef } from '../lib/bindings.ts';
import { stopCapture } from '../lib/api.ts';
import { Capture, type CaptureMode } from '../components/Capture.tsx';
import { type Choice, Picker } from '../components/Picker.tsx';
import { background } from '../lib/repeat.ts';
import type { Held } from '../lib/unsaved.ts';

const VALUE = { kind: 'value' } as const;

/** A saved value to change, and its application's ID. */
export interface Changing {
  readonly app: string;
  readonly value: ValueDef;
  /** Whether it is drawn again at the window's size now, as a fix, or edited as it was first drawn. */
  readonly how: 'fix' | 'edit';
}

interface Props {
  readonly apps: readonly AppSummary[];
  /** The saved value to fix or edit, or null to make a new value. */
  readonly changing: Changing | null;
  readonly onCancel: () => void;
  readonly onSaved: () => void;
  readonly onAddApplication: () => void;
  /** Hears what leaving the flow would lose. */
  readonly onHeld: (held: Held) => void;
}

/** The picker's heading for each way a saved value changes. */
const HEADINGS: Readonly<Record<Changing['how'], (name: string) => string>> = {
  fix: (name) => `Fix ${name} at this size`,
  edit: (name) => `Edit ${name}`,
};

/** Making a value, fixing a saved one at another window size, or editing one: pick the window, then capture and
 * check what to read. */
export function NewValue({ apps, changing, onCancel, onSaved, onAddApplication, onHeld }: Props) {
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
        apps={apps}
        heading={changing === null ? 'New value' : HEADINGS[changing.how](changing.value.name)}
        lockedApp={changing?.app ?? null}
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
