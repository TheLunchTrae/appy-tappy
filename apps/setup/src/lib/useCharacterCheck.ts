// The check of a reference's characters, one at a time, over the finds and the recorded readings it would learn from.

import { useEffect, useMemo, useState } from 'react';

import type { FoundView, RecordingView } from './bindings.ts';
import { type Change, type Guide, type Place, type Turn, openGuide, settle, turn } from './confirm.ts';
import { type FoundRow, listed } from './found.ts';
import type { Picture } from './pixels.ts';
import type { ReviewEntry } from './review.ts';
import type { Finder } from './useFound.ts';
import type { useRecording } from './useRecording.ts';
import type { useReview } from './useReview.ts';

/**
 * The places a check goes over: every find listed, ticked or not, and every recorded reading. It asks about the
 * finds and about the readings review keeps. A reading review leaves out shows only shapes the kept ones show, so it
 * has nothing to ask about, but it gets the answers too: review compares the texts of all the readings, and would
 * flag a kept reading the check fixed against a left-out one it did not.
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
            reading: row.reading,
            asks: true,
          },
        ];
  });
  const readings = recorded.flatMap((reading, index): Place[] => {
    const entry = entries[index];
    return entry === undefined
      ? []
      : [
          {
            source: 'reading',
            index,
            characters: reading.characters,
            width: reading.width,
            text: entry.text,
            keep: entry.keep,
            reading: reading.text.kind === 'pending' && entry.text === '',
            asks: entry.keep,
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
  /** Closes it and forgets its answers; the texts they wrote stay. Escape closes it too. */
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

  // Escape closes the check. It is taken as the window hears it, before review's own Escape, and marked as handled
  // with preventDefault, so review does not also close underneath the check. It still reaches every other listener,
  // such as a hint's.
  const open = held !== null;
  useEffect(() => {
    if (!open) {
      return undefined;
    }
    const onKey = (event: KeyboardEvent): void => {
      if (event.key === 'Escape' && !event.defaultPrevented) {
        event.preventDefault();
        setHeld(null);
      }
    };
    window.addEventListener('keydown', onKey, true);
    return () => {
      window.removeEventListener('keydown', onKey, true);
    };
  }, [open]);

  return {
    places,
    guide,
    open: () => {
      setHeld(openGuide(places, needs));
    },
    close: () => {
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
