// The matching pixels preview's switch, and the requests it makes while it is on.

import { useEffect, useMemo, useReducer, useRef, useState } from 'react';

import { errorText, matchPixels } from './api.ts';
import {
  type Answered,
  NO_PREVIEW,
  type Plan,
  type PreviewDraft,
  type PreviewTarget,
  plan,
  step,
  tinted,
} from './matchPreview.ts';
import type { Size } from './region.ts';

/** How long a request waits after the last change before it is made, so a dragged slider asks once. */
const ASK_DELAY_MS = 80;

/** The preview as the capture screen shows it. */
export interface MatchPreview {
  /** The setting the preview is for, or null where no setting decides pixel by pixel. */
  readonly target: PreviewTarget | null;
  readonly on: boolean;
  readonly setOn: (on: boolean) => void;
  /** What it asks while it is on, or why it has nothing to show. */
  readonly plan: Plan | null;
  /** The answer to tint over the screenshot shown, or null. */
  readonly tint: Answered | null;
  readonly error: string | null;
}

/**
 * Asks which pixels `target`'s colors in `draft` take on the screenshot shown, `selected`, while the user has the
 * preview on. It is the capture screen's own state: it changes neither the draft nor the screenshot. Turning it on
 * is for one target, so another target's preview starts off.
 */
export function useMatchPreview(
  target: PreviewTarget | null,
  draft: PreviewDraft,
  selected: number | null,
  size: Size | null,
): MatchPreview {
  const [wanted, setWanted] = useState<PreviewTarget | null>(null);
  const [preview, dispatch] = useReducer(step, NO_PREVIEW);
  const asked = useRef(0);
  const on = target !== null && wanted === target;
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
    if (request === null) {
      dispatch({ kind: 'cleared' });
      return undefined;
    }
    asked.current += 1;
    const id = asked.current;
    dispatch({ kind: 'asked', id });
    const timer = setTimeout(() => {
      matchPixels(request.screenshot, request.area, request.rule)
        .then((view) => {
          dispatch({ kind: 'answered', id, request, view });
        })
        .catch((error: unknown) => {
          dispatch({ kind: 'failed', id, message: errorText(error) });
        });
    }, ASK_DELAY_MS);
    return () => {
      clearTimeout(timer);
    };
  }, [request]);
  return {
    target,
    on,
    setOn: (next) => {
      setWanted(next ? target : null);
    },
    plan: planned,
    tint: tinted(preview, request),
    error: request === null ? null : preview.error,
  };
}
