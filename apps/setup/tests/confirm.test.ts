import { describe, expect, test } from 'bun:test';

import {
  type Guide,
  type Place,
  type Turn,
  beingRead,
  boxesShowing,
  join,
  listed,
  openGuide,
  progress,
  settle,
  steps,
  turn,
  unconfirmed,
  unfitted,
} from '../src/lib/confirm.ts';
import { disagreements, entries } from '../src/lib/review.ts';
import {
  type Held,
  NOTHING_HELD,
  answersOf,
  checkPlaces,
  editedReading,
  escapeCloses,
  heldStep,
  learnedPlaces,
  leftOutOf,
} from '../src/lib/useCharacterCheck.ts';

/** A place that shows `shapes`, one name each, two pixels wide each, with `text` as the reader or the user has it. */
function place(source: Place['source'], index: number, shapes: string, text: string, keep = true): Place {
  const characters = Array.from(shapes, (shape, at) => ({ shape, left: at * 2, right: at * 2 + 2 }));
  return {
    source,
    index,
    number: index + 1,
    characters,
    width: characters.length * 2,
    text,
    typed: false,
    keep,
    ticks: source !== 'box',
    reading: false,
    asks: true,
  };
}

/** The characters of each step with its shapes, such as `7:ab`. */
const outline = (places: readonly Place[], guide: Guide, needs: readonly string[] = []): string[] =>
  steps(places, guide, needs).map((step) => `${step.character}:${step.shapes.map((use) => use.shape).join('')}`);

/** `places` with `edits` made, as the finder and review make them. A reading that is left out is not asked about. */
function edited(places: readonly Place[], edits: ReturnType<typeof turn>['changes']): Place[] {
  return places.map((each) => {
    const edit = edits.find((change) => change.source === each.source && change.index === each.index);
    if (edit === undefined) {
      return each;
    }
    return { ...each, text: edit.text, keep: edit.keep, asks: each.source === 'find' || edit.keep };
  });
}

/** Takes `turns` one after another, making each one's edits before the next. */
function play(start: readonly Place[], needs: readonly string[], ...turns: Turn[]) {
  let places = [...start];
  let guide = openGuide(places, needs);
  for (const event of turns) {
    const next = turn(guide, event, places, needs);
    places = edited(places, next.changes);
    guide = settle(next.guide, places, needs);
    expect(join(guide, places)).toEqual({ guide, changes: [] });
  }
  return { places, guide };
}

const texts = (places: readonly Place[]) => places.map((each) => [each.text, each.keep]);

// Shape s is 7, e is 8, p is %. The reader misread the 7 of the reading as 1.
const finds = [place('find', 0, 'sep', '78%', false), place('find', 3, 'sp', '7%', false)];
const reading = place('reading', 1, 'se', '18');
const all = [...finds, reading];

describe('the steps of a check', () => {
  test('are one for each character, the needed ones first, with every spot that shows a shape', () => {
    const guide = openGuide(all, ['%', '8']);
    expect(outline(all, guide, ['%', '8'])).toEqual(['%:p', '8:e', '7:s']);
    expect(steps(all, guide, [])[0]).toEqual({
      character: '7',
      shapes: [
        {
          shape: 's',
          spots: [
            { place: 0, at: 0 },
            { place: 1, at: 0 },
            { place: 2, at: 0 },
          ],
          also: ['1'],
          aside: false,
          answered: false,
        },
      ],
    });
    expect(guide.cursor).toEqual({ at: 'character', character: '%' });
  });

  test('put a shape with the first of the characters that as many places give it', () => {
    const tied = [place('find', 0, 's', '1'), place('find', 1, 's', '7')];
    expect(outline(tied, openGuide(tied, []))).toEqual(['1:s']);
  });

  test('leave out a place whose text has another number of characters than it has shapes', () => {
    const places = [place('find', 0, 'se', '7'), place('find', 1, 'e', '8'), place('find', 2, '', '')];
    const guide = openGuide(places, []);
    expect(outline(places, guide)).toEqual(['8:e']);
    expect(unfitted(places, guide)).toEqual([0, 2]);
  });

  test('leave out a place that is still being read, which is not text to type either', () => {
    const unread: Place = { ...place('find', 0, 'se', ''), reading: true };
    const places = [unread, place('find', 1, 'e', '8')];
    const guide = openGuide(places, []);
    expect(outline(places, guide)).toEqual(['8:e']);
    expect(unfitted(places, guide)).toEqual([]);
    expect(beingRead(places)).toBe(1);
    // Its text arrives, and its characters are asked about.
    const arrived = [{ ...unread, text: '78', reading: false }, place('find', 1, 'e', '8')];
    expect(outline(arrived, settle(guide, arrived, []))).toEqual(['7:s', '8:e']);
  });
});

