import { useEffect, useRef, useState } from 'react';

import {
  type Guide,
  type Place,
  type ShapeUse,
  type Step,
  type Turn,
  beingRead,
  listed,
  placeKey,
  progress,
  steps,
  unfitted,
} from '../lib/confirm.ts';
import type { Picture } from '../lib/pixels.ts';
import { andList, labelCharacters } from '../lib/reference.ts';
import type { ReviewEntry } from '../lib/review.ts';
import { PictureCanvas } from './canvas.tsx';

/** How many places one shape's tile shows before it counts the rest. */
const SHOWN_PLACES = 6;

interface Props {
  readonly guide: Guide;
  readonly places: readonly Place[];
  /** The characters the reference needs, which come first. */
  readonly needs: readonly string[];
  readonly picture: (place: Place) => Picture | undefined;
  readonly onTurn: (event: Turn) => void;
  readonly onEdit: (place: Place, entry: ReviewEntry) => void;
  readonly onClose: () => void;
}

function plural(count: number, one: string, many: string): string {
  return count === 1 ? `1 ${one}` : `${count} ${many}`;
}

const SOURCE_NAMES = { box: 'Box', find: 'Find', reading: 'Reading' } as const;

/** What a place is called: its box's, find's, or reading's number, as its list shows it. */
function placeName(place: Place): string {
  return `${SOURCE_NAMES[place.source]} ${place.number}`;
}

/** The columns of `picture` that hold the character from `left` to `right` of a place `width` wide, drawn large. */
function Cutout({
  picture,
  left,
  right,
  width,
  label,
}: {
  readonly picture: Picture | undefined;
  readonly left: number;
  readonly right: number;
  readonly width: number;
  readonly label: string;
}) {
  const canvas = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const context = canvas.current?.getContext('2d') ?? null;
    if (canvas.current === null || context === null || picture === undefined || width === 0) {
      return;
    }
    // The picture can be narrower than the place it shows, so the columns are scaled to it.
    const scale = picture.width / width;
    const from = Math.floor(left * scale);
    const columns = Math.max(1, Math.ceil(right * scale) - from);
    canvas.current.width = columns;
    canvas.current.height = picture.height;
    context.putImageData(new ImageData(picture.data, picture.width, picture.height), -from, 0);
  }, [picture, left, right, width]);
  return (
    <span className="cutout">
      {picture === undefined ? null : <canvas ref={canvas} role="img" aria-label={label} />}
    </span>
  );
}

/** A place's whole picture, or an empty box until it arrives. */
function Whole({ picture, label }: { readonly picture: Picture | undefined; readonly label: string }) {
  return (
    <span className="crop">{picture === undefined ? null : <PictureCanvas picture={picture} label={label} />}</span>
  );
}

/** A place's picture with one of its characters marked. */
function Marked({
  place,
  at,
  picture,
}: {
  readonly place: Place;
  readonly at: number;
  readonly picture: Picture | undefined;
}) {
  const shown = place.characters[at];
  if (picture === undefined || shown === undefined || place.width === 0) {
    return <span className="marked empty" />;
  }
  return (
    <span className="marked" style={{ aspectRatio: `${picture.width} / ${picture.height}` }}>
      <PictureCanvas picture={picture} label={`${placeName(place)}, with the character marked`} />
      <span
        className="mark"
        style={{
          left: `${(shown.left / place.width) * 100}%`,
          width: `${((shown.right - shown.left) / place.width) * 100}%`,
        }}
      />
    </span>
  );
}

/**
 * The field that says which character a shape is. It shows the step's character, and typing another answers for the
 * shape once the user presses Enter or leaves the field, not at each key, so the tile stays put while they type.
 */
