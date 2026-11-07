import { useEffect, useState } from 'react';

import { type AppSummary, stopCapture } from './api.ts';
import { Capture } from './Capture.tsx';
import { type Choice, Picker } from './Picker.tsx';
import { background } from './repeat.ts';

interface Props {
  readonly apps: readonly AppSummary[];
  readonly onCancel: () => void;
  readonly onSaved: () => void;
}

/** Making a value: pick the window, then capture and check what to read. */
export function NewValue({ apps, onCancel, onSaved }: Props) {
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
        initial={choice}
        onCancel={onCancel}
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
      onCancel={onCancel}
      onChangeWindow={() => {
        setPicking(true);
      }}
      onSaved={onSaved}
    />
  );
}