describe('an answer', () => {
  test('for a character fixes every place that shows its shape, and ticks the places checked in full', () => {
    const first = turn(openGuide(all, []), { kind: 'confirm' }, all, []);
    // The misread reading is fixed; no place is checked in full yet, so the finds stay unticked.
    expect(first.changes).toEqual([{ source: 'reading', index: 1, text: '78', keep: true }]);
    expect(first.guide.cursor).toEqual({ at: 'character', character: '8' });

    const { places, guide } = play(all, [], { kind: 'confirm' }, { kind: 'confirm' }, { kind: 'confirm' });
    expect(texts(places)).toEqual([
      ['78%', true],
      ['7%', true],
      ['78', true],
    ]);
    expect(guide.cursor).toEqual({ at: 'done' });
  });

  test('that a shape is another character moves it to that character everywhere', () => {
    const places = [place('find', 0, 'ab', '00'), place('reading', 0, 'bc', '01')];
    const next = turn(openGuide(places, []), { kind: 'retype', shape: 'b', character: 'O' }, places, []);
    expect(next.changes).toEqual([
      { source: 'find', index: 0, text: '0O', keep: true },
      { source: 'reading', index: 0, text: 'O1', keep: true },
    ]);
    // The step still has a shape to ask about, so the check stays on it.
    expect(next.guide.cursor).toEqual({ at: 'character', character: '0' });
    const after = edited(places, next.changes);
    expect(outline(after, next.guide)).toEqual(['0:a', 'O:b', '1:c']);
  });

  test('that takes the last shape of a step goes on to the step after it', () => {
    const places = [place('find', 0, 'ab', '01')];
    const next = turn(openGuide(places, []), { kind: 'retype', shape: 'a', character: 'O' }, places, []);
    expect(next.guide.cursor).toEqual({ at: 'character', character: '1' });
  });

  test('keeps the spaces a text has', () => {
    const places = [place('find', 0, 'sep', ' 1 8%')];
    expect(turn(openGuide(places, []), { kind: 'retype', shape: 's', character: '7' }, places, []).changes).toEqual([
      { source: 'find', index: 0, text: ' 7 8%', keep: true },
    ]);
  });

  test('of nothing typed changes nothing', () => {
    const guide = openGuide(all, []);
    const next = turn(guide, { kind: 'retype', shape: 's', character: ' ' }, all, []);
    expect(next.changes).toEqual([]);
    expect(next.guide).toEqual(guide);
  });

  test('leaves a tick the user changed afterward as it is', () => {
    const { places, guide } = play([place('find', 0, 's', '7', false), place('find', 1, 'e', '8', false)], [], {
      kind: 'confirm',
    });
    expect(places.map((each) => each.keep)).toEqual([true, false]);
    const unticked = places.map((each, at) => (at === 0 ? { ...each, keep: false } : each));
    expect(turn(guide, { kind: 'confirm' }, unticked, []).changes).toEqual([
      { source: 'find', index: 1, text: '8', keep: true },
    ]);
  });
});

describe('a reading review leaves out', () => {
  // Review keeps the first, which shows both shapes, and leaves out the second, which shows nothing new.
  const kept = place('reading', 0, 'se', '18');
  const out: Place = { ...place('reading', 1, 's', '1', false), asks: false };

  test('is not asked about, gets the answers, and stays left out', () => {
    const places = [kept, out];
    const guide = openGuide(places, []);
    expect(steps(places, guide, [])[0]?.shapes[0]?.spots).toEqual([{ place: 0, at: 0 }]);
    const next = turn(guide, { kind: 'retype', shape: 's', character: '7' }, places, []);
    expect(next.changes).toEqual([
      { source: 'reading', index: 0, text: '78', keep: true },
      { source: 'reading', index: 1, text: '7', keep: false },
    ]);
  });

  test('does not clash in review with the kept readings the check fixed', () => {
    const cut = (names: string) => ({
      width: names.length * 2,
      characters: Array.from(names, (shape, at) => ({ shape, left: at * 2, right: at * 2 + 2 })),
    });
    const read = (text: string, shapes: number[], names: string) => ({
      firstMs: 0,
      seen: 1,
      size: null,
      text: { kind: 'read' as const, text },
      shapes,
      ...cut(names),
    });
    const readings = [read('18', [0, 1], 'se'), read('1', [0], 's')];
    const start = entries(readings, new Map(), new Map(), new Map(), []);
    expect(start.map((entry) => entry.keep)).toEqual([true, false]);
    const places = checkPlaces([], [], [], readings, start);
    const next = turn(openGuide(places, []), { kind: 'retype', shape: 's', character: '7' }, places, []);
    const edits = new Map(next.changes.map((change) => [change.index, { keep: change.keep, text: change.text }]));
    const after = entries(readings, edits, new Map(), new Map(), []);
    expect(after).toEqual([
      { keep: true, text: '78' },
      { keep: false, text: '7' },
    ]);
    expect(disagreements(readings, after)).toEqual(new Map());
  });
});

