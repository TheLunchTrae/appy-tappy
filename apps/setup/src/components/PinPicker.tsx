// The side of the window a value stays at as the window changes size. It follows where the region is, and says so
// in words; the 3 by 3 grid of the window's sides and corners is there to choose one by hand instead.

import { useState } from 'react';

import type { PinDef } from '../lib/bindings.ts';
import { AUTOMATIC, type PinChoice, type PinView, pinMeaning, pinNotice, pinOfView, pinSummary } from '../lib/pin.ts';
import { PIN_HINT, PIN_NAMES, PINS } from '../lib/sizes.ts';
import { Hint } from './Hint.tsx';

interface Props {
  /** The side, and whether it follows the region or was chosen by hand. */
  readonly view: PinView;
  /** Whether the application's values scale with its window, so the side matters. */
  readonly scales: boolean;
  readonly onChoice: (choice: PinChoice) => void;
}

export function PinPicker({ view, scales, onChoice }: Props) {
  // Whether the grid is open to choose a side while the side still follows the region.
  const [choosing, setChoosing] = useState(false);
  const pin = pinOfView(view);
  const notice = pinNotice(view);
  const byHand = view.kind === 'byHand';
  return (
    <div className="field">
      <span className="name">
        <span id="stays-at">Stays at</span>
        <Hint topic="Stays at" id="hint-stays-at">
          {PIN_HINT}
        </Hint>
      </span>
      <div className="toggle-row">
        <span>{pinSummary(view)}</span>
        {byHand ? (
          <button
            type="button"
            className="btn small"
            onClick={() => {
              setChoosing(false);
              onChoice(AUTOMATIC);
            }}
          >
            Back to automatic
          </button>
        ) : pin === null ? null : (
          <button
            type="button"
            className="btn small"
            aria-expanded={choosing}
            aria-controls="stays-at-grid"
            onClick={() => {
              setChoosing(!choosing);
            }}
          >
            Change
          </button>
        )}
      </div>
      {notice === null ? null : (
        <p className="warn" role="status">
          {notice}
        </p>
      )}
      {pin !== null && (byHand || choosing) ? (
        <div
          id="stays-at-grid"
          className="pins"
          role="group"
          aria-labelledby="stays-at"
          aria-describedby="hint-stays-at"
        >
          {PINS.map((each: PinDef) => (
            <button
              key={each}
              type="button"
              aria-label={PIN_NAMES[each]}
              title={PIN_NAMES[each]}
              aria-pressed={pin === each}
              onClick={() => {
                setChoosing(false);
                onChoice({ kind: 'byHand', pin: each });
              }}
            />
          ))}
        </div>
      ) : null}
      {pin === null ? null : (
        <span className="note">
          {pinMeaning(pin)}
          {scales
            ? ''
            : " This application's window rarely changes size, so the value is read only at the sizes it is drawn " +
              'at and the side is not used.'}
        </span>
      )}
    </div>
  );
}
