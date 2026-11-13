// The check of a reference's characters, one at a time, over the boxes, the finds and the recorded readings it would
// learn from.

import { useEffect, useMemo, useReducer } from 'react';

import type { Box } from './boxes.ts';

import type { FoundView, RecordedLabel, RecordingView, ShownCharacter } from './bindings.ts';
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
  unconfirmed,
  withEdit,
} from './confirm.ts';
import { type FoundRow, listed } from './found.ts';
import { type Color, type Picture, cropPicture } from './pixels.ts';
import { type ReviewEntry, startEntry } from './review.ts';
import { boxCharactersKey, useBoxCharacters } from './useBoxCharacters.ts';
import type { Finder } from './useFound.ts';
import type { useRecording } from './useRecording.ts';
import type { useReview } from './useReview.ts';

/**
 * The reading at `index` of a recording as a place, with the text and the keep `entry` has for it. The check asks
 * about it when review keeps it or, with `saved`, when a reference saved now would learn it.
 */
function readingPlace(
  reading: RecordingView['readings'][number],
  index: number,
  entry: ReviewEntry,
  saved: boolean,
): Place {
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
    asks: entry.keep || saved,
  };
}

/**
 * The readings a reference saved next would learn, each with its text: while review is open, the ones its Keep
 * would put on the draft, since the save waits for it; otherwise the ones the draft keeps. The offer above Save
 * reference counts their characters, and the check asks about each of them, so the offer never names a character
 * the check has no step for.
 */
export function savedReadings(
  reviewing: boolean,
  keeping: readonly RecordedLabel[],
  recorded: ReadonlyMap<number, string>,
): ReadonlyMap<number, string> {
  return reviewing ? new Map(keeping.map((label) => [label.reading, label.text])) : recorded;
}

/** A box of the reference, with the characters it shows, or null until they arrive. */
export interface DrawnBox {
  readonly box: Box;
  readonly characters: readonly ShownCharacter[] | null;
}

/**
 * The places a check goes over: every box, every find listed, ticked or not, and every recorded reading. It asks
 * about the boxes, the finds, the readings review keeps, and the `saved` ones, which {@link savedReadings} lists:
 * a reading the draft keeps is learned on a save even after review, reopened, left it out without Keep. A box
 * whose characters have not arrived waits like text still being read. Any other reading review leaves out shows
 * only shapes the kept ones show, so it has nothing to ask about, but it gets the answers too: review compares the
 * texts of all the readings, and would flag a kept reading the check fixed against a left-out one it did not.
 */
