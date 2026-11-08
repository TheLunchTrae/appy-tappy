// The side of the window a value stays at as the window changes size, picked on a 3 by 3 grid of its sides and
// corners.

import type { PinDef } from '../lib/bindings.ts';
import { PIN_HINT, PIN_NAMES, PINS } from '../lib/sizes.ts';
import { Hint } from './Hint.tsx';

interface Props {
  /** The side picked, or the guess while none is. */
  readonly pin: PinDef;
  /** Whether `pin` is only the guess from where the region is. */
  readonly guessed: boolean;
  readonly onPin: (pin: PinDef) => void;
}

export function PinPicker({ pin, guessed, onPin }: Props) {
  return (
    <div className="field">
      <span className="name">
        <span id="stays-at">Stays at</span>
        <Hint topic="Stays at" id="hint-stays-at">
          {PIN_HINT}
        </Hint>
      </span>
      <div className="toggle-row">
        <div className="pins" role="group" aria-labelledby="stays-at" aria-describedby="hint-stays-at">
          {PINS.map((each) => (
            <button
              key={each}
              type="button"
              aria-label={PIN_NAMES[each]}
              title={PIN_NAMES[each]}
              aria-pressed={pin === each}
              onClick={() => {
                onPin(each);
              }}
            />
          ))}
        </div>
        <span className="note">
          {PIN_NAMES[pin]}
          {guessed ? ', from where the region is' : ''}
        </span>
      </div>
    </div>
  );
}