function Retype({
  character,
  label,
  describedBy,
  onRetype,
}: {
  readonly character: string;
  readonly label: string;
  readonly describedBy: string;
  readonly onRetype: (character: string) => void;
}) {
  // What the user typed and has not committed; null shows the step's character.
  const [typed, setTyped] = useState<string | null>(null);
  const commit = (): void => {
    const [first] = labelCharacters(typed ?? '');
    setTyped(null);
    if (first !== undefined && first !== character) {
      onRetype(first);
    }
  };
  return (
    <input
      className="input"
      value={typed ?? character}
      aria-label={label}
      aria-describedby={describedBy}
      onFocus={(event) => {
        event.target.select();
      }}
      onChange={(event) => {
        // One character: the newest one typed replaces what was there.
        setTyped(labelCharacters(event.target.value).at(-1) ?? '');
      }}
      onBlur={commit}
      onKeyDown={(event) => {
        if (event.key === 'Enter') {
          commit();
        } else if (event.key === 'Escape' && typed !== null) {
          // Takes back what was typed, and is marked as taken so it does not also close the check.
          event.preventDefault();
          setTyped(null);
        }
      }}
    />
  );
}

/** One shape of a step: the shape drawn large, where it shows, and what to do when it is not the step's character. */
function ShapeTile({
  use,
  step,
  number,
  places,
  picture,
  onTurn,
}: {
  readonly use: ShapeUse;
  readonly step: Step;
  readonly number: number;
  readonly places: readonly Place[];
  readonly picture: Props['picture'];
  readonly onTurn: Props['onTurn'];
}) {
  const [first] = use.spots;
  const firstPlace = first === undefined ? undefined : places[first.place];
  const shown = first === undefined ? undefined : firstPlace?.characters[first.at];
  const name = step.shapes.length === 1 ? 'this shape' : `shape ${number}`;
  const noteId = `shape-${use.shape}-note`;
  const state = use.aside ? 'Left out: not a character. ' : use.answered ? 'Checked. ' : '';
  // A box has no tick to clear, so it still teaches a shape that is no character until the user changes the box.
  const boxes = use.aside
    ? use.spots.flatMap((spot) => {
        const place = places[spot.place];
        return place === undefined || place.ticks ? [] : [placeName(place)];
      })
    : [];
  return (
    <li className={use.aside ? 'tile shape off' : use.answered ? 'tile shape kept' : 'tile shape'}>
      <Cutout
        picture={firstPlace === undefined ? undefined : picture(firstPlace)}
        left={shown?.left ?? 0}
        right={shown?.right ?? 0}
        width={firstPlace?.width ?? 0}
        label={`The shape read as ${step.character}`}
      />
      <span id={noteId} className="meta">
        {state}
        Shows in {plural(use.spots.length, 'place', 'places')}.
        {use.also.length === 0 || use.aside ? '' : ` The text of some has it as ${use.also.join(', ')}.`}
        {boxes.length === 0
          ? ''
          : ` ${andList([...new Set(boxes)])} still ${boxes.length === 1 ? 'teaches' : 'teach'} it: delete the box, or draw it around the characters alone.`}
      </span>
      <span className="marks">
        {use.spots.slice(0, SHOWN_PLACES).map((spot) => {
          const place = places[spot.place];
          return place === undefined ? null : (
            <Marked key={`${placeKey(place)}-${spot.at}`} place={place} at={spot.at} picture={picture(place)} />
          );
        })}
        {use.spots.length > SHOWN_PLACES ? (
          <span className="meta">and {use.spots.length - SHOWN_PLACES} more</span>
        ) : null}
      </span>
      {use.aside ? (
        <span className="retype">
          <button
            type="button"
            className="btn small"
            aria-describedby={noteId}
            onClick={() => {
              onTurn({ kind: 'restore', shape: use.shape });
            }}
          >
            It is a character
          </button>
        </span>
      ) : (
        <span className="retype">
          <span className="it-is">
            It is
            <Retype
              character={step.character}
              label={`The character ${name} is`}
              describedBy={noteId}
              onRetype={(character) => {
                onTurn({ kind: 'retype', shape: use.shape, character });
              }}
            />
          </span>
          <button
            type="button"
            className="btn small"
            aria-describedby={noteId}
            onClick={() => {
              onTurn({ kind: 'setAside', shape: use.shape });
            }}
          >
            Not a character
          </button>
        </span>
      )}
    </li>
  );
}

