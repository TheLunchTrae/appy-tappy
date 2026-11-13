// Checking a reference's characters one at a time, across every place that shows them.
//
// A reference learns a character from each place whose text names it: a find from Find more like it, or a recorded
// reading. The text reader's guesses fill those texts in, and it tends to misread one shape the same way wherever
// the shape shows. So the check goes by shape instead of by place. Each step is one character with every shape the
// texts give it, and one answer for a shape rewrites the text of every place that shows the shape.
//
// The places' texts stay the one record of what each place shows. The check holds only the answers given so far, by
// shape, and where it is. Every change it makes is an edit of a place's text or its tick, the same edit the user
// makes by hand in the list of finds or in review, so the two can be used together and never disagree.
//
// A place whose text has another number of characters than the place has shapes gives its shapes no characters. It
// joins no step. Once every one of its shapes has an answer from the other places, its text is written from the
// answers. One that is still left shows at the end, to type or leave out.

import type { ShownCharacter } from './bindings.ts';
import { labelCharacters } from './reference.ts';

/** One place a reference would learn characters from. */
export interface Place {
  /** Whether it is a find or a recorded reading, and its index there. */
  readonly source: 'find' | 'reading';
  readonly index: number;
  /** Where each of its characters is in its picture, with the name of its shape. */
  readonly characters: readonly ShownCharacter[];
  /** How wide its picture is, in the pixels the characters are placed in. */
  readonly width: number;
  readonly text: string;
  /** Whether it is ticked or kept, so the reference learns from it. */
  readonly keep: boolean;
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
  /** The other characters some places' texts give this shape, which an answer overrules. */
  readonly also: readonly string[];
  /** Whether the user has answered for it. */
  readonly answered: boolean;
}

/** One step of the check: a character, and the shapes the places' texts give it. */
export interface Step {
  readonly character: string;
  readonly shapes: readonly ShapeUse[];
}

/** Where the check is: at a character's step, at the places whose text fits no step, or past them all. */
type Cursor =
  | { readonly at: 'character'; readonly character: string }
  | { readonly at: 'unfitted' }
  | { readonly at: 'done' };

/** A check that is open. */
export interface Guide {
  readonly cursor: Cursor;
  readonly answers: Answers;
}

/** What the user does in a step. */
export type Turn =
  /** Every shape of the step is its character. */
  | { readonly kind: 'confirm' }
  /** Goes on without answering. */
  | { readonly kind: 'skip' }
  | { readonly kind: 'back' }
  /** One shape is another character than its step's. */
  | { readonly kind: 'retype'; readonly shape: string; readonly character: string }
  /** One shape is no character, so the places that show it are left out. */
  | { readonly kind: 'setAside'; readonly shape: string };

