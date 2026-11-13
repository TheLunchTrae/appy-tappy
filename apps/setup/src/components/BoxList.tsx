import { useMemo } from 'react';

import { type Box, boxNumber } from '../lib/boxes.ts';
import { BOXES_HINT } from '../lib/hints.ts';
import { type Picture, cropPicture } from '../lib/pixels.ts';
import { PictureCanvas } from './canvas.tsx';
import { Hint } from './Hint.tsx';

interface Props {
  readonly boxes: readonly Box[];
  readonly selected: number | null;
  /** Each screenshot at its own pixels, once they arrive. */
  readonly full: ReadonlyMap<number, Picture>;
  /** The screenshot's number as the strip shows it. */
  readonly position: (screenshot: number) => number;
  /** Why no box can be drawn on the screenshot shown, or null when one can. */
  readonly blocked: string | null;
  /** The box the reference reads and records at, or null when it reads elsewhere, such as at a value's region. */
  readonly readsAt: number | null;
  readonly onSelect: (id: number) => void;
  readonly onText: (id: number, text: string) => void;
  readonly onDelete: (id: number) => void;
}

/** The id of the field that holds what box `id` shows, which takes focus once the box is drawn. */
export function boxTextId(id: number): string {
  return `box-${id}-text`;
}

/** A box's pixels, or an empty space until its screenshot's pixels arrive. */
function Crop({ box, full }: { readonly box: Box; readonly full: ReadonlyMap<number, Picture> }) {
  const picture = full.get(box.screenshot);
  const crop = useMemo(() => (picture === undefined ? null : cropPicture(picture, box.region)), [picture, box.region]);
  return <span className="crop">{crop === null ? null : <PictureCanvas picture={crop} label="" />}</span>;
}

/** The boxes a reference learns from: each one's pixels, what it shows, and a way to select or delete it. */
export function BoxList({ boxes, selected, full, position, blocked, readsAt, onSelect, onText, onDelete }: Props) {
  return (
    <div className="field">
      <span className="name">
        <span id="boxes-title">Boxes</span>
        <Hint topic="Boxes" id="hint-boxes">
          {BOXES_HINT}
        </Hint>
      </span>
      {boxes.length === 0 ? (
        <p className="note">Drag on a screenshot to draw a box around some characters.</p>
      ) : (
        <ul className="box-list" aria-labelledby="boxes-title">
          {boxes.map((box) => {
            const number = boxNumber(boxes, box.id);
            return (
              <li key={box.id} className={box.id === selected ? 'selected' : undefined}>
                <button
                  type="button"
                  className="pick"
                  aria-pressed={box.id === selected}
                  aria-label={`Select box ${number}`}
                  onClick={() => {
                    onSelect(box.id);
                  }}
                >
                  {number}
                </button>
                <Crop box={box} full={full} />
                <input
                  id={boxTextId(box.id)}
                  className="input"
                  placeholder="87%"
                  aria-label={`What box ${number} shows`}
                  aria-describedby="hint-boxes"
                  value={box.text}
                  onFocus={() => {
                    onSelect(box.id);
                  }}
                  onChange={(event) => {
                    onText(box.id, event.target.value);
                  }}
                />
                <button
                  type="button"
                  className="btn small"
                  aria-label={`Delete box ${number}`}
                  onClick={() => {
                    onDelete(box.id);
                  }}
                >
                  Delete
                </button>
                <span className="where">
                  On screenshot {position(box.screenshot)}
                  {box.id === readsAt ? <span className="reads-here">Reads here</span> : null}
                </span>
              </li>
            );
          })}
        </ul>
      )}
      {blocked === null ? null : <p className="note">{blocked}</p>}
    </div>
  );
}
