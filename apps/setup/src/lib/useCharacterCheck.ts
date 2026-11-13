// The check of a reference's characters, one at a time, over the boxes, the finds and the recorded readings it would
// learn from.

import { useEffect, useMemo, useState } from 'react';

import type { Box } from './boxes.ts';

import type { FoundView, RecordingView, ShownCharacter } from './bindings.ts';
import {
  type Answers,
  type Change,
  type Guide,
  type Place,
  type Turn,
  join,
  openGuide,
  settle,
  turn,
  unchecked,
} from './confirm.ts';
import { type FoundRow, listed } from './found.ts';
import { type Color, type Picture, cropPicture } from './pixels.ts';
import { type ReviewEntry, startEntry } from './review.ts';
import { boxCharactersKey, useBoxCharacters } from './useBoxCharacters.ts';
import type { Finder } from './useFound.ts';
import type { useRecording } from './useRecording.ts';
import type { useReview } from './useReview.ts';

/** The reading at `index` of a recording as a place, with the text and the keep `entry` has for it. */
function readingPlace(reading: RecordingView['readings'][number], index: number, entry: ReviewEntry): Place {
  return {
    source: 'reading',
    index,
    number: index + 1,
    characters: reading.characters,
    width: reading.width,
    text: entry.text,
    typed: entry.text !== startEntry(reading, undefined).text,
    keep: entry.keep,
    ticks: true,
    reading: reading.text.kind === 'pending' && entry.text === '',
    asks: entry.keep,
  };
}

/** A box of the reference, with the characters it shows, or null until they arrive. */
export interface DrawnBox {
  readonly box: Box;
  readonly characters: readonly ShownCharacter[] | null;
}

/**
 * The places a check goes over: every box, every find listed, ticked or not, and every recorded reading. It asks
 * about the boxes, the finds, and the readings review keeps. A box whose characters have not arrived waits like text
 * still being read. A reading review leaves out shows only shapes the kept ones show, so it
 * has nothing to ask about, but it gets the answers too: review compares the texts of all the readings, and would
 * flag a kept reading the check fixed against a left-out one it did not.
 */
export function checkPlaces(
  drawn: readonly DrawnBox[],
  found: FoundView['finds'],
  rows: readonly FoundRow[],
  recorded: RecordingView['readings'],
  entries: readonly ReviewEntry[],
): readonly Place[] {
  const boxes = drawn.map(({ box, characters }, at): Place => ({
    source: 'box',
    index: box.id,
    number: at + 1,
    characters: characters ?? [],
    width: box.region.width,
    text: box.text,
    typed: box.guess !== box.text,
    keep: true,
    ticks: false,
    reading: characters === null,
    asks: true,
  }));
  const finds = listed(rows).flatMap((row): Place[] => {
    const find = found[row.index];
    return find === undefined
      ? []
      : [
          {
            source: 'find',
            index: row.index,
            number: row.index + 1,
            characters: find.characters,
            width: find.region.width,
            text: row.text,
            typed: row.typed,
            keep: row.keep,
            ticks: true,
            reading: row.reading,
            asks: true,
          },
        ];
  });
  const readings = recorded.flatMap((reading, index): Place[] => {
    const entry = entries[index];
    return entry === undefined ? [] : [readingPlace(reading, index, entry)];
  });
  return [...boxes, ...finds, ...readings];
}

/**
 * What an Escape key press closes on the capture screen: the character check when it is open, since it covers
 * review, or else review, which then closes without keeping while its edits stay for the next review. Nothing for
 * another key, or for an Escape that something with focus already took as its own and marked with preventDefault,
 * such as a question's answer, a hint, or a shortcut being chosen.
 */
export function escapeCloses(
  event: Pick<KeyboardEvent, 'key' | 'defaultPrevented'>,
  open: { readonly checking: boolean; readonly reviewing: boolean },
): 'check' | 'review' | null {
  if (event.key !== 'Escape' || event.defaultPrevented) {
    return null;
  }
  if (open.checking) {
    return 'check';
  }
  return open.reviewing ? 'review' : null;
}

/**
 * The places a reference saved now would learn from: each box of `places` with text, and each of the `recorded`
 * readings the draft keeps, with the text `kept` has for it.
 */
export function learnedPlaces(
  places: readonly Place[],
  recorded: RecordingView['readings'],
  kept: ReadonlyMap<number, string>,
): readonly Place[] {
  const boxes = places.filter((place) => place.source === 'box' && place.text.trim() !== '');
  const readings = [...kept].flatMap(([index, text]): Place[] => {
    const reading = recorded[index];
    return reading === undefined ? [] : [readingPlace(reading, index, { keep: true, text })];
  });
  return [...boxes, ...readings];
}

