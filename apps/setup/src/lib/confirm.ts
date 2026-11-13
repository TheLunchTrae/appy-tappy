// Checking a reference's characters one at a time, across every place that shows them.
//
// A reference learns a character from each place whose text names it: a box, a find from Find more like it, or a
// recorded reading. The text reader's guesses fill the texts of finds and readings in, and it tends to misread one
// shape the same way wherever the shape shows. So the check goes by shape instead of by place. Each step is one
// character with every shape the texts give it, and one answer for a shape rewrites the text of every place that
// shows the shape.
//
// The places' texts stay the one record of what each place shows. The check holds only the answers given so far, by
// shape, and where it is. Every change it makes is an edit of a place's text or its tick, the same edit the user
// makes by hand in the list of boxes, the list of finds, or review. It makes them in the turn that gives the answer,
// and once more for each place that joins afterward: one that first shows, or whose text the reader sends, while the
// check is open gets the answers that stand for its shapes. Text the user typed is theirs. Where it disagrees with
// an answer it is left as it is, and the shape counts as not answered again, so its step asks once more.
//
// A place whose text has another number of characters than the place has shapes gives its shapes no characters. It
// joins no step. Once every one of its shapes has an answer from the other places, its text is written from the
// answers. One that is still left shows at the end, to type or leave out, and what the user types there answers for
// its shapes.

import type { ShownCharacter } from './bindings.ts';
import { labelCharacters } from './reference.ts';

/** One place a reference would learn characters from. */
export interface Place {
  /** Whether it is a box, a find, or a recorded reading, and what names it there: a box's ID, or else its index. */
  readonly source: 'box' | 'find' | 'reading';
  readonly index: number;
  /** Its number as its list shows it, from 1. */
  readonly number: number;
  /** Where each of its characters is in its picture, with the name of its shape. */
  readonly characters: readonly ShownCharacter[];
  /** How wide its picture is, in the pixels the characters are placed in. */
  readonly width: number;
  readonly text: string;
  /** Whether its text is the user's, typed by hand or written by an answer, and not the text reader's guess. */
  readonly typed: boolean;
  /** Whether it is ticked or kept, so the reference learns from it. A box always is. */
  readonly keep: boolean;
  /**
   * Whether it has a tick the check may change. A box has none: the reference learns from it while it has text,
   * and only the user deletes it.
   */
  readonly ticks: boolean;
  /** Whether the text reader is still reading it, so it has no text to ask about yet. */
  readonly reading: boolean;
  /**
   * Whether the check asks about it. A reading that review leaves out is not asked about, and its tick is never
   * changed: it only gets the answers, so that its text does not disagree with the texts of the kept ones.
   */
  readonly asks: boolean;
}

/** What the user answered for each shape so far: its character, or null for a shape that is no character. */
export type Answers = ReadonlyMap<string, string | null>;

/** One character of one place: the place's position among the places, and the character's position in it. */
interface Spot {
  readonly place: number;
  readonly at: number;
}

/** One shape in a step, and every spot that shows it. */
export interface ShapeUse {
  readonly shape: string;
  readonly spots: readonly Spot[];
  /** The other characters some places' texts give this shape. */
  readonly also: readonly string[];
  /** Whether the user said it is no character. */
  readonly aside: boolean;
  /** Whether the user has answered for it, and no place's text says otherwise. */
  readonly answered: boolean;
}

/** One step of the check: a character, and the shapes the places' texts give it. */
export interface Step {
  readonly character: string;
  readonly shapes: readonly ShapeUse[];
}

/**
 * Where the check is: at a character's step, at the places whose text fits no step, or past them all. The stop for
 * text that fits no step names its places, as {@link placeKey} does, when the check comes to it. They stay its
 * places until it leaves, so a text does not leave the list while the user types it.
 */
type Cursor =
  | { readonly at: 'character'; readonly character: string }
  | { readonly at: 'unfitted'; readonly places: readonly string[] }
  | { readonly at: 'done' };