describe('a shape that is no character', () => {
  test('leaves out every kept place that shows it, and stays in its step to be taken back', () => {
    const { places, guide } = play(all, [], { kind: 'skip' }, { kind: 'setAside', shape: 'e' });
    expect(places.map((each) => each.keep)).toEqual([false, false, false]);
    expect(guide.cursor).toEqual({ at: 'character', character: '8' });
    expect(guide.left).toEqual(new Set(['reading-1']));
    const eight = steps(places, guide, []).find((step) => step.character === '8');
    expect(eight?.shapes.map((use) => [use.shape, use.aside, use.answered])).toEqual([['e', true, true]]);
    expect(unfitted(places, guide)).toEqual([]);
  });

  test('is a character again once that is taken back, and the places the check left out are kept again', () => {
    const { places, guide } = play(
      all,
      [],
      { kind: 'skip' },
      { kind: 'setAside', shape: 'e' },
      { kind: 'restore', shape: 'e' },
    );
    // The reading was kept before, so it is again; the finds were not ticked, and stay so.
    expect(places.map((each) => each.keep)).toEqual([false, false, true]);
    expect(guide.left).toEqual(new Set());
    expect(guide.answers.has('e')).toBe(false);
    expect(guide.cursor).toEqual({ at: 'character', character: '8' });
  });

  test('keeps a place left out while another of its shapes is still none', () => {
    const { places, guide } = play(
      [reading],
      [],
      { kind: 'setAside', shape: 's' },
      { kind: 'setAside', shape: 'e' },
      { kind: 'restore', shape: 's' },
    );
    expect(places.map((each) => each.keep)).toEqual([false]);
    expect(guide.left).toEqual(new Set(['reading-1']));
  });
});

describe('a box', () => {
  // The user typed the box, and the text reader misread the same 7 in a find and in a reading.
  const drawn = place('box', 4, 'se', '78');
  const places = [drawn, place('find', 0, 's', '1', false), reading];

  test('joins the steps, and one answer fixes it with the finds and the readings', () => {
    const guide = openGuide(places, []);
    expect(outline(places, guide)).toEqual(['1:s', '8:e']);
    expect(turn(guide, { kind: 'retype', shape: 's', character: 'T' }, places, []).changes).toEqual([
      { source: 'box', index: 4, text: 'T8', keep: true },
      { source: 'find', index: 0, text: 'T', keep: true },
      { source: 'reading', index: 1, text: 'T8', keep: true },
    ]);
  });

  test('gets its text from the answers when it has none', () => {
    const empty = [place('box', 4, 'se', ''), place('find', 0, 'se', '78', false)];
    const { places: after } = play(empty, [], { kind: 'confirm' }, { kind: 'confirm' });
    expect(after[0]).toMatchObject({ text: '78', keep: true });
  });

  test('has no tick, so a shape that is no character leaves it as it is, and taking that back gives it nothing', () => {
    const aside = play(places, [], { kind: 'setAside', shape: 's' });
    expect(texts(aside.places)).toEqual([
      ['78', true],
      ['1', false],
      ['18', false],
    ]);
    expect(aside.guide.left).toEqual(new Set(['reading-1']));
    expect(turn(aside.guide, { kind: 'restore', shape: 's' }, aside.places, []).changes).toEqual([
      { source: 'reading', index: 1, text: '18', keep: true },
    ]);
    // One that joins afterward is not remembered as left out either.
    const late = [...aside.places, { ...place('box', 5, 's', '7'), typed: true }];
    const joined = join(aside.guide, late);
    expect(joined.guide.left).toEqual(aside.guide.left);
    expect(joined.changes).toEqual([]);
  });

  test('that shows a shape that is no character twice is named once among the boxes that still teach it', () => {
    const twice = [place('box', 4, 'ss', '11'), place('box', 6, 's', '1'), place('find', 0, 's', '1')];
    const { guide } = play(twice, [], { kind: 'setAside', shape: 's' });
    const [use] = steps(twice, guide, [])[0]?.shapes ?? [];
    expect(use?.spots).toHaveLength(4);
    expect(use === undefined ? [] : boxesShowing(use, twice).map((each) => each.index)).toEqual([4, 6]);
  });

  test('that shows no character is asked nothing and counts for nothing, since no answer could be about it', () => {
    // No character was found in its picture: no text fits it, and it has no tick to leave it out with.
    const bare = place('box', 4, '', '18');
    expect(unfitted([bare], openGuide([bare], []))).toEqual([]);
    expect(openGuide([bare], []).cursor).toEqual({ at: 'done' });
    expect(unconfirmed([bare], new Map())).toEqual([]);
    // A find that shows none has a tick, so it is still text to type or leave out.
    const find = place('find', 0, '', '18');
    expect(unfitted([find], openGuide([find], []))).toEqual([0]);
    expect(unconfirmed([find], new Map())).toEqual(['1', '8']);
  });
});

