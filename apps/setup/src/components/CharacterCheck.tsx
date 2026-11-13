import { useEffect, useRef } from 'react';

import {
  type Guide,
  type Place,
  type ShapeUse,
  type Step,
  type Turn,
  progress,
  steps,
  unfitted,
} from '../lib/confirm.ts';
import type { Picture } from '../lib/pixels.ts';
import { labelCharacters } from '../lib/reference.ts';
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

/** What a place is called: its find's number, as the list and the screenshot show it, or its reading's. */
function placeName(place: Place): string {
  return place.source === 'find' ? `Find ${place.index + 1}` : `Reading ${place.index + 1}`;
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
  return (
    <li className={use.answered ? 'tile shape kept' : 'tile shape'}>
      <Cutout
        picture={firstPlace === undefined ? undefined : picture(firstPlace)}
        left={shown?.left ?? 0}
        right={shown?.right ?? 0}
        width={firstPlace?.width ?? 0}
        label={`The shape read as ${step.character}`}
      />
      <span id={noteId} className="meta">
        {use.answered ? 'Checked. ' : ''}
        Shows in {plural(use.spots.length, 'place', 'places')}.
        {use.also.length === 0 ? '' : ` The text reader also read it as ${use.also.join(', ')}.`}
      </span>
      <span className="marks">
        {use.spots.slice(0, SHOWN_PLACES).map((spot) => {
          const place = places[spot.place];
          return place === undefined ? null : (
            <Marked
              key={`${place.source}-${place.index}-${spot.at}`}
              place={place}
              at={spot.at}
              picture={picture(place)}
            />
          );
        })}
        {use.spots.length > SHOWN_PLACES ? (
          <span className="meta">and {use.spots.length - SHOWN_PLACES} more</span>
        ) : null}
      </span>
      <label className="retype">
        It is
        <input
          className="input"
          // The field always shows the one character the shape has, so typing replaces it.
          value={step.character}
          aria-label={`The character ${name} is`}
          aria-describedby={noteId}
          onFocus={(event) => {
            event.target.select();
          }}
          onChange={(event) => {
            const typed = labelCharacters(event.target.value).filter((character) => character !== step.character);
            const character = typed.at(-1);
            if (character !== undefined) {
              onTurn({ kind: 'retype', shape: use.shape, character });
            }
          }}
        />
        <button
          type="button"
          className="btn small"
          onClick={() => {
            onTurn({ kind: 'setAside', shape: use.shape });
          }}
        >
          Not a character
        </button>
      </label>
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
            Your answer fixes every place that shows it. If a shape is another character, type that character under it.
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
  const left = unfitted(places, guide.answers);
  return (
    <>
      <div className="sheet-head">
        <div>
          <h2 tabIndex={-1} data-check-head>
            Type what these show
          </h2>
          <p>
            The text of each has another number of characters than the picture shows, so its characters could not be
            checked one at a time. Type what it shows, or leave it out.
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
          const noteId = `unfitted-${place.source}-${place.index}`;
          return (
            <li key={noteId} className={place.keep ? 'tile kept' : 'tile off'}>
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
  const stop = cursor.at === 'character' ? cursor.character : cursor.at;
  useEffect(() => {
    sheet.current?.querySelector<HTMLElement>('[data-check-head]')?.focus();
  }, [stop]);
  const step =
    cursor.at === 'character'
      ? steps(places, guide.answers, needs).find((each) => each.character === cursor.character)
      : undefined;
  const all = steps(places, guide.answers, needs);
  const left = unfitted(places, guide.answers).length;
  const skipped = all.filter((each) => each.shapes.some((use) => !use.answered)).map((each) => each.character);
  return (
    <div ref={sheet} className="check-sheet">
      {step !== undefined ? (
        <CharacterStep {...props} step={step} />
      ) : cursor.at === 'unfitted' ? (
        <UnfittedStep {...props} />
      ) : (
        <div className="sheet-head">
          <div>
            <h2 tabIndex={-1} data-check-head>
              {all.length === 0 && left === 0
                ? 'Nothing to check yet'
                : skipped.length === 0 && left === 0
                  ? 'Every character is checked'
                  : 'That was the last one'}
            </h2>
            <p>
              {all.length === 0 && left === 0
                ? 'No find or kept reading has text to check. Find more like a box, or record the reference, first.'
                : [
                    skipped.length === 0
                      ? 'Each place has the text you confirmed, and a find you checked in full is ticked, ready to add.'
                      : `You skipped ${skipped.join(', ')}. The places that show ${skipped.length === 1 ? 'it' : 'them'} keep the text they had.`,
                    left === 0
                      ? null
                      : `${plural(left, 'place still needs its text typed', 'places still need their text typed')}; Back shows ${left === 1 ? 'it' : 'them'}.`,
                  ]
                    .filter((part) => part !== null)
                    .join(' ')}
            </p>
          </div>
          <div className="end">
            {all.length === 0 && left === 0 ? null : (
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