/** A check that is open. */
export interface Guide {
  readonly cursor: Cursor;
  readonly answers: Answers;
  /**
   * The places, as {@link placeKey} names them, that the check left out for a shape that is no character. Taking
   * that answer back keeps them again, and until then they stay in the steps, so the answer can be taken back.
   */
  readonly left: ReadonlySet<string>;
  /**
   * The places, as {@link placeKey} names them, that have joined the check: their text was read, and they got the
   * answers that stood then. One that is not among them yet gets the answers when it joins.
   */
  readonly seen: ReadonlySet<string>;
}

/** What the user does in a step. */
export type Turn =
  /**
   * Every shape of the step that is not answered is its character. On the stop for text that fits no step, what the
   * user typed there answers for the shapes that had no answer.
   */
  | { readonly kind: 'confirm' }
  /** Goes on without answering. */
  | { readonly kind: 'skip' }
  | { readonly kind: 'back' }
  /** One shape is another character than its step's. */
  | { readonly kind: 'retype'; readonly shape: string; readonly character: string }
  /** One shape is no character, so the places that show it are left out. */
  | { readonly kind: 'setAside'; readonly shape: string }
  /** One shape is a character after all: takes back that it is none. */
  | { readonly kind: 'restore'; readonly shape: string };

/** An edit of one place: its text and whether it is ticked or kept. */
export interface Change {
  readonly source: Place['source'];
  readonly index: number;
  readonly text: string;
  readonly keep: boolean;
}

/** The answers and the places left out, which say what the steps are. */
type Given = Pick<Guide, 'answers' | 'left'>;

/** A name for `place` that stays the same while the places around it change. */
export function placeKey(place: Pick<Place, 'source' | 'index'>): string {
  return `${place.source}-${place.index}`;
}

/** The character `place`'s text gives each of its shapes, or null unless it has one character for each. */
function named(place: Place): readonly string[] | null {
  const characters = labelCharacters(place.text);
  return characters.length > 0 && characters.length === place.characters.length ? characters : null;
}

/** Whether `place` shows a shape the user said is no character, so the reference cannot learn it. */
function setAside(place: Place, answers: Answers): boolean {
  return place.characters.some((character) => answers.get(character.shape) === null);
}

/**
 * Whether `place` gives the check nothing to do: no character was found in its picture, so no answer is about it,
 * and it has no tick to leave it out with. That is a box whose characters could not be found, which only the user
 * changes, in the list of boxes.
 */
function blank(place: Place): boolean {
  return !place.ticks && place.characters.length === 0;
}

/** Whether the steps go over `place`: the check asks about it, or left it out and may yet keep it again. */
function asked(place: Place, left: ReadonlySet<string>): boolean {
  return place.asks || left.has(placeKey(place));
}

/** Whether the check may change `place`'s tick: it has one, and the steps go over the place. */
function ticked(place: Place, left: ReadonlySet<string>): boolean {
  return place.ticks && asked(place, left);
}

/**
 * The steps for `places` with what is `given` so far: each character with its shapes, the characters among `needs`
 * first, in that order, then the rest as they first show. A shape goes with its answer, or else with the character
 * most places' texts give it, the first of equals; one that is no character stays with the character the texts
 * give it. A shape with an answer counts as answered only while no place's text gives it another character.
 */
export function steps(places: readonly Place[], given: Given, needs: readonly string[]): readonly Step[] {
  const uses = new Map<string, { spots: Spot[]; votes: Map<string, number> }>();
  places.forEach((place, at) => {
    const characters = named(place);
    if (characters === null || !asked(place, given.left)) {
      return;
    }
    place.characters.forEach((shown, position) => {
      const use = uses.get(shown.shape) ?? { spots: [], votes: new Map<string, number>() };
      uses.set(shown.shape, use);
      use.spots.push({ place: at, at: position });
      const character = characters[position] ?? '';
      use.votes.set(character, (use.votes.get(character) ?? 0) + 1);
    });
  });
  const byCharacter = new Map<string, ShapeUse[]>();
  for (const [shape, use] of uses) {
    const answer = given.answers.get(shape);
    const most = Math.max(...use.votes.values());
    const voted = [...use.votes].find(([, count]) => count === most)?.[0] ?? '';
    const character = answer ?? voted;
    const also = [...use.votes.keys()].filter((other) => other !== character);
    const shapes = byCharacter.get(character) ?? [];
    byCharacter.set(character, shapes);
    shapes.push({
      shape,
      spots: use.spots,
      also,
      aside: answer === null,
      answered: answer === null || (answer !== undefined && also.length === 0),
    });
  }
  const order = [
    ...needs.filter((character) => byCharacter.has(character)),
    ...[...byCharacter.keys()].filter((character) => !needs.includes(character)),
  ];
  return order.map((character) => ({ character, shapes: byCharacter.get(character) ?? [] }));
}