describe('a reading the draft keeps already', () => {
  // The draft keeps reading 1 as the reader read it, 18, and its 1 is a 7 misread.
  const kept = new Map([[1, '18']]);
  const none = new Map<number, string>();
  /** The one edit of the reading among `edits`, as the entry the hook passes on. */
  const entryOf = (edits: ReturnType<typeof turn>['changes']) => {
    const [edit] = edits.filter((change) => change.source === 'reading');
    return { keep: edit?.keep ?? true, text: edit?.text ?? '' };
  };

  test('goes from the draft for a shape that is no character, and comes back when that is taken back', () => {
    const open = openGuide([reading], []);
    const aside = turn(open, { kind: 'setAside', shape: 's' }, [reading], []);
    const gone = editedReading(kept, none, reading, entryOf(aside.changes));
    expect(gone).toEqual({ text: '18', leftOut: true });
    // The draft keeps it no longer, and review has it left out.
    const left = edited([reading], aside.changes);
    const back = turn(aside.guide, { kind: 'restore', shape: 's' }, left, []);
    expect(back.changes).toEqual([{ source: 'reading', index: 1, text: '18', keep: true }]);
    expect(editedReading(none, new Map([[1, '18']]), left[0] ?? reading, entryOf(back.changes))).toEqual({
      text: '18',
      leftOut: false,
    });
  });

  test("gets the check's answer on the draft, and not what review changed without keeping", () => {
    const fixed = turn(openGuide([reading], []), { kind: 'retype', shape: 's', character: '7' }, [reading], []);
    expect(editedReading(kept, none, reading, entryOf(fixed.changes))).toEqual({ text: '78', leftOut: false });
    // Review has its 8 as a 9, which the user did not keep: the draft gets the 7 alone.
    const reviewed = { ...reading, text: '19', typed: true };
    const again = turn(openGuide([reviewed], []), { kind: 'retype', shape: 's', character: '7' }, [reviewed], []);
    expect(entryOf(again.changes)).toEqual({ keep: true, text: '79' });
    expect(editedReading(kept, none, reviewed, entryOf(again.changes))).toEqual({ text: '78', leftOut: false });
    // A text typed over the whole reading is the check's, whatever the draft had.
    expect(editedReading(kept, none, reviewed, { keep: true, text: '7 8 %' })).toEqual({
      text: '7 8 %',
      leftOut: false,
    });
    // A draft text of another length than review's does not line up with the edit, so it takes none of review's.
    expect(editedReading(new Map([[1, '185']]), none, reviewed, entryOf(again.changes))).toEqual({
      text: '185',
      leftOut: false,
    });
  });

  test('stays on the draft when an answer rewrites its text while review has it left out without Keep', () => {
    // Review cleared its tick, and the user did not press Keep, so the check does not ask about it.
    const unticked = { ...reading, keep: false, asks: false };
    const asked = place('find', 0, 's', '1');
    const both = [asked, unticked];
    const fixed = turn(openGuide(both, []), { kind: 'retype', shape: 's', character: '7' }, both, []);
    expect(entryOf(fixed.changes)).toEqual({ keep: false, text: '78' });
    expect(editedReading(kept, none, unticked, entryOf(fixed.changes))).toEqual({ text: '78', leftOut: false });
    // One the check itself left out stays off the draft through such an edit.
    expect(editedReading(none, new Map([[1, '18']]), unticked, entryOf(fixed.changes))).toEqual({
      text: '78',
      leftOut: true,
    });
  });

  test('is none of the readings review alone keeps, which stay off the draft', () => {
    expect(editedReading(none, none, reading, { keep: true, text: '78' })).toBeNull();
    expect(editedReading(none, none, reading, { keep: false, text: '18' })).toBeNull();
  });
});