/** The step for one character: its shapes, and the buttons that answer for all of them. */
function CharacterStep({ step, ...props }: Props & { readonly step: Step }) {
  const { guide, places, needs, picture, onTurn } = props;
  const at = progress(guide, places, needs);
  const open = step.shapes.filter((use) => !use.answered).length;
  const several = step.shapes.length > 1;
  const count = step.shapes.reduce((sum, use) => sum + use.spots.length, 0);
  return (
    <>
      <div className="sheet-head">
        <div>
          <h2 tabIndex={-1} data-check-head>
            {several ? 'Are these all' : 'Is this'} <span className="asked">{step.character}</span>?
          </h2>
          <p>
            {several
              ? `The text has ${step.character} drawn ${step.shapes.length} ways, in ${plural(count, 'place', 'places')}. `
              : `It shows in ${plural(count, 'place', 'places')}. `}
            Your answer fixes every place that shows it. If a shape is another character, type that character under it
            and press Enter.
          </p>
        </div>
        <div className="end">
          <button
            type="button"
            className="btn"
            disabled={at === null || at.step === 1}
            onClick={() => {
              onTurn({ kind: 'back' });
            }}
          >
            Back
          </button>
          {open === 0 ? null : (
            <button
              type="button"
              className="btn"
              onClick={() => {
                onTurn({ kind: 'skip' });
              }}
            >
              Skip
            </button>
          )}
          <button
            type="button"
            className="btn primary"
            onClick={() => {
              onTurn({ kind: open === 0 ? 'skip' : 'confirm' });
            }}
          >
            {open === 0 ? 'Next' : several ? `Yes, they are all ${step.character}` : `Yes, it is ${step.character}`}
          </button>
        </div>
      </div>
      <ul className="sheet shapes" aria-label={`Shapes read as ${step.character}`}>
        {step.shapes.map((use, index) => (
          <ShapeTile
            key={use.shape}
            use={use}
            step={step}
            number={index + 1}
            places={places}
            picture={picture}
            onTurn={onTurn}
          />
        ))}
      </ul>
    </>
  );
}

/** The last step: the places whose text has another number of characters than the place shows. */
function UnfittedStep({ guide, places, picture, onTurn, onEdit }: Props) {
  // The places the stop named when the check came to it, so none leaves the list while its text is typed.
  const left = listed(guide.cursor, places);
  return (
    <>
      <div className="sheet-head">
        <div>
          <h2 tabIndex={-1} data-check-head>
            Type what these show
          </h2>
          <p>
            The text of each has another number of characters than the picture shows, so its characters could not be
            checked one at a time. Type what it shows, or leave it out. What you type counts as your answer for its
            characters. A box with no text teaches nothing.
          </p>
        </div>
        <div className="end">
          <button
            type="button"
            className="btn"
            onClick={() => {
              onTurn({ kind: 'back' });
            }}
          >
            Back
          </button>
          <button
            type="button"
            className="btn primary"
            onClick={() => {
              onTurn({ kind: 'confirm' });
            }}
          >
            Next
          </button>
        </div>
      </div>
      <ul className="sheet" aria-label="Text to type">
        {left.map((at) => {
          const place = places[at];
          if (place === undefined) {
            return null;
          }
          const name = placeName(place);
          const noteId = `unfitted-${placeKey(place)}`;
          return (
            <li key={noteId} className={place.keep ? 'tile kept' : 'tile off'}>
              {place.ticks ? (
                <label className="top">
                  <input
                    type="checkbox"
                    aria-label={`Keep ${name}`}
                    checked={place.keep}
                    onChange={(event) => {
                      onEdit(place, { keep: event.target.checked, text: place.text });
                    }}
                  />
                  {name}
                </label>
              ) : (
                <span className="top">{name}</span>
              )}
              <Whole picture={picture(place)} label={name} />
              <input
                className="input"
                placeholder="Type what it shows"
                aria-label={`Text of ${name}`}
                aria-describedby={noteId}
                value={place.text}
                onChange={(event) => {
                  onEdit(place, { keep: place.keep, text: event.target.value });
                }}
              />
              <span id={noteId} className="meta">
                {place.characters.length === 0
                  ? 'No characters were found in its picture.'
                  : labelCharacters(place.text).length === place.characters.length
                    ? `Its text has the ${plural(place.characters.length, 'character', 'characters')} its picture shows.`
                    : `Its picture shows ${plural(place.characters.length, 'character', 'characters')}, and its text has ${labelCharacters(place.text).length}.`}
              </span>
            </li>
          );
        })}
      </ul>
    </>
  );
}

