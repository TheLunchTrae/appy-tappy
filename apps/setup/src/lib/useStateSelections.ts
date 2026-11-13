// The selections a state value's steps make on their screenshots, and the region they fit.

import { type Dispatch, type SetStateAction, useEffect, useMemo, useRef, useState } from 'react';

import { LIMITS, type SelectStep } from './bindings.ts';
import { errorText, select } from './api.ts';
import { type Draft, stateOn, withStateStep } from './check.ts';
import { ask } from './request.ts';
import { type Fitted, type Masking, type Selection, ignoredPixels, keptCount, madeFor, refit } from './selection.ts';

/** How long a selection waits after the last change before it is made, so a dragged slider asks once. */
const SELECT_DELAY_MS = 80;

/**
 * Makes the selection each of `draft`'s states' steps describe on its screenshot, and fits the region around what
 * every state selects. `selected` is the screenshot shown, or null while the live view shows.
 */
export function useStateSelections(
  draft: Draft,
  setDraft: Dispatch<SetStateAction<Draft>>,
  selected: number | null,
): {
  /** What each state's selection leaves of the region, by the screenshot it is taken from. */
  readonly masks: ReadonlyMap<number, Masking>;
  /** Which pixels of the region the selection leaves out on the screenshot shown, or null. */
  readonly ignored: Uint8Array | null;
  /** The selection last made on each screenshot, by its index. */
  readonly selections: ReadonlyMap<number, Selection>;
  readonly error: string | null;
  /** Adds a step on the screenshot shown, to its state or to a new one. */
  readonly addStep: (step: SelectStep) => void;
} {
  // The selection each state's steps last made, by the screenshot it is taken from.
  const [selections, setSelections] = useState<ReadonlyMap<number, Selection>>(new Map());
  const [error, setError] = useState<string | null>(null);
  const { states, colorMatch } = draft;
  // The box the region was last fitted to, and the steps it came from, so a
  // new color match refits a region the user has not changed by hand, and new
  // steps refit it either way.
  const fitted = useRef<Fitted | null>(null);
  const shownRegion = useRef(draft.region);
  shownRegion.current = draft.region;
  useEffect(() => {
    const stale = states.filter((state) => state.steps.length > 0 && madeFor(selections, state, colorMatch) === null);
    if (stale.length === 0) {
      return undefined;
    }
    return ask(
      async () =>
        Promise.all(
          stale.map(async (state) => ({
            state,
            view: await select(state.screenshot, { tolerance: colorMatch, steps: state.steps }),
          })),
        ),
      (made) => {
        setError(null);
        const next = new Map(selections);
        for (const { state, view } of made) {
          next.set(state.screenshot, { steps: state.steps, colorMatch, view });
        }
        setSelections(next);
        const every = states.flatMap((state) => {
          const selection = madeFor(next, state, colorMatch);
          return selection === null ? [] : [selection];
        });
        const fit = refit(every, fitted.current, shownRegion.current);
        fitted.current = fit.fitted;
        const { region } = fit;
        if (region !== null) {
          const on = made[0]?.state.screenshot ?? null;
          setDraft((latest) => ({ ...latest, region, drawnOn: latest.drawnOn ?? on }));
        }
      },
      (error) => {
        setError(errorText(error));
      },
      SELECT_DELAY_MS,
    );
  }, [states, colorMatch, selections, setDraft]);

  const { masks, ignored } = useMemo(() => {
    const masks = new Map<number, Masking>();
    let ignored: Uint8Array | null = null;
    for (const state of states) {
      const made = madeFor(selections, state, colorMatch);
      const view = made?.view ?? null;
      const left =
        draft.ignoreBackground && view !== null && draft.region !== null ? ignoredPixels(view, draft.region) : null;
      masks.set(state.screenshot, {
        found: made === null || view !== null,
        kept: left === null ? null : keptCount(left),
      });
      if (state.screenshot === selected && draft.kind === 'state') {
        ignored = left;
      }
    }
    return { masks, ignored };
  }, [states, selections, colorMatch, draft.ignoreBackground, draft.region, draft.kind, selected]);

  const addStep = (step: SelectStep): void => {
    if (selected === null) {
      return;
    }
    if (stateOn(draft, selected) === -1 && draft.states.length >= LIMITS.maxStates) {
      setError(`A value can tell apart at most ${LIMITS.maxStates} states. Remove one to select on this screenshot.`);
      return;
    }
    setError(null);
    setDraft((current) => withStateStep(current, selected, step));
  };
  return { masks, ignored, selections, error, addStep };
}