describe('what the capture screen holds of the check', () => {
  const answered = play(all, [], { kind: 'confirm' }).guide;
  const open: Held = heldStep(NOTHING_HELD, { kind: 'show', guide: answered });
  const leftOut = { kind: 'reading', recording: 3, index: 1, reading: { text: '18', leftOut: true } } as const;

  test('closing keeps the answers as what the user confirmed, and forgets the readings left out', () => {
    const closed = heldStep(heldStep(open, leftOut), { kind: 'close' });
    expect(closed.guide).toBeNull();
    expect(closed.past).toEqual(answered.answers);
    expect(leftOutOf(closed, 3).size).toBe(0);
    // The next check's answers come after them.
    const next = heldStep(closed, { kind: 'show', guide: { ...answered, answers: new Map([['s', null]]) } });
    expect(answersOf(next).get('s')).toBeNull();
    expect(answersOf(next).size).toBe(answered.answers.size);
  });

  test("the reference's editor closing closes an open check and forgets every answer", () => {
    const closed = heldStep(open, { kind: 'close' });
    for (const held of [open, closed, heldStep(open, leftOut)]) {
      expect(heldStep(held, { kind: 'forget' })).toEqual(NOTHING_HELD);
    }
    expect(answersOf(open).size).toBeGreaterThan(0);
  });

  test('remembers a reading left out until it is kept again, and drops one of another recording', () => {
    const held = heldStep(open, leftOut);
    expect([...leftOutOf(held, 3)]).toEqual([[1, '18']]);
    // A recording that replaced that one has its own readings under the same indexes.
    expect(leftOutOf(held, 4).size).toBe(0);
    const other = heldStep(held, { ...leftOut, recording: 4, index: 2 });
    expect([...leftOutOf(other, 4)]).toEqual([[2, '18']]);
    expect(leftOutOf(other, 3).size).toBe(0);
    const back = heldStep(held, { ...leftOut, reading: { text: '18', leftOut: false } });
    expect(leftOutOf(back, 3).size).toBe(0);
    // An edit of a reading that was never left out changes nothing held.
    expect(heldStep(held, { ...leftOut, index: 5, reading: { text: '4', leftOut: false } })).toBe(held);
  });
});

describe('the characters nobody has checked', () => {
  const guess = place('box', 0, 'sep', '18%');
  const typed = { ...place('box', 1, 'sx', '74'), typed: true };

  test("are the ones of the text reader's guesses, each once, and never the ones the user typed", () => {
    expect(unconfirmed([guess, typed, place('reading', 0, 'se', '18')], new Map())).toEqual(['1', '8', '%']);
    expect(unconfirmed([typed], new Map())).toEqual([]);
  });

  test('leave out a character the user typed for the same shape elsewhere, unless an answer says otherwise', () => {
    const typedToo = { ...place('box', 1, 'ep', '8%'), typed: true };
    expect(unconfirmed([guess, typedToo], new Map())).toEqual(['1']);
    // Typed text that gives the shape another character does not check the guess.
    expect(unconfirmed([guess, { ...place('box', 1, 's', '7'), typed: true }], new Map())).toEqual(['1', '8', '%']);
    expect(unconfirmed([guess, typedToo], new Map([['e', '3']]))).toEqual(['1', '8']);
    // Typed text that does not fit its shapes says nothing about them.
    expect(unconfirmed([guess, { ...place('box', 1, 'ep', '8'), typed: true }], new Map())).toEqual(['1', '8', '%']);
  });

  test('leave out a character whose shape an answer says is that character, or is none', () => {
    const answers = new Map<string, string | null>([
      ['s', '1'],
      ['e', '3'],
      ['p', null],
    ]);
    // The answer for the second shape is another character than the guess has, so nobody has confirmed the guess.
    expect(unconfirmed([guess], answers)).toEqual(['8']);
  });

  test('are every character of a guess that does not fit its shapes, and none of one still being read', () => {
    expect(unconfirmed([place('reading', 0, 'se', '187')], new Map([['s', '1']]))).toEqual(['1', '8', '7']);
    expect(unconfirmed([{ ...guess, reading: true }], new Map())).toEqual([]);
  });
});

describe('text typed by hand while the check is open', () => {
  const places = [place('find', 0, 's', '7'), place('find', 1, 'e', '8')];

  test('is not rewritten by an answer about another shape', () => {
    const first = turn(openGuide(places, []), { kind: 'confirm' }, places, []);
    // The user types 1 over the 7 in the list of finds, then answers for the 8.
    const typed = [{ ...places[0], text: '1' }, places[1]].filter((each): each is Place => each !== undefined);
    const second = turn(settle(first.guide, typed, []), { kind: 'confirm' }, typed, []);
    expect(second.changes).toEqual([]);
  });

  test('that disagrees with an answer makes the step ask again, and the answer then fixes it', () => {
    const first = turn(openGuide(places, []), { kind: 'confirm' }, places, []);
    const typed = [place('find', 0, 's', '1'), place('find', 1, 'e', '8')];
    const guide = settle(first.guide, typed, []);
    const seven = steps(typed, guide, []).find((step) => step.character === '7');
    expect(seven?.shapes.map((use) => [use.also, use.answered])).toEqual([[['1'], false]]);
    // Answering there again writes the answer over what was typed.
    expect(turn({ ...guide, cursor: { at: 'character', character: '7' } }, { kind: 'confirm' }, typed, [])).toEqual({
      guide: { ...guide, cursor: { at: 'character', character: '8' } },
      changes: [{ source: 'find', index: 0, text: '7', keep: true }],
    });
  });

  test('that joins after an answer stays, and its shape is asked about again', () => {
    const { places: after, guide } = play(all, [], { kind: 'confirm' });
    const late = [...after, { ...place('find', 5, 's', '1', false), typed: true }];
    const next = join(guide, late);
    expect(next.changes).toEqual([]);
    const seven = steps(late, next.guide, []).find((step) => step.character === '7');
    expect(seven?.shapes[0]?.answered).toBe(false);
  });
});

