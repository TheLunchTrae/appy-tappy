// The matching pixels preview's switch, and the requests it makes while it is on.

import { useEffect, useMemo, useState } from 'react';

import { errorText, matchPixels } from './api.ts';
import type { Draft } from './check.ts';
import {
  type Answered,
  NO_PREVIEW,
  type Painted,
  type Plan,
  type PreviewDraft,
  type PreviewTarget,
  type SelectedPixels,
  awaited,
  paintedOf,
  plan,
  previewDriver,
  selectedPixels,
  tinted,
} from './matchPreview.ts';
import type { Size } from './region.ts';
import type { Selection } from './selection.ts';

/** The preview as the capture screen shows it. */
export interface MatchPreview {
  /** The setting the preview is for, or null where no setting decides pixel by pixel. */
  readonly target: PreviewTarget | null;
  readonly on: boolean;
  readonly setOn: (on: boolean) => void;
  /** What it asks while it is on, or why it has nothing to show. */
  readonly plan: Plan | null;
  /** The answer the counts are of, or null. */
  readonly tint: Answered | null;
  /** That answer as the overlay tints it over the screenshot shown. */
  readonly painted: Painted | null;
  /** Whether the answer for the settings now is still awaited, so the tint and its counts are for earlier ones. */
  readonly updating: boolean;
  readonly error: string | null;
}

/**
 * Asks which pixels `target`'s colors in `draft` take on the screenshot shown, `selected`, while the user has the
 * preview on. It is the capture screen's own state: it changes neither the draft nor the screenshot. Each target has
 * its own switch, which stays as the user left it for as long as the capture screen is open.
 */
export function useMatchPreview(
  target: PreviewTarget | null,
  draft: PreviewDraft,
  selected: number | null,
  size: Size | null,
): MatchPreview {
  const [wanted, setWanted] = useState<ReadonlySet<PreviewTarget>>(new Set());
  const [preview, setPreview] = useState(NO_PREVIEW);
  const [want] = useState(() =>
    previewDriver(
      async (request) => matchPixels(request.screenshot, request.area, request.rule),
      setPreview,
      errorText,
    ),
  );
  const on = target !== null && wanted.has(target);
  const { ink, tolerance, filled, empty, color, region } = draft;
  const width = size?.width ?? null;
  const height = size?.height ?? null;
  const planned = useMemo(
    () =>
      on
        ? plan(
            target,
            { ink, tolerance, filled, empty, color, region },
            selected,
            width === null || height === null ? null : { width, height },
          )
        : null,
    [on, target, ink, tolerance, filled, empty, color, region, selected, width, height],
  );
  const request = planned?.kind === 'ask' ? planned.request : null;
  useEffect(() => {
    want(request);
  }, [want, request]);
  // Once the capture screen is gone, a queued request has no one to answer.
  useEffect(
    () => () => {
      want(null);
    },
    [want],
  );
  const tint = tinted(preview, request);
  const painted = useMemo(() => (tint === null ? null : paintedOf(tint)), [tint]);
  return {
    target,
    on,
    setOn: (next) => {
      if (target !== null) {
        setWanted((current) => {
          const targets = new Set(current);
          if (next) {
            targets.add(target);
          } else {
            targets.delete(target);
          }
          return targets;
        });
      }
    },
    plan: planned,
    tint,
    painted,
    updating: awaited(preview, request),
    error: request === null ? null : preview.error,
  };
}

/** The selected pixels preview as the capture screen shows it, under **Color match**. */
export interface SelectedPreview {
  /** Whether the state check compares only what is selected, and the screenshot shades what a selection leaves out. */
  readonly ignoresBackground: boolean;
  readonly on: boolean;
  readonly setOn: (on: boolean) => void;
  readonly selected: SelectedPixels;
  /** The selection's own pixels to tint over the screenshot shown, or null. */
  readonly painted: Painted | null;
}

/**
 * What **Show selected pixels** shows of the selection `draft` has on the screenshot shown, `selected`, from the
 * `selections` the capture screen made already, or tried to when `failed`. `offered` is whether **Color match** is in
 * the inspector; no tint shows without it. The switch stays as the user left it for as long as the capture screen is
 * open.
 */
export function useSelectedPreview(
  offered: boolean,
  draft: Pick<Draft, 'states' | 'colorMatch' | 'ignoreBackground'>,
  selections: ReadonlyMap<number, Selection>,
  selected: number | null,
  failed: boolean,
): SelectedPreview {
  const [on, setOn] = useState(false);
  const { states, colorMatch, ignoreBackground } = draft;
  const pixels = useMemo(
    () => selectedPixels(states, selections, selected, colorMatch, failed),
    [states, selections, selected, colorMatch, failed],
  );
  return {
    ignoresBackground: ignoreBackground,
    on,
    setOn,
    selected: pixels,
    painted: offered && on ? pixels.painted : null,
  };
}