/** An edit of one place: its text and whether it is ticked or kept. */
export interface Change {
  readonly source: Place['source'];
  readonly index: number;
  readonly text: string;
  readonly keep: boolean;
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
 * The steps for `places` with the `answers` so far: each character with its shapes, the characters among `needs`
 * first, in that order, then the rest as they first show. A shape goes with its answer, or else with the character
 * most places' texts give it, the first of equals.
 */
export function steps(places: readonly Place[], answers: Answers, needs: readonly string[]): readonly Step[] {
  const uses = new Map<string, { spots: Spot[]; votes: Map<string, number> }>();
  places.forEach((place, at) => {
    const characters = named(place);
    if (characters === null || setAside(place, answers)) {
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
    const answer = answers.get(shape);
    const most = Math.max(...use.votes.values());
    const voted = [...use.votes].find(([, count]) => count === most)?.[0] ?? '';
    const character = answer ?? voted;
    const shapes = byCharacter.get(character) ?? [];
    byCharacter.set(character, shapes);
    shapes.push({
      shape,
      spots: use.spots,
      also: [...use.votes.keys()].filter((other) => other !== character),
      answered: answer !== undefined,
    });
  }
  const order = [
    ...needs.filter((character) => byCharacter.has(character)),
    ...[...byCharacter.keys()].filter((character) => !needs.includes(character)),
  ];
  return order.map((character) => ({ character, shapes: byCharacter.get(character) ?? [] }));
}

/** `place`'s text by the answers alone, or null unless each of its shapes has a character for an answer. */
function answeredText(place: Place, answers: Answers): string | null {
  const characters = place.characters.map((shown) => answers.get(shown.shape) ?? null);
  return characters.length > 0 && characters.every((character) => character !== null) ? characters.join('') : null;
}

/**
 * The places, by position, whose text fits no step and that the answers cannot fill in: the text has another number
 * of characters than the place has shapes, or the place shows no shape at all, and some shape of it has no answer.
 */
export function unfitted(places: readonly Place[], answers: Answers): readonly number[] {
  return places.flatMap((place, at) =>
    named(place) !== null || setAside(place, answers) || answeredText(place, answers) !== null ? [] : [at],
  );
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
 * The edits that bring `places` in line with `answers`.
 *
 * Every place's text follows the answers: one whose every shape has an answer gets its text from them, and any
 * other whose text names its shapes gets the answers for the shapes that have one. So a text that arrives late, or
 * that the user types against an answer, comes out as the answers have it while the check is open.
 *
 * Whether a place is kept follows only the answers given in this turn, for the shapes `touched`: a place that shows
 * a touched shape is left out if any of its shapes is no character, and kept once every one has an answer, since
 * the user has then checked all of it. A tick the user changes by hand afterward stays.
 */
export function changes(places: readonly Place[], answers: Answers, touched: ReadonlySet<string>): readonly Change[] {
  return places.flatMap((place): Change[] => {
    const aside = setAside(place, answers);
    const whole = aside ? null : answeredText(place, answers);
    const fits = named(place) !== null;
    let { text } = place;
    if (!aside && whole !== null) {
      // Spaces the text had stay where they were when it had one character for each shape.
      text = fits ? rewritten(place.text, Array.from(whole)) : whole;
    } else if (!aside && fits) {
      text = rewritten(
        place.text,
        place.characters.map((shown) => answers.get(shown.shape)),
      );
    }
    const turned = place.characters.some((shown) => touched.has(shown.shape));
    const keep = turned && aside ? false : turned && whole !== null ? true : place.keep;
    return text === place.text && keep === place.keep ? [] : [{ source: place.source, index: place.index, text, keep }];
  });
}

/** Every stop of the check, in order: each step's character, the unfitted places if there are any, and the end. */
function stops(places: readonly Place[], answers: Answers, needs: readonly string[]): readonly Cursor[] {
  return [
    ...steps(places, answers, needs).map((step): Cursor => ({ at: 'character', character: step.character })),
    ...(unfitted(places, answers).length > 0 ? [{ at: 'unfitted' } as const] : []),
    { at: 'done' },
  ];
}

function sameStop(one: Cursor, other: Cursor): boolean {
  return (
    one.at === other.at && (one.at !== 'character' || other.at !== 'character' || one.character === other.character)
  );
}

/** The first stop that still asks something: a step with a shape that has no answer, or else the first after them. */
function firstOpen(places: readonly Place[], answers: Answers, needs: readonly string[]): Cursor {
  const open = steps(places, answers, needs).find((step) => step.shapes.some((shape) => !shape.answered));
  if (open !== undefined) {
    return { at: 'character', character: open.character };
  }
  return unfitted(places, answers).length > 0 ? { at: 'unfitted' } : { at: 'done' };
}

/** A check of `places` that starts at its first step, for a reference that needs the characters `needs`. */
export function openGuide(places: readonly Place[], needs: readonly string[]): Guide {
  const answers: Answers = new Map();
  return { cursor: firstOpen(places, answers, needs), answers };
}

/**
 * `guide` where it can be for `places` as they are now. The places change under an open check: a text the reader
 * was still reading arrives, a search or a recording is replaced. A cursor on a character that has no step anymore,
 * or on unfitted places that are gone, moves to the first stop that still asks something.
 */
export function settle(guide: Guide, places: readonly Place[], needs: readonly string[]): Guide {
  const here = stops(places, guide.answers, needs).some((stop) => sameStop(stop, guide.cursor));
  return here ? guide : { ...guide, cursor: firstOpen(places, guide.answers, needs) };
}

/**
 * `guide` after the user's `turn` on `places`, and the edits of the places that follow from it.
 *
 * Confirm answers each shape of the step at the cursor that has no answer yet with the step's character, and goes
 * to the next stop. Retype answers one shape with another character, and set aside answers that it is none; both
 * stay at the step while it has shapes left, and otherwise go to the stop that took its place. Skip and back move
 * without answering, and neither moves past the ends.
 */
export function turn(
  guide: Guide,
  event: Turn,
  places: readonly Place[],
  needs: readonly string[],
): { readonly guide: Guide; readonly changes: readonly Change[] } {
  const before = stops(places, guide.answers, needs);
  const at = before.findIndex((stop) => sameStop(stop, guide.cursor));
  const moved = (by: number): Guide => ({
    ...guide,
    cursor: before[Math.max(0, Math.min(before.length - 1, at + by))] ?? { at: 'done' },
  });
  if (event.kind === 'skip' || event.kind === 'back') {
    return { guide: moved(event.kind === 'skip' ? 1 : -1), changes: [] };
  }
  const answers = new Map(guide.answers);
  if (event.kind === 'confirm') {
    if (guide.cursor.at !== 'character') {
      return { guide: moved(1), changes: [] };
    }
    const { character } = guide.cursor;
    const step = steps(places, guide.answers, needs).find((each) => each.character === character);
    for (const use of step?.shapes ?? []) {
      if (!use.answered) {
        answers.set(use.shape, character);
      }
    }
  } else if (event.kind === 'retype') {
    const character = labelCharacters(event.character)[0];
    if (character === undefined) {
      return { guide, changes: [] };
    }
    answers.set(event.shape, character);
  } else {
    answers.set(event.shape, null);
  }
  // The stop after this one, which a confirmed step goes on to and which takes the place of a step left empty.
  const following = before.slice(at + 1);
  const after = stops(places, answers, needs);
  const still = event.kind !== 'confirm' && after.some((stop) => sameStop(stop, guide.cursor));
  const cursor = still
    ? guide.cursor
    : (following.find((stop) => after.some((each) => sameStop(each, stop))) ?? firstOpen(places, answers, needs));
  const touched = new Set(
    [...answers].filter(([shape, answer]) => guide.answers.get(shape) !== answer).map(([shape]) => shape),
  );
  return { guide: { cursor, answers }, changes: changes(places, answers, touched) };
}

/** How far the check is: the number of the stop it is at, from 1, and how many steps there are; null past them. */
export function progress(
  guide: Guide,
  places: readonly Place[],
  needs: readonly string[],
): { readonly step: number; readonly of: number } | null {
  const all = steps(places, guide.answers, needs);
  const { cursor } = guide;
  const at = cursor.at === 'character' ? all.findIndex((step) => step.character === cursor.character) : -1;
  return at === -1 ? null : { step: at + 1, of: all.length };
}
