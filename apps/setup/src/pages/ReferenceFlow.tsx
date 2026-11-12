import { useEffect, useState } from 'react';

import type { AppSummary, ReferenceDef } from '../lib/bindings.ts';
import { stopCapture } from '../lib/api.ts';
import { Capture, type CaptureMode } from '../components/Capture.tsx';
import { type Choice, Picker } from '../components/Picker.tsx';
import { background } from '../lib/repeat.ts';
import type { Held } from '../lib/unsaved.ts';

/** A saved reference and the application it belongs to. */
export interface SavedRef {
  readonly app: string;
  readonly reference: ReferenceDef;
}

interface Props {
  readonly apps: readonly AppSummary[];
  /** The reference to change, or null to make one. */
  readonly existing: SavedRef | null;
  /** The application a new reference starts with, such as one just added for it, or null to let the user choose. */
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

/** Making or changing a reference: choose its application, then capture its window and teach it the characters. */
export function ReferenceFlow({
  apps,
  existing,
  app,
  onCancel,
  onSaved,
  onAddApplication,
  onEditApplication,
  onHeld,
}: Props) {
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
