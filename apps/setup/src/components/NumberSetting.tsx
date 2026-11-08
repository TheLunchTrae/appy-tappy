import { useEffect, useState } from 'react';

import type { SettingsFile } from '../lib/bindings.ts';
import { errorText } from '../lib/api.ts';
import { readWhole, useSettings } from '../lib/settings.ts';

interface NumberProps {
  readonly id: string;
  readonly label: string;
  readonly note: string;
  readonly unit: string;
  /** The saved number. */
  readonly saved: number;
  readonly min: number;
  readonly max: number;
  /** What to fix when the field does not hold a whole number from `min` to `max`. */
  readonly invalid: string;
  /** The settings with the new number, made from the current ones, or what to fix. */
  readonly update: (current: SettingsFile, value: number) => SettingsFile | { readonly problem: string };
}

/**
 * A whole-number setting, saved when the field loses focus or Enter is pressed. A number that cannot be saved puts the
 * saved one back in the field and says why under it, so the field never shows a number the runtime does not use.
 */
export function NumberSetting({ id, label, note, unit, saved, min, max, invalid, update }: NumberProps) {
  const { change } = useSettings();
  const [text, setText] = useState(String(saved));
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    setText(String(saved));
  }, [saved]);
  const refuse = (problem: string): void => {
    setText(String(saved));
    setError(problem);
  };
  const commit = (): void => {
    const value = readWhole(text, min, max);
    if (value === null) {
      refuse(invalid);
      return;
    }
    if (value === saved) {
      setError(null);
      return;
    }
    change((current) => update(current, value)).then(
      (problem) => {
        if (problem === null) {
          setError(null);
        } else {
          refuse(problem);
        }
      },
      (failure: unknown) => {
        refuse(errorText(failure));
      },
    );
  };
  const described = [`${id}-note`, `${id}-unit`, ...(error === null ? [] : [`${id}-error`])].join(' ');
  return (
    <div className="set-row">
      <div className="what">
        <label htmlFor={id}>{label}</label>
        <span className="note" id={`${id}-note`}>
          {note}
        </span>
        {error === null ? null : (
          <span className="warn" id={`${id}-error`} role="alert">
            {error}
          </span>
        )}
      </div>
      <span className="num">
        <input
          id={id}
          className="input"
          inputMode="numeric"
          aria-invalid={error !== null}
          aria-describedby={described}
          value={text}
          onChange={(event) => {
            setText(event.target.value);
          }}
          onBlur={commit}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              commit();
            }
          }}
        />
        <span id={`${id}-unit`}>{unit}</span>
      </span>
    </div>
  );
}
