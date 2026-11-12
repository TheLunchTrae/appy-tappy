// The saved value the capture screen edits: the screenshot its region sits on, and the edit it tries and saves.

import { type Dispatch, type SetStateAction, useEffect, useMemo, useState } from 'react';

import type { EditSpec, PinDef, SizeDef, ValueDef } from './bindings.ts';
import { type Built, type Draft, newestOfSize } from './check.ts';
import { buildEdit } from './edit.ts';
import type { Size } from './region.ts';

interface Editing {
  /** The saved value, or null while the capture makes something else. */
  readonly value: ValueDef | null;
  /** Whether the value's own fields show, rather than a reference's. */
  readonly open: boolean;
  /** The saved application's ID. */
  readonly app: string;
  readonly draft: Draft;
  readonly setDraft: Dispatch<SetStateAction<Draft>>;
  /** The value's name as typed. */
  readonly name: string;
  readonly pin: PinDef;
  /** Each screenshot's window size, by its index. */
  readonly sizes: ReadonlyMap<number, Size>;
}

/**
 * Edits `value` with the capture screen's draft. Its region is in the pixels of the window size it was drawn at, so
 * the region sits on the newest screenshot of that size as soon as there is one, and on none until then.
 */
export function useValueEdit({ value, open, app, draft, setDraft, name, pin, sizes }: Editing): {
  /** The edit, or what it still needs; null while no saved value's own fields show. */
  readonly built: Built<EditSpec> | null;
  /** The window sizes of the fixes the user agreed to lose. */
  readonly lost: readonly SizeDef[];
  /** Agrees to lose the fixes at these window sizes, and no others. */
  readonly agree: (lost: readonly SizeDef[]) => void;
} {
  const drawn = value?.screen.windowSize ?? null;
  const unplaced = open && draft.drawnOn === null;
  useEffect(() => {
    if (drawn === null || !unplaced) {
      return;
    }
    const on = newestOfSize([...sizes], drawn);
    if (on !== null) {
      setDraft((current) => (current.drawnOn === null ? { ...current, drawnOn: on } : current));
    }
  }, [drawn, unplaced, sizes, setDraft]);

  const built = useMemo(
    () => (value === null || !open ? null : buildEdit(draft, value, app, name, pin)),
    [value, open, draft, app, name, pin],
  );
  const [lost, agree] = useState<readonly SizeDef[]>([]);
  return { built, lost, agree };
}