describe('a place that joins an open check', () => {
  // The 7 and the 8 are answered, and the percent sign is not.
  const { places: answered, guide } = play(all, [], { kind: 'confirm' }, { kind: 'confirm' });
  const unread: Place = { ...place('find', 5, 'se', '', false), reading: true };

  test('gets the answers that stand for its shapes once its text arrives, and is ticked when they cover it', () => {
    const waiting = join(guide, [...answered, unread]);
    expect(waiting).toEqual({ guide, changes: [] });
    // The text reader misreads the 7 here too.
    const late = [...answered, { ...unread, text: '18', reading: false }];
    const next = join(guide, late);
    expect(next.changes).toEqual([{ source: 'find', index: 5, text: '78', keep: true }]);
    expect(next.guide.seen).toEqual(new Set(['find-0', 'find-3', 'reading-1', 'find-5']));
    expect(next.guide.cursor).toEqual(guide.cursor);
    // It joined, so nothing more follows from it.
    const joined = edited(late, next.changes);
    expect(join(next.guide, joined).guide).toBe(next.guide);
  });

  test('gets an answer for some of its shapes, and stays unticked while another has none', () => {
    const late = [...answered, place('find', 5, 'sp', '1%', false)];
    expect(join(guide, late).changes).toEqual([{ source: 'find', index: 5, text: '7%', keep: false }]);
  });

  test('gets its whole text from the answers when its own does not fit its shapes', () => {
    const late = [...answered, place('reading', 4, 'se', '7')];
    expect(join(guide, late).changes).toEqual([{ source: 'reading', index: 4, text: '78', keep: true }]);
  });

  test('is left out when it shows a shape that is no character, and kept again once that is taken back', () => {
    const aside = play(all, [], { kind: 'skip' }, { kind: 'setAside', shape: 'e' });
    const late = [...aside.places, place('reading', 4, 'se', '18'), place('find', 5, 'e', '8', false)];
    const next = join(aside.guide, late);
    expect(next.changes).toEqual([{ source: 'reading', index: 4, text: '18', keep: false }]);
    expect(next.guide.left).toEqual(new Set(['reading-1', 'reading-4']));
    const joined = edited(late, next.changes);
    // The reading was kept when it joined, so it is again; the find was not ticked, and stays so.
    const back = turn(next.guide, { kind: 'restore', shape: 'e' }, joined, []);
    expect(back.changes).toEqual([
      { source: 'reading', index: 1, text: '18', keep: true },
      { source: 'reading', index: 4, text: '18', keep: true },
    ]);
  });

  test('keeps text the user typed, and is still left out for a shape that is no character', () => {
    const typed = { ...place('find', 5, 'se', '18'), typed: true };
    expect(join(guide, [...answered, typed]).changes).toEqual([]);
    const aside = play(all, [], { kind: 'retype', shape: 's', character: '7' }, { kind: 'setAside', shape: 'e' });
    expect(join(aside.guide, [...aside.places, typed]).changes).toEqual([
      { source: 'find', index: 5, text: '18', keep: false },
    ]);
  });

  test('gets the answers without its keep changing when review leaves it out', () => {
    const out: Place = { ...place('reading', 4, 'se', '18', false), asks: false };
    expect(join(guide, [...answered, out]).changes).toEqual([{ source: 'reading', index: 4, text: '78', keep: false }]);
  });

  test('joins once: a tick the user changes afterward stays', () => {
    const aside = play(all, [], { kind: 'skip' }, { kind: 'setAside', shape: 'e' });
    const ticked = aside.places.map((each) => ({ ...each, keep: true }));
    expect(join(aside.guide, ticked)).toEqual({ guide: aside.guide, changes: [] });
  });

  test('joins again when its text is read again', () => {
    const late = [...answered, { ...unread, text: '18', reading: false }];
    const joined = join(guide, late).guide;
    const again = join(joined, [...answered, unread]).guide;
    expect(again.seen).toEqual(guide.seen);
    expect(join(again, late).changes).toEqual([{ source: 'find', index: 5, text: '78', keep: true }]);
  });

  test('is one of the places a check opens on, which have no answer to get', () => {
    expect(openGuide([...all, unread], []).seen).toEqual(new Set(['find-0', 'find-3', 'reading-1']));
  });
});

