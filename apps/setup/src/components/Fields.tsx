// The inspector's fields that values and references share.

import { LIMITS, type Reading } from '../lib/bindings.ts';
import { readingText } from '../lib/automation.ts';
import type { CheckState } from '../lib/check.ts';
import { REGION_HINT, TOLERANCE_HINT } from '../lib/hints.ts';
import { type Color, hex } from '../lib/pixels.ts';
import type { Rect } from '../lib/region.ts';
import { Hint } from './Hint.tsx';
import type { ReactNode } from 'react';

export function RegionFields({
  region,
  locked = false,
  onRegion,
}: {
  readonly region: Rect | null;
  /** Whether the region shows but cannot be typed over, as while no screenshot holds it. */
  readonly locked?: boolean;
  readonly onRegion: (region: Rect) => void;
}) {
  const parts = ['x', 'y', 'width', 'height'] as const;
  const names = { x: 'X', y: 'Y', width: 'Width', height: 'Height' } as const;
  return (
    <div className="field">
      <span className="name">
        <span>Region, in the window&apos;s pixels</span>
        <Hint topic="Region">{REGION_HINT}</Hint>
      </span>
      <div className="grid4">
        {parts.map((part) => (
          <input
            key={part}
            className="input"
            type="number"
            min={part === 'width' || part === 'height' ? 1 : 0}
            aria-label={names[part]}
            title={names[part]}
            disabled={region === null || locked}
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
  /** What the ? beside the label explains, if the label needs explaining. */
  readonly hint?: ReactNode;
  readonly color: Color | null;
  readonly picking: boolean;
  readonly canPick: boolean;
  readonly onPick: () => void;
}

export function ColorField({ label, hint, color, picking, canPick, onPick }: ColorProps) {
  return (
    <div className="field">
      {hint === undefined ? (
        <span>{label}</span>
      ) : (
        <span className="name">
          <span>{label}</span>
          <Hint topic={label}>{hint}</Hint>
        </span>
      )}
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
  hint = TOLERANCE_HINT,
  value,
  onChange,
}: {
  readonly help: string;
  /** What the ? beside it explains, which differs where tolerance picks a reference's characters. */
  readonly hint?: string;
  readonly value: number;
  readonly onChange: (value: number) => void;
}) {
  return (
    <div className="field">
      <span className="name">
        <label htmlFor="tolerance">Tolerance</label>
        <Hint topic="Tolerance" id="hint-tolerance">
          {hint}
        </Hint>
      </span>
      <div className="range">
        <input
          id="tolerance"
          type="range"
          min={0}
          max={LIMITS.maxTolerance}
          aria-describedby="hint-tolerance"
          value={value}
          onChange={(event) => {
            onChange(Number(event.target.value));
          }}
        />
        <input
          className="input"
          type="number"
          min={0}
          max={LIMITS.maxTolerance}
          aria-label="Tolerance, typed"
          aria-describedby="hint-tolerance"
          value={value}
          onChange={(event) => {
            const typed = Number.parseInt(event.target.value, 10);
            if (Number.isInteger(typed)) {
              onChange(Math.min(LIMITS.maxTolerance, Math.max(0, typed)));
            }
          }}
        />
      </div>
      {help === '' ? null : <span className="note">{help}</span>}
    </div>
  );
}

/** What the check being set up reads on the live window, why it cannot be read there now, or what it still needs. */
export function ReadingBox({
  built,
  state,
  reading,
  problem,
}: {
  readonly built: string | null;
  readonly state: CheckState;
  readonly reading: Reading | null;
  readonly problem: string | null;
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
      if (reading === null && problem !== null) {
        return (
          <p className="reading warn" aria-live="polite">
            {problem}
          </p>
        );
      }
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
