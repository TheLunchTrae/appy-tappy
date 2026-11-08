import { useEffect, useState } from 'react';

import { type AppSummary, type ReferenceDef, stopCapture } from '../lib/api.ts';
import { Capture, type CaptureMode } from '../components/Capture.tsx';
import { type Choice, Picker } from '../components/Picker.tsx';
import { background } from '../lib/repeat.ts';

/** A saved reference and the application it belongs to. */
export interface SavedRef {
  readonly app: string;
  readonly reference: ReferenceDef;
}

interface Props {
  readonly apps: readonly AppSummary[];
  /** The reference to change, or null to make one. */
  readonly existing: SavedRef | null;
  readonly onCancel: () => void;
  readonly onSaved: () => void;
  /** Leaves to add an application, for when the one wanted is not saved yet. */
  readonly onAddApplication: () => void;
}

/** Making or changing a reference: pick its application's window, then teach it the characters. */
export function ReferenceFlow({ apps, existing, onCancel, onSaved, onAddApplication }: Props) {
  const [choice, setChoice] = useState<Choice | null>(null);
  const [picking, setPicking] = useState(true);
  const [mode] = useState<CaptureMode>({ kind: 'reference', existing: existing?.reference ?? null });
  // Leaving the flow stops the window's capture.
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
        heading={existing === null ? 'New reference' : `Edit ${existing.reference.name}`}
        lockedApp={existing?.app ?? null}
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
    />
  );
}
