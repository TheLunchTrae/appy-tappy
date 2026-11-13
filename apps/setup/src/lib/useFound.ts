// Finding more text like a box a reference learns from, and adding what the user confirms to the draft as boxes.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import type { FoundView, ReferenceRef } from './bindings.ts';
import { errorText, findLike, forgetFound, foundPicture, foundProblems, foundState, rereadFound } from './api.ts';
import { type Box, type NewBox, boxesOn } from './boxes.ts';
import { type Draft, taughtSpec } from './check.ts';
import {
  type FoundEntry,
  type FoundRow,
  addBlocked,
  askable,
  asLabels,
  flaggedFinds,
  foundRows,
  keepAll,
  keptFound,
  rereadOffer,
  rowFlags,
  withoutFlagged,
} from './found.ts';
import type { Picture } from './pixels.ts';
import { background, useRepeat } from './repeat.ts';

/** How long the search's state waits between reads while text is still being read. */
const FOUND_PAUSE_MS = 250;
/** How long it waits after the last edit before it asks which finds the reference cannot learn. */
const CHECK_DELAY_MS = 200;
/** The widest a find is asked for: text is small, and the panel shows it a little larger. */
const CROP_WIDTH = 320;

/** The search for more text like a box, as {@link useFound} runs it. */
export interface Finder {
  readonly view: FoundView | null;
  /** The finds, with the user's edits. */
  readonly rows: readonly FoundRow[];
  /** Each kept find's flag, by its index. */
  readonly flags: ReadonlyMap<number, string>;
  readonly crops: ReadonlyMap<number, Picture>;
  /** Why Add waits, or null. */
  readonly blocked: string | null;
  /** Why the last search, or the last check of the finds, failed. */
  readonly error: string | null;
  readonly searching: boolean;
  /** Searches the screenshot `sample` is on for text like it, away from the boxes there already. */
  readonly search: (sample: Box) => Promise<void>;
  readonly edit: (index: number, entry: FoundEntry) => void;
  /** Ticks every listed find, or with `keep` false leaves every one out. */
  readonly keepAll: (keep: boolean) => void;
  /** Leaves out every ticked find that is flagged. */
  readonly leaveOutFlagged: () => void;
  /**
   * Whether Reread is offered, and with `blocked` why it waits: every find it would read again was last read with
   * the characters' color and tolerance the draft has now.
   */
  readonly rereadOffer: { readonly blocked: string | null } | null;
  /**
   * Has the text reader read again, with the characters' color as the draft has it now, the finds it could not read
   * and the flagged ones whose text is its guess and was read with another. Ticks and typed text stay.
   */
  readonly reread: () => Promise<void>;
  /** Adds the kept finds to the draft and forgets the search. */
  readonly add: () => void;
  /** Forgets the search without adding any of it. */
  readonly dismiss: () => void;
}

/**
 * The search for text like a box of the reference `draft` teaches, which adds to `reference`, if any: its finds with
 * the user's edits, their pictures, and which ones the reference cannot learn. `addBoxes` adds the kept finds to the
 * draft as boxes. `learned` is every text the reference learns from already, and `needs` the characters it needs,
 * which the finds listed may only show. Leaving the component that uses it forgets the search.
 */
