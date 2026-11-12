// The side of the window a value stays at as the window changes size. It follows where the region is, and says so
// in words; the 3 by 3 grid of the window's sides and corners is there to choose one by hand instead.

import { useEffect, useRef, useState } from 'react';

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
  const byHand = view.kind === 'byHand';
  const grid = pin !== null && (byHand || choosing);
  // Back to automatic gives way to Change, which takes the focus it had.
  const change = useRef<HTMLButtonElement>(null);
  const returned = useRef(false);
  useEffect(() => {
    if (returned.current && !byHand) {
      returned.current = false;
      change.current?.focus();
    }
  }, [byHand]);
  return (
    <div className="field" role="group" aria-labelledby="stays-at">
      <span className="name">
        <span id="stays-at">Stays at</span>
        <Hint topic="Stays at" id="hint-stays-at">
          {PIN_HINT}
        </Hint>
      </span>
      <div>
        <div className="toggle-row">
          <span id="stays-at-now">{pinSummary(view)}</span>
          {byHand ? (
            <button
              key="automatic"
              type="button"
              className="btn small"
              aria-describedby="stays-at-now"
              onClick={() => {
                returned.current = true;
                setChoosing(false);
                onChoice(AUTOMATIC);
              }}
            >
              Back to automatic
            </button>
          ) : pin === null ? null : (
            <button
              key="change"
              ref={change}
              type="button"
              className="btn small"
              aria-describedby="stays-at-now"
              aria-expanded={choosing}
              aria-controls={choosing ? 'stays-at-grid' : undefined}
              onClick={() => {
                setChoosing(!choosing);
              }}
            >
              Change
            </button>
          )}
        </div>
        {/* Always there, so a screen reader hears the notice when its text comes. */}
        <p className="warn pin-notice" role="status">
          {pinNotice(view)}
        </p>
      </div>
      {grid ? (
        <div id="stays-at-grid" className="pins" role="group" aria-label="Side" aria-describedby="hint-stays-at">
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
        <p className="note">
          {pinMeaning(pin)}
          {scales
            ? ''
            : " This application's window rarely changes size, so the value is read only at the sizes it is drawn " +
              'at and the side is not used.'}
        </p>
      )}
    </div>
  );
}