export function checkPlaces(
  drawn: readonly DrawnBox[],
  found: FoundView['finds'],
  rows: readonly FoundRow[],
  recorded: RecordingView['readings'],
  entries: readonly ReviewEntry[],
  saved: ReadonlyMap<number, string>,
): readonly Place[] {
  const boxes = drawn.map(({ box, characters }, at): Place => ({
    source: 'box',
    index: box.id,
    number: at + 1,
    characters: characters ?? [],
    width: box.region.width,
    text: box.text,
    typed: box.guessed !== true,
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
    return entry === undefined ? [] : [readingPlace(reading, index, entry, saved.has(index))];
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
 * The places a reference saved next would learn from: each box of `places` with text, and each of the `recorded`
 * readings among `saved`, which {@link savedReadings} lists, with the text it has there.
 */
export function learnedPlaces(
  places: readonly Place[],
  recorded: RecordingView['readings'],
  saved: ReadonlyMap<number, string>,
): readonly Place[] {
  const boxes = places.filter((place) => place.source === 'box' && place.text.trim() !== '');
  const readings = [...saved].flatMap(([index, text]): Place[] => {
    const reading = recorded[index];
    return reading === undefined ? [] : [readingPlace(reading, index, { keep: true, text }, true)];
  });
  return [...boxes, ...readings];
}

/** One reading as the draft has it: the text it keeps it with, or kept it with until a check left it out. */
export interface KeptReading {
  readonly text: string;
  /** Whether a check left it out, so the draft keeps it no longer until the check keeps it again. */
  readonly leftOut: boolean;
}

/**
 * What the draft has of the reading at `place` after a check's edit of it to `entry`, or null for a reading the
 * draft never kept, which review alone keeps or leaves out. `recorded` has the readings the draft keeps, and
 * `leftOut` the ones the open check took from it, each with its text there.
 *
 * The draft gets only what the edit changed, so that saving learns what the check wrote and nothing else: `place`
 * has review's text and keep, and what review changed there stays review's until its Keep. So the draft's text gets
 * the characters the edit changed, and the draft keeps the reading or not as before unless the edit changes
 * `place`'s keep: an answer that only rewrites the text of a reading review left out without Keep leaves it on the
 * draft. A reading the edit leaves out goes from the draft with its text remembered, and one it keeps again comes
 * back with that text.
 */
export function editedReading(
  recorded: ReadonlyMap<number, string>,
  leftOut: ReadonlyMap<number, string>,
  place: Place,
  entry: ReviewEntry,
): KeptReading | null {
  const text = recorded.get(place.index) ?? leftOut.get(place.index);
  if (text === undefined) {
    return null;
  }
  return {
    text: withEdit(text, place.text, entry.text),
    leftOut: entry.keep === place.keep ? !recorded.has(place.index) : !entry.keep,
  };
}

/** What the capture screen holds of the check of the reference being taught. */
export interface Held {
  /** The open check, or null while it is closed. */
  readonly guide: Guide | null;
  /** What the checks closed since the reference's editor opened answered, which says what the user has confirmed. */
  readonly past: Answers;
  /**
   * The readings the open check took from the draft, each with the text the draft kept it with, and the ID of the
   * recording they are readings of.
   */
  readonly leftOut: { readonly recording: number | null; readonly texts: ReadonlyMap<number, string> };
}

/** Nothing held: no check is open, and none has answered anything. */
export const NOTHING_HELD: Held = { guide: null, past: new Map(), leftOut: { recording: null, texts: new Map() } };

/** What changes what the capture screen holds of the check. */
export type HeldEvent =
  /** The check opens with `guide`, or the open one becomes it. */
  | { readonly kind: 'show'; readonly guide: Guide }
  /** The check closes: its answers stay as what the user has confirmed. */
  | { readonly kind: 'close' }
  /** The reference's editor closes, so the next reference starts with nothing. */
  | { readonly kind: 'forget' }
  /** The check edited reading `index` of recording `recording`, which the draft now has as `reading`. */
  | {
      readonly kind: 'reading';
      readonly recording: number | null;
      readonly index: number;
      readonly reading: KeptReading;
    };

/** The readings `held` remembers as left out of recording `recording`: none for any other recording than theirs. */
export function leftOutOf(held: Held, recording: number | null): ReadonlyMap<number, string> {
  return held.leftOut.recording === recording ? held.leftOut.texts : NOTHING_HELD.leftOut.texts;
}

/** Every answer `held` has, of the closed checks and the open one, whose answers come last. */
export function answersOf(held: Held): Answers {
  return new Map([...held.past, ...(held.guide?.answers ?? [])]);
}

/**
 * What the capture screen holds of the check after `event`. Closing forgets which readings the check left out with
 * its answers, since only an answer of the open check keeps one again.
 */
export function heldStep(held: Held, event: HeldEvent): Held {
  switch (event.kind) {
    case 'show':
      return { ...held, guide: event.guide };
    case 'close':
      return { ...NOTHING_HELD, past: answersOf(held) };
    case 'forget':
      return NOTHING_HELD;
    case 'reading': {
      const texts = new Map(leftOutOf(held, event.recording));
      if (event.reading.leftOut) {
        texts.set(event.index, event.reading.text);
      } else if (!texts.delete(event.index) && held.leftOut.recording === event.recording) {
        return held;
      }
      return { ...held, leftOut: { recording: event.recording, texts } };
    }
  }
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
   * The characters a reference saved next would learn that nobody has confirmed: the text reader read them, and the
   * user neither typed them nor answered for their shapes in a check of this reference.
   */
  readonly unconfirmed: readonly string[];
  /** Closes it and forgets what every check answered, as the reference's editor closes. */
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
  const [state, send] = useReducer(heldStep, NOTHING_HELD);
  const held = state.guide;
  const { boxes, ink, tolerance, recorded, full } = taught;
  const shown = useBoxCharacters(boxes, ink, tolerance);
  const saved = useMemo(
    () => savedReadings(review.reviewing, review.keeping, recorded),
    [review.reviewing, review.keeping, recorded],
  );
  const places = useMemo(() => {
    const drawn =
      ink === null
        ? []
        : boxes.map((box) => ({ box, characters: shown.get(boxCharactersKey(box, ink, tolerance)) ?? null }));
    return checkPlaces(
      drawn,
      finder.view?.finds ?? [],
      finder.rows,
      recorder.view?.readings ?? [],
      review.entries,
      saved,
    );
  }, [boxes, ink, tolerance, shown, finder.view, finder.rows, recorder.view, review.entries, saved]);
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
  const notConfirmed = useMemo(
    () => unconfirmed(learnedPlaces(places, recorder.view?.readings ?? [], saved), answersOf(state)),
    [places, recorder.view, saved, state],
  );
  const close = (): void => {
    send({ kind: 'close' });
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
      const recording = recorder.view?.id ?? null;
      const reading = editedReading(recorded, leftOutOf(state, recording), place, entry);
      if (reading !== null) {
        taught.keepReading(place.index, reading.leftOut ? null : reading.text);
        send({ kind: 'reading', recording, index: place.index, reading });
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
      send({ kind: 'show', guide: next.guide });
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
      send({ kind: 'show', guide: openGuide(places, needs) });
    },
    close,
    unconfirmed: notConfirmed,
    forget: () => {
      send({ kind: 'forget' });
    },
    act: (event) => {
      if (guide === null) {
        return;
      }
      const next = turn(guide, event, places, needs);
      apply(next.changes);
      send({ kind: 'show', guide: next.guide });
    },
    edit,
    picture: (place) => {
      const crops = place.source === 'box' ? boxCrops : place.source === 'find' ? finder.crops : recorder.crops;
      return crops.get(place.index);
    },
  };
}