export function useFound(
  draft: Draft,
  addBoxes: (added: readonly NewBox[]) => void,
  reference: ReferenceRef | null,
  learned: readonly string[],
  needs: readonly string[],
): Finder {
  const [view, setView] = useState<FoundView | null>(null);
  const [edits, setEdits] = useState<ReadonlyMap<number, FoundEntry>>(new Map());
  const [error, setError] = useState<string | null>(null);
  const [searching, setSearching] = useState(false);
  // Bumped by each search, reread and dismissal, so an answer sent before one is ignored.
  const turn = useRef(0);
  const [crops, setCrops] = useState<{ readonly id: number; readonly pictures: ReadonlyMap<number, Picture> }>({
    id: -1,
    pictures: new Map(),
  });
  const [problems, setProblems] = useState<{ readonly key: string; readonly found: ReadonlyMap<number, string> }>({
    key: '',
    found: new Map(),
  });

  useEffect(
    () => () => {
      background(forgetFound());
    },
    [],
  );

  const poll = useCallback(async (): Promise<void> => {
    const sent = turn.current;
    try {
      const state = await foundState();
      if (sent === turn.current) {
        setView(state);
      }
    } catch (error: unknown) {
      if (sent === turn.current) {
        setError(errorText(error));
      }
    }
  }, []);
  const reading = view?.finds.some((find) => find.text.kind === 'pending') ?? false;
  useRepeat(poll, FOUND_PAUSE_MS, reading);

  // Fetches each find's pixels once per search.
  const searchId = view?.id ?? null;
  const findCount = view?.finds.length ?? 0;
  useEffect(() => {
    if (searchId === null) {
      return;
    }
    for (let index = 0; index < findCount; index += 1) {
      background(
        foundPicture(searchId, index, CROP_WIDTH).then((picture) => {
          if (picture === null) {
            return;
          }
          setCrops((current) => {
            const pictures = current.id === searchId ? new Map(current.pictures) : new Map<number, Picture>();
            return { id: searchId, pictures: pictures.set(index, picture) };
          });
        }),
      );
    }
  }, [searchId, findCount]);

  const rows = useMemo(
    () => (view === null ? [] : foundRows(view, edits, learned, needs)),
    [view, edits, learned, needs],
  );
  const asked = useMemo(() => askable(rows), [rows]);
  const spec = useMemo(() => taughtSpec(draft, reference), [draft, reference]);
  const labels = useMemo(() => (view === null ? [] : asLabels(view.screenshot, asked)), [view, asked]);
  const key = spec === null || labels.length === 0 ? '' : JSON.stringify([spec, labels]);
  // The effect runs when the contents change, not on each new object with the same contents.
  const latest = useRef({ spec, labels, asked });
  latest.current = { spec, labels, asked };
  useEffect(() => {
    const { spec: toCheck, labels: toLearn, asked: rowsAsked } = latest.current;
    if (key === '' || toCheck === null) {
      return undefined;
    }
    let current = true;
    const timer = setTimeout(() => {
      foundProblems(toCheck, toLearn)
        .then((answers) => {
          if (current) {
            setProblems({ key, found: flaggedFinds(rowsAsked, answers) });
            setError(null);
          }
        })
        .catch((error: unknown) => {
          if (current) {
            // Saving learns the text again, so a failed check flags none
            // rather than leaving Add waiting for an answer that never comes.
            setProblems({ key, found: new Map() });
            setError(errorText(error));
          }
        });
    }, CHECK_DELAY_MS);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [key]);
  const checked = key === '' || problems.key === key;
  const flags = rowFlags(rows, checked ? problems.found : new Map());

  const offer = rereadOffer(rows, flags, draft.ink, draft.tolerance);

  const forget = (): void => {
    turn.current += 1;
    setView(null);
    setEdits(new Map());
    setError(null);
    background(
      forgetFound().catch((error: unknown) => {
        setError(errorText(error));
      }),
    );
  };

  const search = async (sample: Box): Promise<void> => {
    const { ink, tolerance, boxes } = draft;
    if (ink === null) {
      return;
    }
    const { screenshot, region } = sample;
    // The server leaves these out, so a search after adding offers the rest.
    const added = boxesOn(boxes, screenshot)
      .filter((box) => box.id !== sample.id)
      .map((box) => box.region);
    turn.current += 1;
    const sent = turn.current;
    setSearching(true);
    try {
      const result = await findLike(screenshot, region, added, ink, tolerance);
      if (sent === turn.current) {
        setView(result);
        setEdits(new Map());
        setError(null);
      }
    } catch (error: unknown) {
      if (sent === turn.current) {
        setView(null);
        setError(errorText(error));
      }
    } finally {
      setSearching(false);
    }
  };

  return {
    view,
    rows,
    flags,
    crops: crops.id === searchId ? crops.pictures : new Map<number, Picture>(),
    blocked: addBlocked(rows, flags, checked),
    error,
    searching,
    search,
    edit: (index, entry) => {
      setEdits((current) => new Map(current).set(index, entry));
    },
    keepAll: (keep) => {
      setEdits((current) => new Map([...current, ...keepAll(rows, keep)]));
    },
    leaveOutFlagged: () => {
      setEdits((current) => new Map([...current, ...withoutFlagged(rows, flags)]));
    },
    rereadOffer: offer,
    reread: async () => {
      const again = offer?.indexes ?? [];
      if (view === null || draft.ink === null || again.length === 0) {
        return;
      }
      // The search's state read before this would show the finds as they were, and end the wait for their text.
      turn.current += 1;
      const sent = turn.current;
      try {
        const result = await rereadFound(view.id, again, draft.ink, draft.tolerance);
        if (sent === turn.current && result !== null) {
          setView(result);
          setError(null);
        }
      } catch (error: unknown) {
        if (sent === turn.current) {
          setError(errorText(error));
        }
      }
    },
    add: () => {
      if (view === null) {
        return;
      }
      addBoxes(keptFound(view.screenshot, rows));
      forget();
    },
    dismiss: forget,
  };
}
