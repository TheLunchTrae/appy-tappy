// Reviewing a recording's readings before they teach a reference.

import { type Dispatch, type SetStateAction, useEffect, useMemo, useRef, useState } from 'react';

import type { RecordedLabel, RecordingView, ReferenceRef } from './bindings.ts';
import { errorText, reviewProblems } from './api.ts';
import { type Draft, reviewSpec } from './check.ts';
import {
  type ReviewEntry,
  type Untaught,
  checkedReadings,
  decide,
  disagreements,
  editAll,
  entries,
  flaggedReadings,
  keepBlocked,
  keptAdditions,
  keptLabels,
  untaught,
} from './review.ts';

/** How long review waits after the last edit before it asks which readings the check cannot learn. */
const REVIEW_DELAY_MS = 200;

/** Moves focus to the inspector's recording part once it shows again, after the button that had focus went. */
function focusRecordPanel(): void {
  requestAnimationFrame(() => {
    document.getElementById('record-title')?.focus();
  });
}

/**
 * Review of `recording`'s readings for the reference `draft` teaches, which adds to `reference`, if any, and needs
 * the characters `needs`: the user's edits, and which kept readings the check cannot learn. Keep puts the kept
 * readings on the draft.
 */
export function useReview(
  recording: RecordingView | null,
  draft: Draft,
  setDraft: Dispatch<SetStateAction<Draft>>,
  reference: ReferenceRef | null,
  needs: readonly string[],
): {
  readonly reviewing: boolean;
  /** Opens or closes review, leaving focus where it is. */
  readonly show: (reviewing: boolean) => void;
  /** Leaves review without keeping, and moves focus to the recording part; its edits stay for the next review. */
  readonly close: () => void;
  /** Each reading's entry: the user's edit where there is one, or its start. */
  readonly entries: readonly ReviewEntry[];
  /** The readings the user has checked, where the rest of what review keeps is the text reader's guess. */
  readonly checked: ReadonlySet<number>;
  /** For each reading, the places in it of the shapes no other kept reading teaches. */
  readonly adds: readonly (readonly number[])[];
  /** The readings whose text and another's give one shape two characters, each by its index with what to say. */
  readonly clashes: ReadonlyMap<number, string>;
  /** The characters only readings that are left out could teach, and those readings. */
  readonly untaught: Untaught;
  /** The readings Keep would keep, with their text. */
  readonly keeping: readonly RecordedLabel[];
  /** Why the check cannot learn each flagged reading, by its index. */
  readonly flagged: ReadonlyMap<number, string>;
  /** Why Keep waits, or null. */
  readonly keepBlocked: string | null;
  /** Why the readings could not be checked, until they next are or the recording is discarded. */
  readonly error: string | null;
  readonly edit: (index: number, entry: ReviewEntry) => void;
  /** Keep all, or with false Keep none. */
  readonly keepAll: (keep: boolean) => void;
  /** Puts the kept readings on the draft and closes review. Its edits stay, so reviewing again starts from them. */
  readonly keep: () => void;
  /** Forgets the edits and the kept readings, for a new recording. */
  readonly forget: () => void;
  /** Forgets everything review holds once the recording is discarded, and moves focus to the recording part. */
  readonly discarded: () => void;
} {
  const [reviewing, setReviewing] = useState(false);
  const [edits, setEdits] = useState<ReadonlyMap<number, ReviewEntry>>(new Map());
  // Each reading's start, decided once review shows it with its text read, so its Keep changes only by the user.
  const [decided, setDecided] = useState<ReadonlyMap<number, ReviewEntry>>(new Map());
  // Why the check cannot learn each flagged reading, for the review spec they were found on.
  const [problems, setProblems] = useState<{ readonly spec: string; readonly found: ReadonlyMap<number, string> }>({
    spec: '',
    found: new Map(),
  });
  // Clears only when asking review's problems next succeeds, or on a discard.
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (reviewing) {
      setDecided((current) => decide(current, recording?.readings ?? [], edits, draft.recorded, needs));
    }
  }, [reviewing, recording, edits, draft.recorded, needs]);
  const all = useMemo(
    () => entries(recording?.readings ?? [], edits, draft.recorded, decided, needs),
    [recording, edits, draft.recorded, decided, needs],
  );
  const keeping = useMemo(() => keptLabels(all), [all]);
  const userChecked = useMemo(() => checkedReadings(all, edits, draft.recorded), [all, edits, draft.recorded]);
  const adds = useMemo(() => keptAdditions(recording?.readings ?? [], all), [recording, all]);
  const clashes = useMemo(() => disagreements(recording?.readings ?? [], all), [recording, all]);
  const left = useMemo(() => untaught(recording?.readings ?? [], all, adds, needs), [recording, all, adds, needs]);
  const toReview = useMemo(() => reviewSpec(draft, keeping, reference), [draft, keeping, reference]);
  const reviewKey = toReview === null ? '' : JSON.stringify(toReview);
  // The effect below runs when the spec's contents change, not on each new
  // object with the same contents, so it reads the spec through a ref.
  const reviewSpecRef = useRef(toReview);
  reviewSpecRef.current = toReview;
  // Asks which kept readings the check cannot learn, such as one whose shape
  // a box or an earlier reading gave to another character.
  useEffect(() => {
    const toReview = reviewSpecRef.current;
    if (!reviewing || toReview === null) {
      return undefined;
    }
    let current = true;
    const timer = setTimeout(() => {
      reviewProblems(toReview)
        .then((answers) => {
          if (!current) {
            return;
          }
          setProblems({ spec: reviewKey, found: flaggedReadings(toReview.recorded, answers) });
          setError(null);
        })
        .catch((error: unknown) => {
          if (current) {
            // Saving checks the readings again, so a failed check flags none
            // rather than leaving Keep waiting for an answer that never comes.
            setProblems({ spec: reviewKey, found: new Map() });
            setError(errorText(error));
          }
        });
    }, REVIEW_DELAY_MS);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [reviewing, reviewKey]);
  // Flags found on another spec are stale: none show, and Keep waits for the answer.
  const checked = problems.spec === reviewKey;
  const flagged = checked ? problems.found : new Map<number, string>();

  const close = (): void => {
    if (!reviewing) {
      return;
    }
    setReviewing(false);
    focusRecordPanel();
  };

  const forget = (): void => {
    setEdits(new Map());
    setDecided(new Map());
    setDraft((current) => ({ ...current, recorded: new Map() }));
  };

  return {
    reviewing,
    show: setReviewing,
    close,
    entries: all,
    checked: userChecked,
    adds,
    clashes,
    untaught: left,
    keeping,
    flagged,
    keepBlocked: keepBlocked(keeping, flagged, checked),
    error,
    edit: (index, entry) => {
      setEdits((current) => new Map(current).set(index, entry));
    },
    keepAll: (keep) => {
      setEdits((current) => editAll(current, all, recording?.readings ?? [], keep));
    },
    keep: () => {
      setDraft((current) => ({ ...current, recorded: new Map(keeping.map((label) => [label.reading, label.text])) }));
      close();
    },
    forget,
    discarded: () => {
      setError(null);
      setReviewing(false);
      // Discarding removes the button that had focus, wherever it was.
      focusRecordPanel();
      forget();
    },
  };
}
