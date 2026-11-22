// The check of a reference's characters, one at a time, over the boxes, the finds and the recorded readings it would
// learn from.

import { useEffect, useMemo, useReducer } from 'react';

import type { Box } from './boxes.ts';

import type { FoundView, RecordedLabel, RecordingView, ShownCharacter } from './bindings.ts';
import {
  type Answers,
  type Change,
  type Earlier,
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
import { type Picture, cropPicture } from './pixels.ts';
import { type ReviewEntry, startEntry } from './review.ts';
import { boxCharactersKey, useBoxCharacters } from './useBoxCharacters.ts';
import type { Finder } from './useFound.ts';
import type { useRecording } from './useRecording.ts';
import type { useReview } from './useReview.ts';

/**
 * The reading at `index` of a recording as a place, with the text `entry` has for it. It is kept, and the check
 * asks about it, when review keeps it or, with `saved`, when a reference saved now would learn it: a place's keep
 * says what the reference learns from, which review's tick alone does not say for a reading the draft keeps.
 */
function readingPlace(
  reading: RecordingView['readings'][number],
  index: number,
  entry: ReviewEntry,
  saved: boolean,
): Place {
  const learned = entry.keep || saved;
  return {
    source: 'reading',
    index,
    number: index + 1,
    characters: reading.characters,
    width: reading.width,
    text: entry.text,
    typed: entry.text !== startEntry(reading, undefined).text,
    keep: learned,
    ticks: true,
    reading: reading.text.kind === 'pending' && entry.text === '',
    asks: learned,
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
 * a reading the draft keeps is learned on a save even after review, reopened, left it out without Keep, so it
 * counts as kept here, and the check leaves it out of the draft like any kept place. A box whose characters have
 * not arrived waits like text still being read. Any other reading review leaves out shows only shapes the kept
 * ones show, so it has nothing to ask about, but it gets the answers too: review compares the texts of all the
 * readings, and would flag a kept reading the check fixed against a left-out one it did not.
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

/** A reading the open check took from the draft. */
export interface LeftOut {
  /** The text the draft kept it with. */
  readonly text: string;
  /** Whether review had it ticked then, which is the tick it gets back when the check keeps it again. */
  readonly ticked: boolean;
}

/** One reading as the draft has it: the text it keeps it with, or kept it with until a check left it out. */
export interface KeptReading extends LeftOut {
  /** Whether a check left it out, so the draft keeps it no longer until the check keeps it again. */
  readonly leftOut: boolean;
}

/**
 * Whether review has the reading at `place` ticked after a check's edit of it to `entry`, given its `tick` before.
 * A place's keep says whether a save learns the reading, which for one the draft keeps can hold while review's tick
 * is clear. So the tick follows the edit only when the edit changes the keep: an answer that rewrites the text of
 * such a reading does not tick it. One the check keeps again gets the tick it had when the check took it from the
 * draft, `was`, or is ticked when the draft never had it.
 */
export function reviewTick(tick: boolean, was: boolean | undefined, place: Place, entry: ReviewEntry): boolean {
  if (entry.keep === place.keep) {
    return tick;
  }
  return entry.keep && (was ?? true);
}

/**
 * What the draft has of the reading at `place` after a check's edit of it to `entry`, or null for a reading the
 * draft never kept, which review alone keeps or leaves out. `recorded` has the readings the draft keeps, and
 * `leftOut` the ones the open check took from it. `tick` is review's tick for the reading before the edit.
 *
 * The draft gets only what the edit changed, so that saving learns what the check wrote and nothing else: `place`
 * has review's text, and what review changed there stays review's until its Keep. So the draft's text gets the
 * characters the edit changed, and the draft keeps the reading or not as before unless the edit changes `place`'s
 * keep: while review is open, an answer that only rewrites the text of a reading review left out without Keep
 * leaves it on the draft. A reading the edit leaves out goes from the draft with its text and review's tick
 * remembered, and one it keeps again comes back with that text.
 */
export function editedReading(
  recorded: ReadonlyMap<number, string>,
  leftOut: ReadonlyMap<number, LeftOut>,
  place: Place,
  tick: boolean,
  entry: ReviewEntry,
): KeptReading | null {
  const left = leftOut.get(place.index);
  const text = recorded.get(place.index) ?? left?.text;
  if (text === undefined) {
    return null;
  }
  return {
    text: withEdit(text, place.text, entry.text),
    leftOut: entry.keep === place.keep ? !recorded.has(place.index) : !entry.keep,
    ticked: left?.ticked ?? tick,
  };
}

/** What the capture screen holds of the check of the reference being taught. */
export interface Held {
  /** The open check, or null while it is closed. */
  readonly guide: Guide | null;
  /**
   * What the checks closed since the reference's editor opened leave the next one: their answers, which also say
   * what the user has confirmed, the places they left out, the places that joined them, and the IDs of the search
   * and the recording those places were finds and readings of.
   */
  readonly past: Past;
  /** The readings the checks took from the draft, and the ID of the recording they are readings of. */
  readonly leftOut: { readonly recording: number | null; readonly readings: ReadonlyMap<number, LeftOut> };
}

/** What the closed checks leave the next one. */
interface Past extends Earlier {
  readonly search: number | null;
  readonly recording: number | null;
}

/** Nothing held: no check is open, and none has answered anything. */
export const NOTHING_HELD: Held = {
  guide: null,
  past: { answers: new Map(), left: new Set(), seen: new Set(), search: null, recording: null },
  leftOut: { recording: null, readings: new Map() },
};

/** What changes what the capture screen holds of the check. */
export type HeldEvent =
  /** The check opens with `guide`, or the open one becomes it. */
  | { readonly kind: 'show'; readonly guide: Guide }
  /**
   * The check closes: its answers, the places it left out, and the places that joined it stay for the next
   * check. `search` and `recording` are the IDs of the search and the recording whose finds and readings it went
   * over.
   */
  | { readonly kind: 'close'; readonly search: number | null; readonly recording: number | null }
  /**
   * The reference has the boxes `ids` now. A box that is gone is no place that joined a closed check anymore, so a
   * box drawn later, which can have its ID, joins anew.
   */
  | { readonly kind: 'boxes'; readonly ids: ReadonlySet<number> }
  /** The check edited reading `index` of recording `recording`, which the draft now has as `reading`. */
  | {
      readonly kind: 'reading';
      readonly recording: number | null;
      readonly index: number;
      readonly reading: KeptReading;
    };

/** The readings `held` remembers as left out of recording `recording`: none for any other recording than theirs. */
export function leftOutOf(held: Held, recording: number | null): ReadonlyMap<number, LeftOut> {
  return held.leftOut.recording === recording ? held.leftOut.readings : NOTHING_HELD.leftOut.readings;
}

/**
 * The answers that stand in `held`: the open check's, which began with the closed checks' and may have taken one
 * back, or else the closed checks'.
 */
export function answersOf(held: Held): Answers {
  return held.guide?.answers ?? held.past.answers;
}

/**
 * What a check that opens now goes on from: the answers of the closed checks, and the places they left out and the
 * places that joined them that are the same places still. A find of another search than `search`, or a reading of
 * another recording than `recording`, only shares an index with one of those, so it joins anew and was never left
 * out. A box is known by the shapes it joined with.
 */
export function earlierOf(held: Held, search: number | null, recording: number | null): Earlier {
  const { past } = held;
  const same = (key: string): boolean =>
    key.startsWith('find-') ? past.search === search : !key.startsWith('reading-') || past.recording === recording;
  const still = (keys: ReadonlySet<string>): ReadonlySet<string> => new Set([...keys].filter(same));
  return { answers: past.answers, left: still(past.left), seen: still(past.seen) };
}

/**
 * What the capture screen holds of the check after `event`. Closing keeps the answers, the places left out, the
 * places that joined, and the readings the check took from the draft, which the next check goes on from: taking
 * back there that a shape is no character keeps those places again.
 */
export function heldStep(held: Held, event: HeldEvent): Held {
  switch (event.kind) {
    case 'show':
      return { ...held, guide: event.guide };
    case 'close':
      return {
        ...held,
        guide: null,
        past: {
          answers: answersOf(held),
          left: held.guide?.left ?? held.past.left,
          seen: held.guide?.seen ?? held.past.seen,
          search: held.guide === null ? held.past.search : event.search,
          recording: held.guide === null ? held.past.recording : event.recording,
        },
      };
    case 'boxes': {
      const seen = [...held.past.seen].filter((key) => {
        const id = /^box-(\d+)(?: |$)/u.exec(key)?.[1];
        return id === undefined || event.ids.has(Number(id));
      });
      return seen.length === held.past.seen.size ? held : { ...held, past: { ...held.past, seen: new Set(seen) } };
    }
    case 'reading': {
      const readings = new Map(leftOutOf(held, event.recording));
      if (event.reading.leftOut) {
        readings.set(event.index, { text: event.reading.text, ticked: event.reading.ticked });
      } else if (!readings.delete(event.index) && held.leftOut.recording === event.recording) {
        return held;
      }
      return { ...held, leftOut: { recording: event.recording, readings } };
    }
  }
}

/** What the check goes over besides the finds and the readings: the reference's boxes, and what it keeps. */
export interface Taught {
  readonly boxes: readonly Box[];
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
  /** Closes it. Its answers stay for the next check, and the texts they wrote stay. Escape closes it too. */
  readonly close: () => void;
  /**
   * The characters a reference saved next would learn that nobody has confirmed: the text reader read them, and the
   * user neither typed them nor answered for their shapes in a check of this reference.
   */
  readonly unconfirmed: readonly string[];
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
  const { boxes, recorded, full } = taught;
  const shown = useBoxCharacters(boxes);
  const saved = useMemo(
    () => savedReadings(review.reviewing, review.keeping, recorded),
    [review.reviewing, review.keeping, recorded],
  );
  const places = useMemo(() => {
    const drawn = boxes.map((box) => ({
      box,
      characters: shown.get(boxCharactersKey(box)) ?? null,
    }));
    return checkPlaces(
      drawn,
      finder.view?.finds ?? [],
      finder.rows,
      recorder.view?.readings ?? [],
      review.entries,
      saved,
    );
  }, [boxes, shown, finder.view, finder.rows, recorder.view, review.entries, saved]);
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
  const search = finder.view?.id ?? null;
  const recording = recorder.view?.id ?? null;
  const close = (): void => {
    send({ kind: 'close', search, recording });
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
      const leftOut = leftOutOf(state, recording);
      const tick = review.entries[place.index]?.keep ?? place.keep;
      review.edit(place.index, {
        text: entry.text,
        keep: reviewTick(tick, leftOut.get(place.index)?.ticked, place, entry),
      });
      // A reading the draft keeps already is fixed there too, so that saving learns what the check wrote.
      const reading = editedReading(recorded, leftOut, place, tick, entry);
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

  // A box deleted while the check is closed leaves the places that joined, before a new box can take its ID.
  useEffect(() => {
    send({ kind: 'boxes', ids: new Set(boxes.map((box) => box.id)) });
  }, [boxes]);

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
      // The answers of the checks closed before stand, and the places that show since get them.
      const start = openGuide(places, needs, earlierOf(state, search, recording));
      apply(start.changes);
      send({ kind: 'show', guide: start.guide });
    },
    close,
    unconfirmed: notConfirmed,
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