/**
 * The places, by position, whose text fits no step: the check asks about the place, the text reader is done with
 * it, and its text has another number of characters than the place has shapes, or the place shows no shape at all
 * and can be left out. A place left out for a shape that is no character needs no text, and neither does a box
 * that shows no shape: no text fits it, and the check cannot leave it out.
 */
export function unfitted(places: readonly Place[], given: Given): readonly number[] {
  return places.flatMap((place, at) =>
    asked(place, given.left) &&
    !place.reading &&
    !blank(place) &&
    named(place) === null &&
    !setAside(place, given.answers)
      ? [at]
      : [],
  );
}

/** How many places the check asks about are still being read, so their characters are not asked about yet. */
export function beingRead(places: readonly Place[]): number {
  return places.filter((place) => place.asks && place.reading).length;
}

/** `place`'s text by the answers alone, or null unless each of its shapes has a character for an answer. */
function answeredText(place: Place, answers: Answers): string | null {
  const characters = place.characters.map((shown) => answers.get(shown.shape) ?? null);
  return characters.length > 0 && characters.every((character) => character !== null) ? characters.join('') : null;
}

/** `text` with each character that is not white space replaced by the one at its position in `characters`, if any. */
function rewritten(text: string, characters: readonly (string | null | undefined)[]): string {
  let position = -1;
  return Array.from(text, (character) => {
    if (/\p{White_Space}/u.test(character)) {
      return character;
    }
    position += 1;
    return characters[position] ?? character;
  }).join('');
}

/**
 * `text` after the edit that made `from` into `to`, for a text that may differ from `from`: the same place's text as
 * another list holds it. While it has as many characters as `from`, the two line up: only the characters the edit
 * changed change in it, or it becomes `to` when the edit wrote a text of another length, which is a whole text and
 * no change of some characters. A text that does not line up with `from` stays as it is, since nothing says which
 * of its characters the edit is about.
 */
export function withEdit(text: string, from: string, to: string): string {
  if (text === from) {
    return to;
  }
  const before = labelCharacters(from);
  const after = labelCharacters(to);
  if (labelCharacters(text).length !== before.length) {
    return text;
  }
  if (before.length !== after.length) {
    return to;
  }
  return rewritten(
    text,
    after.map((character, at) => (character === before[at] ? undefined : character)),
  );
}

/**
 * The edits of `places` that follow from the answers given this turn, for the shapes `touched`. Only a place that
 * shows a touched shape changes, and in its text only the characters of touched shapes, so a text the user typed by
 * hand is never rewritten by an answer about something else.
 *
 * A place whose text names its shapes gets the answer for each touched shape. One whose text does not gets its whole
 * text from the answers once every one of its shapes has one. A place the check asks about is left out when one of
 * its shapes is no character, and kept once every one has an answer, since the user has then checked all of it. A
 * tick the user changes by hand afterward stays, and the tick of a place the check does not ask about never changes.
 * A box has no tick, so it only gets its text.
 */
