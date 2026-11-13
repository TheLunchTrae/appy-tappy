// The check of a reference's characters, one at a time, over the finds and the recorded readings it would learn from.

import { useMemo, useState } from 'react';

import type { FoundView, RecordingView } from './bindings.ts';
import { type Change, type Guide, type Place, type Turn, changes, openGuide, settle, turn } from './confirm.ts';
import { type FoundRow, listed } from './found.ts';
import type { Picture } from './pixels.ts';
import type { ReviewEntry } from './review.ts';
import type { Finder } from './useFound.ts';
import type { useRecording } from './useRecording.ts';
import type { useReview } from './useReview.ts';

const NO_SHAPES: ReadonlySet<string> = new Set();

/**
 * The places a check goes over: every find listed, ticked or not, and every recorded reading that review keeps. A
 * reading review leaves out shows only characters the kept ones show, so it has nothing to ask about.
 */
export function checkPlaces(
  found: FoundView['finds'],
  rows: readonly FoundRow[],
  recorded: RecordingView['readings'],
  entries: readonly ReviewEntry[],
): readonly Place[] {
  const finds = listed(rows).flatMap((row): Place[] => {
    const find = found[row.index];
    return find === undefined
      ? []
      : [
          {
            source: 'find',
            index: row.index,
            characters: find.characters,
            width: find.region.width,
            text: row.text,
            keep: row.keep,
          },
        ];
  });
  const readings = recorded.flatMap((reading, index): Place[] => {
    const entry = entries[index];
    return entry === undefined || !entry.keep
      ? []
      : [
          {
            source: 'reading',
            index,
            characters: reading.characters,
            width: reading.width,
            text: entry.text,
            keep: true,
          },
        ];
  });
  return [...finds, ...readings];
}

/** The check of the characters `finder`'s finds and `review`'s kept readings show, as the capture screen runs it. */
export interface CharacterCheck {
  /** The places it goes over, whether or not it is open. */
  readonly places: readonly Place[];
  /** The open check, where it can be for the places as they are now, or null while it is closed. */
  readonly guide: Guide | null;
  readonly open: () => void;
  /** Closes it and forgets its answers; the texts they wrote stay. */
  readonly close: () => void;
  readonly act: (event: Turn) => void;
  /** Edits one place's text and tick by hand, as its row in the list of finds or its tile in review does. */
  readonly edit: (place: Place, entry: ReviewEntry) => void;
  /** A place's pixels, once they arrive. */
  readonly picture: (place: Place) => Picture | undefined;
}

export function useCharacterCheck(
  finder: Finder,
  review: ReturnType<typeof useReview>,
  recorder: ReturnType<typeof useRecording>,
  needs: readonly string[],
): CharacterCheck {
  const [held, setHeld] = useState<Guide | null>(null);
  const places = useMemo(
    () => checkPlaces(finder.view?.finds ?? [], finder.rows, recorder.view?.readings ?? [], review.entries),
    [finder.view, finder.rows, recorder.view, review.entries],
  );
  const guide = held === null ? null : settle(held, places, needs);

  const edit = (place: Place, entry: ReviewEntry): void => {
    if (place.source === 'reading') {
      review.edit(place.index, entry);
      return;
    }
    const row = finder.rows.find((each) => each.index === place.index);
    // Text the check wrote is the user's answer, so it counts as typed.
    finder.edit(place.index, { ...entry, typed: (row?.typed ?? false) || entry.text !== place.text });
  };
  const apply = (edits: readonly Change[]): void => {
    for (const change of edits) {
      const place = places.find((each) => each.source === change.source && each.index === change.index);
      if (place !== undefined) {
        edit(place, change);
      }
    }
  };

  return {
    places,
    guide,
    open: () => {
      setHeld(openGuide(places, needs));
    },
    close: () => {
      // A text that arrived after the last answer still gets the answers.
      if (guide !== null) {
        apply(changes(places, guide.answers, NO_SHAPES));
      }
      setHeld(null);
    },
    act: (event) => {
      if (guide === null) {
        return;
      }
      const next = turn(guide, event, places, needs);
      apply(next.changes);
      setHeld(next.guide);
    },
    edit,
    picture: (place) => (place.source === 'find' ? finder.crops : recorder.crops).get(place.index),
  };
}