describe('a place whose text does not fit its shapes', () => {
  const places = [place('find', 0, 'se', '7', false), place('find', 1, 's', '7'), place('find', 2, 'e', '8')];

  test('gets its text from the answers once every one of its shapes has one', () => {
    const { places: after, guide } = play(places, [], { kind: 'confirm' }, { kind: 'confirm' });
    expect(after[0]).toMatchObject({ text: '78', keep: true });
    expect(guide.cursor).toEqual({ at: 'done' });
  });

  test('is asked about last while a shape of it has no answer', () => {
    const { places: after, guide } = play(places, [], { kind: 'confirm' }, { kind: 'skip' });
    expect(after[0]).toMatchObject({ text: '7', keep: false });
    expect(guide.cursor).toEqual({ at: 'unfitted', places: ['find-0'] });
    expect(listed(guide.cursor, after)).toEqual([0]);
    expect(turn(guide, { kind: 'confirm' }, after, []).guide.cursor).toEqual({ at: 'done' });
  });

  test('stays listed while its text is typed, though the text fits on the way', () => {
    // Four characters typed for three shapes; a backspace away from fitting, and one more from what it shows.
    const long = [place('find', 0, 'abc', '1234')];
    const guide = openGuide(long, []);
    expect(guide.cursor).toEqual({ at: 'unfitted', places: ['find-0'] });
    const fitting = [place('find', 0, 'abc', '123')];
    const settled = settle(guide, fitting, []);
    expect(settled).toBe(guide);
    expect(listed(settled.cursor, fitting)).toEqual([0]);
    // The steps its text now makes do not take the check away from it.
    expect(outline(fitting, settled)).toEqual(['1:a', '2:b', '3:c']);
  });

  test('answers for its shapes with what the user typed there', () => {
    const typed = [place('find', 0, 'abc', '1234', false), place('find', 1, 'a', '7', false)];
    const guide = play(typed, [], { kind: 'skip' }).guide;
    expect(guide.cursor).toEqual({ at: 'unfitted', places: ['find-0'] });
    const fitting = [place('find', 0, 'abc', '123', false), place('find', 1, 'a', '7', false)];
    const next = turn(guide, { kind: 'confirm' }, fitting, []);
    // Every shape of it is answered, so it is ticked, and the other place that shows its first shape gets the 1.
    expect(next.changes).toEqual([
      { source: 'find', index: 0, text: '123', keep: true },
      { source: 'find', index: 1, text: '1', keep: true },
    ]);
    expect(next.guide.cursor).toEqual({ at: 'done' });
  });

  test('goes when its places are gone', () => {
    const guide = play(places, [], { kind: 'confirm' }, { kind: 'skip' }).guide;
    expect(settle(guide, [], []).cursor).toEqual({ at: 'done' });
  });
});

describe('moving through a check', () => {
  test('goes back and on without answering, and stops at both ends', () => {
    const start = openGuide(all, []);
    expect(turn(start, { kind: 'back' }, all, [])).toEqual({ guide: start, changes: [] });
    const { guide } = play(all, [], { kind: 'skip' }, { kind: 'skip' }, { kind: 'skip' }, { kind: 'skip' });
    expect(guide).toEqual({
      cursor: { at: 'done' },
      answers: new Map(),
      left: new Set(),
      seen: new Set(['find-0', 'find-3', 'reading-1']),
    });
    expect(turn(guide, { kind: 'back' }, all, []).guide.cursor).toEqual({ at: 'character', character: '%' });
  });

  test('counts the step it is at among the steps', () => {
    const { guide } = play(all, [], { kind: 'skip' });
    expect(progress(guide, all, [])).toEqual({ step: 2, of: 3 });
    expect(progress({ ...guide, cursor: { at: 'done' } }, all, [])).toBeNull();
  });
});

describe('places that change under a check', () => {
  test('move a cursor whose character is gone to the first step that still asks', () => {
    const { guide } = play(all, [], { kind: 'confirm' }, { kind: 'skip' });
    expect(guide.cursor).toEqual({ at: 'character', character: '%' });
    // A new search has other finds: nothing shows a percent sign anymore.
    const other = [place('find', 0, 'sx', '74'), place('reading', 1, 'se', '78')];
    expect(settle(guide, other, []).cursor).toEqual({ at: 'character', character: '4' });
    expect(settle(guide, [], []).cursor).toEqual({ at: 'done' });
    expect(settle(guide, all, [])).toBe(guide);
  });
});