function changes(places: readonly Place[], given: Given, touched: ReadonlySet<string>): readonly Change[] {
  return places.flatMap((place): Change[] => {
    if (!place.characters.some((shown) => touched.has(shown.shape))) {
      return [];
    }
    const aside = setAside(place, given.answers);
    const whole = aside ? null : answeredText(place, given.answers);
    let { text } = place;
    if (named(place) !== null) {
      // Spaces the text had stay where they were.
      text = rewritten(
        place.text,
        place.characters.map((shown) => (touched.has(shown.shape) ? given.answers.get(shown.shape) : undefined)),
      );
    } else if (whole !== null) {
      text = whole;
    }
    let { keep } = place;
    if (ticked(place, given.left)) {
      keep = aside ? false : whole !== null || keep;
    }
    return text === place.text && keep === place.keep ? [] : [{ source: place.source, index: place.index, text, keep }];
  });
}

/** The stop for the places of `places` whose text fits no step now, or null when there is none. */
function unfittedStop(places: readonly Place[], given: Given): Cursor | null {
  const keys = unfitted(places, given).flatMap((at) => {
    const place = places[at];
    return place === undefined ? [] : [placeKey(place)];
  });
  return keys.length > 0 ? { at: 'unfitted', places: keys } : null;
}

/**
 * Every stop of the check, in order: each step's character, the places whose text fits no step if there are any,
 * and the end. While the check is at those places, the stop is the one it is at as long as one of its places is
 * still there, whether or not their text fits by now.
 */
function stops(places: readonly Place[], guide: Guide, needs: readonly string[]): readonly Cursor[] {
  const { cursor } = guide;
  const held =
    cursor.at === 'unfitted' && places.some((place) => cursor.places.includes(placeKey(place))) ? cursor : null;
  const fits = held ?? unfittedStop(places, guide);
  return [
    ...steps(places, guide, needs).map((step): Cursor => ({ at: 'character', character: step.character })),
    ...(fits === null ? [] : [fits]),
    { at: 'done' },
  ];
}

function sameStop(one: Cursor, other: Cursor): boolean {
  return (
    one.at === other.at && (one.at !== 'character' || other.at !== 'character' || one.character === other.character)
  );
}

/** The first stop that still asks something: a step with a shape that is not answered, or else the first after them. */
function firstOpen(places: readonly Place[], given: Given, needs: readonly string[]): Cursor {
  const open = steps(places, given, needs).find((step) => step.shapes.some((shape) => !shape.answered));
  if (open !== undefined) {
    return { at: 'character', character: open.character };
  }
  return unfittedStop(places, given) ?? { at: 'done' };
}

/** The names of the places of `places` whose text is read, so they are part of a check. */
function readKeys(places: readonly Place[]): ReadonlySet<string> {
  return new Set(places.flatMap((place) => (place.reading ? [] : [placeKey(place)])));
}

/** A check of `places` that starts at its first step, for a reference that needs the characters `needs`. */
export function openGuide(places: readonly Place[], needs: readonly string[]): Guide {
  const given: Given = { answers: new Map(), left: new Set() };
  return { cursor: firstOpen(places, given, needs), ...given, seen: readKeys(places) };
}

/**
 * `guide` after the places of `places` that are new to it join, and the edits that follow: a place that first shows
 * while the check is open, or whose text the reader sends then, gets every answer that stands for its shapes, as
 * the turn that gave the answer would have written it. Its text takes the answered characters, it is left out for a
 * shape that is no character, and it is kept once every one of its shapes has an answer. Text the user typed stays
 * as it is, so such a place is only left out. A place joins once, so what the user changes afterward stays; one that
 * is gone or being read again joins anew. The same `guide` comes back when nothing joined or left.
 */
export function join(
  guide: Guide,
  places: readonly Place[],
): { readonly guide: Guide; readonly changes: readonly Change[] } {
  const seen = readKeys(places);
  const fresh = places.filter((place) => !place.reading && !guide.seen.has(placeKey(place)));
  if (fresh.length === 0 && seen.size === guide.seen.size) {
    return { guide, changes: [] };
  }
  const left = new Set(guide.left);
  for (const place of fresh) {
    if (place.keep && ticked(place, left) && setAside(place, guide.answers)) {
      left.add(placeKey(place));
    }
  }
  const given: Given = { answers: guide.answers, left };
  const standing = new Set(guide.answers.keys());
  const none = new Set([...standing].filter((shape) => guide.answers.get(shape) === null));
  return {
    guide: { ...guide, left, seen },
    changes: [
      ...changes(
        fresh.filter((place) => !place.typed),
        given,
        standing,
      ),
      ...changes(
        fresh.filter((place) => place.typed),
        given,
        none,
      ),
    ],
  };
}

