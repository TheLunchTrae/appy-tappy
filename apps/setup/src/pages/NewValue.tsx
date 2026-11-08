import { useEffect, useState } from 'react';

import { type AppSummary, stopCapture } from '../lib/api.ts';
import { Capture } from '../components/Capture.tsx';
import { type Choice, Picker } from '../components/Picker.tsx';
import { background } from '../lib/repeat.ts';
import type { Held } from '../lib/unsaved.ts';

const VALUE = { kind: 'value' } as const;

interface Props {
  readonly apps: readonly AppSummary[];
  readonly onCancel: () => void;
  readonly onSaved: () => void;
  readonly onAddApplication: () => void;
  /** Hears what leaving the flow would lose. */
  readonly onHeld: (held: Held) => void;
}

/** Making a value: pick the window, then capture and check what to read. */
export function NewValue({ apps, onCancel, onSaved, onAddApplication, onHeld }: Props) {
  const [choice, setChoice] = useState<Choice | null>(null);
  const [picking, setPicking] = useState(true);
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
        heading="New value"
        lockedApp={null}
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
      mode={VALUE}
      onCancel={onCancel}
      onChangeWindow={() => {
        setPicking(true);
      }}
      onSaved={onSaved}
      onHeld={onHeld}
    />
  );
}