describe('the places a check goes over', () => {
  const region = { x: 0, y: 0, width: 9, height: 4 };
  const ink = { red: 255, green: 255, blue: 255 };
  const characters = [{ shape: 's', left: 0, right: 3 }];
  const row = { region, typed: false, failed: false, readWith: { ink, tolerance: 40 }, flag: null };

  test('are every box, listed find and reading; it asks about all but readings left out; a save learns the boxes with text and the kept readings', () => {
    const finds = [0, 1, 2].map(() => ({ region, text: { kind: 'pending' as const }, ink, tolerance: 40, characters }));
    const rows = [
      { ...row, index: 0, keep: false, text: '7', reading: false, fits: true },
      { ...row, index: 1, keep: true, text: 'x', reading: false, fits: false },
      { ...row, index: 2, keep: false, text: '', reading: true, fits: true },
    ];
    const recorded = (kind: 'pending' | 'read') => ({
      firstMs: 0,
      seen: 1,
      size: null,
      text: kind === 'pending' ? { kind } : { kind, text: '1' },
      shapes: [1],
      width: 5,
      characters,
    });
    const entries = [
      { keep: false, text: '1' },
      { keep: true, text: '7' },
      { keep: true, text: '' },
    ];
    const box = { screenshot: 0, region };
    const drawn = [
      { box: { ...box, id: 7, text: '7' }, characters },
      { box: { ...box, id: 2, text: '1', guessed: true as const }, characters: null },
    ];
    const readings = [recorded('read'), recorded('read'), recorded('pending')];
    const places = checkPlaces(drawn, finds, rows, readings, entries);
    expect(places.map((each) => [each.source, each.index, each.text, each.keep, each.asks, each.reading])).toEqual([
      ['box', 7, '7', true, true, false],
      ['box', 2, '1', true, true, true],
      ['find', 0, '7', false, true, false],
      ['find', 2, '', false, true, true],
      ['reading', 0, '1', false, false, false],
      ['reading', 1, '7', true, true, false],
      ['reading', 2, '', true, true, true],
    ]);
    // The second reading's text is not the reader's 1, so it is the user's.
    expect(places.map((each) => each.typed)).toEqual([true, false, false, false, false, true, false]);
    // A box has no tick, is numbered as its list shows it, and its text is the user's unless it is still a guess.
    expect(places.map((each) => [each.number, each.ticks])).toEqual([
      [1, false],
      [2, false],
      [1, true],
      [3, true],
      [1, true],
      [2, true],
      [3, true],
    ]);
    // A reference saved now learns from the boxes with text and the readings the draft keeps, with their kept text.
    const blank = { box: { ...box, id: 3, text: ' ' }, characters };
    const learned = learnedPlaces(
      checkPlaces([...drawn, blank], finds, rows, readings, entries),
      readings,
      new Map([
        [0, '1'],
        [1, '7'],
        [9, '4'],
      ]),
    );
    expect(learned.map((each) => [each.source, each.index, each.text, each.keep, each.typed])).toEqual([
      ['box', 7, '7', true, true],
      ['box', 2, '1', true, false],
      ['reading', 0, '1', true, false],
      ['reading', 1, '7', true, true],
    ]);
    expect(places[0]).toMatchObject({ characters, width: 9 });
    expect(places[1]).toMatchObject({ characters: [], width: 9 });
    expect(places[2]).toMatchObject({ characters, width: 9 });
    expect(places[5]).toMatchObject({ characters, width: 5 });
  });
});

describe('escapeCloses', () => {
  const escape = { key: 'Escape', defaultPrevented: false };

  test('Escape closes the check alone while it is open over review, and review once it is not', () => {
    expect(escapeCloses(escape, { checking: true, reviewing: true })).toBe('check');
    expect(escapeCloses(escape, { checking: true, reviewing: false })).toBe('check');
    expect(escapeCloses(escape, { checking: false, reviewing: true })).toBe('review');
    expect(escapeCloses(escape, { checking: false, reviewing: false })).toBeNull();
  });

  test('an Escape that something with focus took closes neither, and no other key does', () => {
    const open = { checking: true, reviewing: true };
    expect(escapeCloses({ key: 'Escape', defaultPrevented: true }, open)).toBeNull();
    expect(escapeCloses({ key: 'Enter', defaultPrevented: false }, open)).toBeNull();
    expect(escapeCloses({ key: 'Escape', defaultPrevented: true }, { checking: false, reviewing: true })).toBeNull();
  });
});