/**
 * `guide` where it can be for `places` as they are now. The places change under an open check: a text the reader
 * was still reading arrives, the user edits a text by hand, a search or a recording is replaced. A cursor on a
 * character that has no step anymore, or on places that are all gone, moves to the first stop that still asks
 * something.
 */
export function settle(guide: Guide, places: readonly Place[], needs: readonly string[]): Guide {
  const here = stops(places, guide, needs).some((stop) => sameStop(stop, guide.cursor));
  return here ? guide : { ...guide, cursor: firstOpen(places, guide, needs) };
}

/** The places the stop at `cursor` lists, by position: the ones it named when the check came to it. */
export function listed(cursor: Cursor, places: readonly Place[]): readonly number[] {
  return cursor.at === 'unfitted'
    ? places.flatMap((place, at) => (cursor.places.includes(placeKey(place)) ? [at] : []))
    : [];
}

/**
 * `guide` after the user's `turn` on `places`, and the edits of the places that follow from it.
 *
 * Confirm answers each shape of the step at the cursor that is not answered with the step's character, and goes to
 * the next stop. On the stop for text that fits no step, it takes what the user typed for each listed place as the
 * answer for the shapes of it that had none, and goes to the end. Retype
 * answers one shape with another character and stays at the step while it has shapes left; otherwise it goes to the
 * stop that took its place. Set aside answers that a shape is no character and leaves out the kept places that show
 * it; restore takes that back and keeps those places again. Both stay at the step. Skip and back move without
 * answering, and neither moves past the ends. A place that shows afterward gets these answers when it joins, as
 * {@link join} says.
 */
export function turn(
  guide: Guide,
  event: Turn,
  places: readonly Place[],
  needs: readonly string[],
): { readonly guide: Guide; readonly changes: readonly Change[] } {
  const before = stops(places, guide, needs);
  const at = before.findIndex((stop) => sameStop(stop, guide.cursor));
  const { cursor } = guide;
  if (event.kind === 'skip' || event.kind === 'back') {
    const to = Math.max(0, Math.min(before.length - 1, at + (event.kind === 'skip' ? 1 : -1)));
    return { guide: { ...guide, cursor: before[to] ?? { at: 'done' } }, changes: [] };
  }
  const answers = new Map(guide.answers);
  const left = new Set(guide.left);
  const touched = new Set<string>();
  const answer = (shape: string, character: string | null): void => {
    answers.set(shape, character);
    touched.add(shape);
  };
  // What a restored shape's places get back, which the answers alone do not say.
  const kept: Change[] = [];
  if (event.kind === 'confirm' && cursor.at === 'character') {
    const step = steps(places, guide, needs).find((each) => each.character === cursor.character);
    for (const use of step?.shapes ?? []) {
      if (!use.answered) {
        answer(use.shape, cursor.character);
      }
    }
  } else if (event.kind === 'confirm' && cursor.at === 'unfitted') {
    for (const index of listed(cursor, places)) {
      const place = places[index];
      const characters = place === undefined ? null : named(place);
      place?.characters.forEach((shown, position) => {
        const character = characters?.[position];
        if (character !== undefined && !answers.has(shown.shape)) {
          answer(shown.shape, character);
        }
      });
    }
  } else if (event.kind === 'retype') {
    const character = labelCharacters(event.character)[0];
    if (character !== undefined) {
      answer(event.shape, character);
    }
  } else if (event.kind === 'setAside') {
    answer(event.shape, null);
    for (const place of places) {
      if (place.keep && ticked(place, left) && place.characters.some((shown) => shown.shape === event.shape)) {
        left.add(placeKey(place));
      }
    }
  } else if (event.kind === 'restore' && answers.get(event.shape) === null) {
    answers.delete(event.shape);
    for (const place of places) {
      const key = placeKey(place);
      const shows = place.characters.some((shown) => shown.shape === event.shape);
      if (shows && left.has(key) && !setAside(place, answers)) {
        left.delete(key);
        kept.push({ source: place.source, index: place.index, text: place.text, keep: true });
      }
    }
  }
  const given: Given = { answers, left };
  const next: Guide = { ...guide, ...given };
  const edits = [...changes(places, given, touched), ...kept];
  // Where the check goes follows from the places as this turn leaves them, such as a text it just filled in.
  const then = places.map((place) => {
    const edit = edits.find((change) => change.source === place.source && change.index === place.index);
    return edit === undefined ? place : { ...place, text: edit.text, keep: edit.keep };
  });
  const after = stops(then, next, needs);
  let to: Cursor;
  if (event.kind === 'confirm' && cursor.at === 'unfitted') {
    // Not back to a step that still asks: the user skipped it, and Next would never reach the end.
    to = { at: 'done' };
  } else if (event.kind !== 'confirm' && after.some((stop) => sameStop(stop, cursor))) {
    to = cursor;
  } else {
    // The stop after this one, which a confirmed step goes on to and which takes the place of a step left empty.
    const following = before.slice(at + 1).find((stop) => after.some((each) => sameStop(each, stop)));
    to = following ?? firstOpen(then, given, needs);
  }
  return { guide: { ...next, cursor: to }, changes: edits };
}

