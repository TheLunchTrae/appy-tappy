// The inspector's fields that values and references share.

import type { Reading } from '../lib/api.ts';
import { readingText } from '../lib/automation.ts';
import type { CheckState } from '../lib/check.ts';
import { type Color, hex } from '../lib/pixels.ts';
import type { Rect } from '../lib/region.ts';

/** The tolerance's highest value, as a color channel's. */
const MAX_TOLERANCE = 255;

export function RegionFields({
  region,
  onRegion,
}: {
  readonly region: Rect | null;
  readonly onRegion: (region: Rect) => void;
}) {
  const parts = ['x', 'y', 'width', 'height'] as const;
  const names = { x: 'X', y: 'Y', width: 'Width', height: 'Height' } as const;
  return (
    <div className="field">
      <span>Region, in the window&apos;s pixels</span>
      <div className="grid4">
        {parts.map((part) => (
          <input
            key={part}
            className="input"
            type="number"
            min={part === 'width' || part === 'height' ? 1 : 0}
            aria-label={names[part]}
            title={names[part]}
            disabled={region === null}
            value={region?.[part] ?? ''}
            onChange={(event) => {
              const value = Number.parseInt(event.target.value, 10);
              const low = part === 'width' || part === 'height' ? 1 : 0;
              if (region !== null && Number.isInteger(value) && value >= low) {
                onRegion({ ...region, [part]: value });
              }
            }}
          />
        ))}
      </div>
    </div>
  );
}

interface ColorProps {
  readonly label: string;
  readonly color: Color | null;
  readonly picking: boolean;
  readonly canPick: boolean;
  readonly onPick: () => void;
}

export function ColorField({ label, color, picking, canPick, onPick }: ColorProps) {
  return (
    <div className="field">
      <span>{label}</span>
      <div className="colorpick">
        <span className="chip">
          {color === null ? (
            'Not picked'
          ) : (
            <>
              <span className="swatch" style={{ background: hex(color) }} />
              {hex(color)}
            </>
          )}
        </span>
        <button
          type="button"
          className="btn small"
          aria-pressed={picking}
          disabled={!canPick}
          title={canPick ? undefined : 'Select a screenshot first'}
          onClick={onPick}
        >
          {picking ? 'Cancel' : 'Pick'}
        </button>
      </div>
    </div>
  );
}

/** A tolerance, set with the slider or typed in. */
export function ToleranceField({
  help,
  value,
  onChange,
}: {
  readonly help: string;
  readonly value: number;
  readonly onChange: (value: number) => void;
}) {
  return (
    <div className="field">
      <label htmlFor="tolerance">Tolerance</label>
      <div className="range">
        <input
          id="tolerance"
          type="range"
          min={0}
          max={MAX_TOLERANCE}
          value={value}
          onChange={(event) => {
            onChange(Number(event.target.value));
          }}
        />
        <input
          className="input"
          type="number"
          min={0}
          max={MAX_TOLERANCE}
          aria-label="Tolerance, typed"
          value={value}
          onChange={(event) => {
            const typed = Number.parseInt(event.target.value, 10);
            if (Number.isInteger(typed)) {
              onChange(Math.min(MAX_TOLERANCE, Math.max(0, typed)));
            }
          }}
        />
      </div>
      {help === '' ? null : <span className="note">{help}</span>}
    </div>
  );
}

/** What the check being set up reads on the live window, or what it still needs. */
export function ReadingBox({
  built,
  state,
  reading,
}: {
  readonly built: string | null;
  readonly state: CheckState;
  readonly reading: Reading | null;
}) {
  if (built !== null) {
    return <p className="reading note">{built}</p>;
  }
  switch (state.kind) {
    case 'idle':
    case 'pending':
      return <p className="reading note">Trying the check…</p>;
    case 'error':
      return <p className="reading warn">{state.message}</p>;
    case 'set':
      return (
        <p className="reading" aria-live="polite">
          {reading === null ? (
            <small>Waiting for a frame…</small>
          ) : (
            <>
              <strong>{readingText(reading)}</strong>
              <small>on the live window</small>
            </>
          )}
        </p>
      );
  }
}