/** What the check goes over besides the finds and the readings: the reference's boxes, and what it keeps. */
export interface Taught {
  readonly boxes: readonly Box[];
  /** The characters' color and tolerance, which split a box into its characters. */
  readonly ink: Color | null;
  readonly tolerance: number;
  /** The readings the draft keeps, with their text. */
  readonly recorded: ReadonlyMap<number, string>;
  /** Each screenshot at its own pixels, once they arrive. */
  readonly full: ReadonlyMap<number, Picture>;
  /** Types `text` as what box `id` shows. */
  readonly retype: (id: number, text: string) => void;
  /** Keeps reading `index` with `text` on the draft, or with null keeps it no longer. */
  readonly keepReading: (index: number, text: string | null) => void;
}

/**
 * The check of the characters the reference's boxes, `finder`'s finds and `review`'s kept readings show, as the
 * capture screen runs it.
 */
export interface CharacterCheck {
  /** The places it goes over, whether or not it is open. */
  readonly places: readonly Place[];
  /** The open check, where it can be for the places as they are now, or null while it is closed. */
  readonly guide: Guide | null;
  readonly open: () => void;
  /** Closes it and forgets its answers; the texts they wrote stay. Escape closes it too. */
  readonly close: () => void;
  /**
   * The characters a reference saved now would learn that nobody has checked: the text reader read them, and the
   * user neither typed them nor answered for their shapes in a check of this reference.
   */
  readonly unchecked: readonly string[];
  /** Forgets what earlier checks answered, as the reference's editor closes. */
  readonly forget: () => void;
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
  taught: Taught,
): CharacterCheck {
  const [held, setHeld] = useState<Guide | null>(null);
  // What the checks that are closed answered, which says what the user has checked once the answers are forgotten.
  const [past, setPast] = useState<Answers>(new Map());
  const { boxes, ink, tolerance, recorded, full } = taught;
  const shown = useBoxCharacters(boxes, ink, tolerance);
  const places = useMemo(() => {
    const drawn =
      ink === null
        ? []
        : boxes.map((box) => ({ box, characters: shown.get(boxCharactersKey(box, ink, tolerance)) ?? null }));
    return checkPlaces(drawn, finder.view?.finds ?? [], finder.rows, recorder.view?.readings ?? [], review.entries);
  }, [boxes, ink, tolerance, shown, finder.view, finder.rows, recorder.view, review.entries]);
  const guide = held === null ? null : settle(held, places, needs);
  const boxCrops = useMemo(
    () =>
      new Map(
        boxes.flatMap((box): [number, Picture][] => {
          const picture = full.get(box.screenshot);
          const crop = picture === undefined ? null : cropPicture(picture, box.region);
          return crop === null ? [] : [[box.id, crop]];
        }),
      ),
    [boxes, full],
  );
  const answered = useMemo(() => new Map([...past, ...(held?.answers ?? [])]), [past, held]);
  const notChecked = useMemo(
    () => unchecked(learnedPlaces(places, recorder.view?.readings ?? [], recorded), answered),
    [places, recorder.view, recorded, answered],
  );
  const close = (): void => {
    setPast(answered);
    setHeld(null);
  };

  const edit = (place: Place, entry: ReviewEntry): void => {
    if (place.source === 'box') {
      // A box has no tick. Text the check writes is the user's answer, as typing it in the list is.
      if (entry.text !== place.text) {
        taught.retype(place.index, entry.text);
      }
      return;
    }
    if (place.source === 'reading') {
      review.edit(place.index, entry);
      // A reading the draft keeps already is fixed there too, so that saving learns what the check wrote.
      if (recorded.has(place.index)) {
        taught.keepReading(place.index, entry.keep ? entry.text : null);
      }
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

  // A place that shows while the check is open, or whose text arrives then, gets the answers that stand.
  useEffect(() => {
    if (held === null) {
      return;
    }
    const next = join(held, places);
    if (next.guide !== held) {
      apply(next.changes);
      setHeld(next.guide);
    }
  });

  // Escape closes the check, or review when the check is not open over it. One listener decides for both, so the
  // two never both close. It hears the key last, as it reaches the window, after whatever has focus took its own.
  const checking = held !== null;
  const { reviewing, close: closeReview } = review;
  useEffect(() => {
    if (!checking && !reviewing) {
      return undefined;
    }
    const onKey = (event: KeyboardEvent): void => {
      const closes = escapeCloses(event, { checking, reviewing });
      if (closes === 'check') {
        close();
      } else if (closes === 'review') {
        closeReview();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('keydown', onKey);
    };
  });

  return {
    places,
    guide,
    open: () => {
      setHeld(openGuide(places, needs));
    },
    close,
    unchecked: notChecked,
    forget: () => {
      setPast(new Map());
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
    picture: (place) => {
      const crops = place.source === 'box' ? boxCrops : place.source === 'find' ? finder.crops : recorder.crops;
      return crops.get(place.index);
    },
  };
}
