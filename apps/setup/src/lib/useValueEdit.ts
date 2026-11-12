// The saved value the capture screen edits: the screenshot its region sits on, and the edit it tries and saves.

import { type Dispatch, type SetStateAction, useCallback, useEffect, useMemo, useState } from 'react';

import { windowSize } from './api.ts';
import type { EditSpec, PinDef, SizeDef, ValueDef } from './bindings.ts';
import { type Built, type Draft, newestOfSize } from './check.ts';
import { buildEdit } from './edit.ts';
import type { Size } from './region.ts';
import { useRepeat } from './repeat.ts';
import { sameContent } from './unsaved.ts';

/** How long the edit waits between asking for the window's size. */
const WINDOW_PAUSE_MS = 500;

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
  /** The side it stays at, or null while there is no region to follow. */
  readonly pin: PinDef | null;
  /** Whether its region is a reading area. */
  readonly readingArea: boolean;
  /** Each screenshot's window size, by its index. */
  readonly sizes: ReadonlyMap<number, Size>;
}

/**
 * Edits `value` with the capture screen's draft. Its region is in the pixels of the window size it was drawn at, so
 * the region sits on the newest screenshot of that size as soon as there is one, and on none until then.
 */
export function useValueEdit({ value, open, app, draft, setDraft, name, pin, readingArea, sizes }: Editing): {
  /** The edit, or what it still needs; null while no saved value's own fields show. */
  readonly built: Built<EditSpec> | null;
  /** The window sizes of the fixes the user agreed to lose. */
  readonly lost: readonly SizeDef[];
  /** Agrees to lose the fixes at these window sizes, and no others. */
  readonly agree: (lost: readonly SizeDef[]) => void;
  /**
   * Changes when the edit has to be tried again though it is the same: the owner places it at the window's size
   * when it is tried, so the window's size is part of it, and so is each save the owner refused, whose reason the
   * next try shows. Null while the capture edits no saved value.
   */
  readonly again: string | null;
  /** Notes that the owner refused a save. */
  readonly refused: () => void;
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
    () => (value === null || !open ? null : buildEdit(draft, value, app, name, pin, readingArea)),
    [value, open, draft, app, name, pin, readingArea],
  );
  const [lost, agree] = useState<readonly SizeDef[]>([]);

  // The picked window's own size, as the owner's newest frame has it. The live view's size will not do: it is
  // scaled down, and stands still while a screenshot shows.
  const [size, setSize] = useState<SizeDef | null>(null);
  const measure = useCallback(async (): Promise<void> => {
    try {
      const now = await windowSize();
      setSize((current) => (sameContent(current, now) ? current : now));
    } catch {
      // The live view says why the window cannot be read; the size stays as it was last read.
    }
  }, []);
  useRepeat(measure, WINDOW_PAUSE_MS, value !== null);
  const [refusals, setRefusals] = useState(0);
  const refused = useCallback((): void => {
    setRefusals((count) => count + 1);
  }, []);
  const again = value === null ? null : `${size?.width ?? 0} × ${size?.height ?? 0}, ${refusals}`;
  return { built, lost, agree, again, refused };
}