/**
 * The boxes among `places` that show the shape of `use`, each once however often it shows the shape. A box has no
 * tick, so it still teaches a shape the user said is no character.
 */
export function boxesShowing(use: ShapeUse, places: readonly Place[]): readonly Place[] {
  const boxes = use.spots.flatMap((spot) => {
    const place = places[spot.place];
    return place === undefined || place.ticks ? [] : [place];
  });
  return [...new Set(boxes)];
}

/** How far the check is: the number of the step it is at, from 1, and how many steps there are; null past them. */
export function progress(
  guide: Guide,
  places: readonly Place[],
  needs: readonly string[],
): { readonly step: number; readonly of: number } | null {
  const all = steps(places, guide, needs);
  const { cursor } = guide;
  const at = cursor.at === 'character' ? all.findIndex((step) => step.character === cursor.character) : -1;
  return at === -1 ? null : { step: at + 1, of: all.length };
}

/**
 * The characters of `places` that nobody has confirmed, each once, as they first show: the ones in text that is the
 * text reader's guess, for a shape nobody has said is that character. The user says so with an answer in `answers`,
 * or by typing a text that gives the shape that character in one of `places`; an answer comes before typed text.
 * An answer that a shape is no character confirms it. A guess with another number of characters than its place has
 * shapes names none of its shapes, so every character of it counts, unless the place is a box that shows no shape:
 * no answer could confirm it, so it would count for good.
 */
export function unconfirmed(places: readonly Place[], answers: Answers): readonly string[] {
  const said = new Map<string, string | null>();
  for (const place of places) {
    const characters = place.typed ? named(place) : null;
    place.characters.forEach((shown, at) => {
      const character = characters?.[at];
      if (character !== undefined && !said.has(shown.shape)) {
        said.set(shown.shape, character);
      }
    });
  }
  for (const [shape, answer] of answers) {
    said.set(shape, answer);
  }
  const found = new Set<string>();
  for (const place of places) {
    if (place.typed || place.reading || blank(place)) {
      continue;
    }
    const characters = named(place);
    if (characters === null) {
      for (const character of labelCharacters(place.text)) {
        found.add(character);
      }
      continue;
    }
    place.characters.forEach((shown, at) => {
      const character = characters[at];
      const answer = said.get(shown.shape);
      if (character !== undefined && answer !== null && answer !== character) {
        found.add(character);
      }
    });
  }
  return [...found];
}