/** The check of a reference's characters, one step at a time, shown in place of the screenshot. */
export function CharacterCheckSheet(props: Props) {
  const { guide, places, needs, onClose } = props;
  const { cursor } = guide;
  const sheet = useRef<HTMLDivElement>(null);
  // Each step replaces the buttons of the last, so focus moves to the new step's heading.
  const stop = cursor.at === 'character' ? `character ${cursor.character}` : cursor.at;
  useEffect(() => {
    sheet.current?.querySelector<HTMLElement>('[data-check-head]')?.focus();
  }, [stop]);
  // An answer can take away the tile that had focus, such as a shape retyped to another step's character. Focus
  // then falls to the page, and goes to the step's heading instead.
  useEffect(() => {
    if (document.activeElement === document.body) {
      sheet.current?.querySelector<HTMLElement>('[data-check-head]')?.focus();
    }
  });
  const step =
    cursor.at === 'character'
      ? steps(places, guide, needs).find((each) => each.character === cursor.character)
      : undefined;
  const all = steps(places, guide, needs);
  const left = unfitted(places, guide).length;
  const unread = beingRead(places);
  const skipped = all.filter((each) => each.shapes.some((use) => !use.answered)).map((each) => each.character);
  return (
    <div ref={sheet} className="check-sheet">
      {step !== undefined ? (
        // Keyed by its character, so the next step's buttons are never the last step's, reused. Pressing a button
        // takes focus from a field, which commits what was typed there and can move the check on before the button
        // is let go; the click then finds no button it was pressed on, and answers for nothing.
        <CharacterStep key={step.character} {...props} step={step} />
      ) : cursor.at === 'unfitted' ? (
        <UnfittedStep {...props} />
      ) : (
        <div className="sheet-head">
          <div>
            <h2 tabIndex={-1} data-check-head>
              {all.length === 0 && left === 0 && unread === 0
                ? 'Nothing to check yet'
                : skipped.length === 0 && left === 0 && unread === 0
                  ? 'Every character is checked'
                  : 'That was the last one'}
            </h2>
            <p>
              {all.length === 0 && left === 0 && unread === 0
                ? 'No box, find, or kept reading shows characters to check. Draw a box around some characters, find more like it, or record the reference, first.'
                : [
                    skipped.length === 0
                      ? 'Each box, find, and reading has the text you confirmed, and a find you checked in full is ticked, ready to add.'
                      : `${skipped.join(', ')} ${skipped.length === 1 ? 'is' : 'are'} not checked yet. The places that show ${skipped.length === 1 ? 'it' : 'them'} keep the text they have.`,
                    left === 0
                      ? null
                      : `${plural(left, 'place still needs its text typed', 'places still need their text typed')}; Back shows ${left === 1 ? 'it' : 'them'}.`,
                    unread === 0
                      ? null
                      : `${plural(unread, 'place is', 'places are')} still being read, and ${unread === 1 ? 'its' : 'their'} characters are asked about once the text arrives.`,
                  ]
                    .filter((part) => part !== null)
                    .join(' ')}
            </p>
          </div>
          <div className="end">
            {all.length === 0 && left === 0 && unread === 0 ? null : (
              <button
                type="button"
                className="btn"
                onClick={() => {
                  props.onTurn({ kind: 'back' });
                }}
              >
                Back
              </button>
            )}
            <button type="button" className="btn primary" onClick={onClose}>
              Done
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
