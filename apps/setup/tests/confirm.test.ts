import { describe, expect, test } from 'bun:test';

import {
  type Guide,
  type Place,
  type Turn,
  changes,
  openGuide,
  progress,
  settle,
  steps,
  turn,
  unfitted,
} from '../src/lib/confirm.ts';
import { checkPlaces } from '../src/lib/useCharacterCheck.ts';

/** A place that shows `shapes`, one name each, two pixels wide each, with `text` as the reader or the user has it. */
function place(source: Place['source'], index: number, shapes: string, text: string, keep = true): Place {
  const characters = Array.from(shapes, (shape, at) => ({ shape, left: at * 2, right: at * 2 + 2 }));
  return { source, index, characters, width: characters.length * 2, text, keep };
}

/** The characters of each step with its shapes, such as `7:ab`. */
const outline = (places: readonly Place[], guide: Guide, needs: readonly string[] = []): string[] =>
  steps(places, guide.answers, needs).map(
    (step) => `${step.character}:${step.shapes.map((use) => use.shape).join('')}`,
  );

/** `places` with `edits` made, as the finder and review make them. */
function edited(places: readonly Place[], edits: ReturnType<typeof turn>['changes']): Place[] {
  return places.map((each) => {
    const edit = edits.find((change) => change.source === each.source && change.index === each.index);
    return edit === undefined ? each : { ...each, text: edit.text, keep: edit.keep };
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
  }
  return { places, guide };
}

// Shape s is 7, e is 8, p is %. The reader misread the 7 of the reading as 1.
const finds = [place('find', 0, 'sep', '78%', false), place('find', 3, 'sp', '7%', false)];
const reading = place('reading', 1, 'se', '18');
const all = [...finds, reading];

describe('the steps of a check', () => {
  test('are one for each character, the needed ones first, with every spot that shows a shape', () => {
    const guide = openGuide(all, ['%', '8']);
    expect(outline(all, guide, ['%', '8'])).toEqual(['%:p', '8:e', '7:s']);
    expect(steps(all, guide.answers, [])[0]).toEqual({
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
    expect(unfitted(places, guide.answers)).toEqual([0, 2]);
  });
});

describe('an answer', () => {
  test('for a character fixes every place that shows its shape, and ticks the places checked in full', () => {
    const first = turn(openGuide(all, []), { kind: 'confirm' }, all, []);
    // The misread reading is fixed; no place is checked in full yet, so the finds stay unticked.
    expect(first.changes).toEqual([{ source: 'reading', index: 1, text: '78', keep: true }]);
    expect(first.guide.cursor).toEqual({ at: 'character', character: '8' });

    const { places, guide } = play(all, [], { kind: 'confirm' }, { kind: 'confirm' }, { kind: 'confirm' });
    expect(places.map((each) => [each.text, each.keep])).toEqual([
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

  test('that a shape is no character leaves out every place that shows it', () => {
    const { places, guide } = play(all, [], { kind: 'setAside', shape: 'e' });
    expect(places.map((each) => each.keep)).toEqual([false, false, false]);
    expect(outline(places, guide)).toEqual(['7:s', '%:p']);
    expect(unfitted(places, guide.answers)).toEqual([]);
  });

  test('keeps the spaces a text has', () => {
    const places = [place('find', 0, 'sep', ' 1 8%')];
    expect(turn(openGuide(places, []), { kind: 'retype', shape: 's', character: '7' }, places, []).changes).toEqual([
      { source: 'find', index: 0, text: ' 7 8%', keep: true },
    ]);
  });

  test('of nothing typed changes nothing', () => {
    const guide = openGuide(all, []);
    expect(turn(guide, { kind: 'retype', shape: 's', character: ' ' }, all, [])).toEqual({ guide, changes: [] });
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
    expect(guide.cursor).toEqual({ at: 'unfitted' });
    expect(unfitted(after, guide.answers)).toEqual([0]);
    expect(turn(guide, { kind: 'confirm' }, after, []).guide.cursor).toEqual({ at: 'done' });
  });
});

describe('moving through a check', () => {
  test('goes back and on without answering, and stops at both ends', () => {
    const start = openGuide(all, []);
    expect(turn(start, { kind: 'back' }, all, [])).toEqual({ guide: start, changes: [] });
    const { guide } = play(all, [], { kind: 'skip' }, { kind: 'skip' }, { kind: 'skip' }, { kind: 'skip' });
    expect(guide).toEqual({ cursor: { at: 'done' }, answers: new Map() });
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
    const other = [place('find', 0, 'sx', '74'), reading];
    expect(settle(guide, other, []).cursor).toEqual({ at: 'character', character: '4' });
    expect(settle(guide, [], []).cursor).toEqual({ at: 'done' });
    expect(settle(guide, all, [])).toBe(guide);
  });

  test('get the answers already given, and keep their ticks', () => {
    const { places, guide } = play(all, [], { kind: 'confirm' });
    // A text the reader was still reading arrives, misread the same way.
    const late = [...places, place('find', 5, 's', '1', false)];
    expect(changes(late, guide.answers, new Set())).toEqual([{ source: 'find', index: 5, text: '7', keep: false }]);
  });
});

describe('the places a check goes over', () => {
  const region = { x: 0, y: 0, width: 9, height: 4 };
  const ink = { red: 255, green: 255, blue: 255 };
  const characters = [{ shape: 's', left: 0, right: 3 }];
  const row = { region, typed: false, reading: false, failed: false, readWith: { ink, tolerance: 40 }, flag: null };

  test('are every listed find, ticked or not, and only the readings review keeps', () => {
    const finds = [0, 1].map(() => ({ region, text: { kind: 'pending' as const }, ink, tolerance: 40, characters }));
    const rows = [
      { ...row, index: 0, keep: false, text: '7', fits: true },
      { ...row, index: 1, keep: true, text: 'x', fits: false },
    ];
    const reading = { firstMs: 0, seen: 1, text: { kind: 'pending' as const }, shapes: [1], width: 5, characters };
    const entries = [
      { keep: false, text: '1' },
      { keep: true, text: '7' },
    ];
    expect(checkPlaces(finds, rows, [reading, reading], entries)).toEqual([
      { source: 'find', index: 0, characters, width: 9, text: '7', keep: false },
      { source: 'reading', index: 1, characters, width: 5, text: '7', keep: true },
    ]);
  });
});
