import { useState } from 'react';

import {
  LETTER_PRESETS,
  NUMBER_PRESETS,
  type Preset,
  TOO_MANY,
  otherCharacters,
  presetCount,
  presetState,
  tooMany,
  withOther,
  withPreset,
} from '../lib/expected.ts';
import { EXPECTED_HINT } from '../lib/hints.ts';
import { Hint } from './Hint.tsx';

interface Props {
  /** The characters the value needs, each once; none for every character its reference knows. */
  readonly expected: readonly string[];
  readonly onExpected: (expected: readonly string[]) => void;
}

/** One group of presets, each a checkbox that is mixed while the set holds only some of its characters. */
function Presets({
  title,
  presets,
  expected,
  onExpected,
}: Props & { readonly title: string; readonly presets: readonly Preset[] }) {
  return (
    <div className="needs" role="group" aria-label={title}>
      {presets.map((preset) => {
        const state = presetState(expected, preset);
        return (
          <label key={preset.name} className="need">
            <input
              type="checkbox"
              checked={state === 'on'}
              ref={(box) => {
                if (box !== null) {
                  box.indeterminate = state === 'partial';
                }
              }}
              onChange={(event) => {
                onExpected(withPreset(expected, preset, event.target.checked));
              }}
            />
            {preset.name}
            <span className="glyph">{preset.shown}</span>
            {state === 'partial' ? (
              <small>
                {presetCount(expected, preset)} of {preset.characters.length}
              </small>
            ) : null}
          </label>
        );
      })}
    </div>
  );
}

/**
 * The characters a number value needs: presets to turn on, and a field for any other character. It edits one set of
 * characters, which the value reads.
 */
export function ExpectedCharacters({ expected, onExpected }: Props) {
  const label = 'Characters it needs';
  // The field's text as typed, and the set it started from, while it is being typed in. Outside that, the field
  // shows the set's own other characters, in order.
  const [typing, setTyping] = useState<{ readonly start: readonly string[]; readonly text: string } | null>(null);
  return (
    <div className="field">
      <span className="name">
        <span>{label}</span>
        <Hint topic={label}>{EXPECTED_HINT}</Hint>
      </span>
      <span className="note">
        {expected.length === 0
          ? 'Nothing chosen, so the value reads every character its reference knows, and any it learns later.'
          : 'Turn on everything the value shows: its digits and any sign right after them, such as the % in 87%.'}
      </span>
      <Presets title="Numbers" presets={NUMBER_PRESETS} expected={expected} onExpected={onExpected} />
      <span className="note">
        Letters. A number value reads no text. Turn these on for letters its region shows with the number, such as the
        Lv in Lv.5.
      </span>
      <Presets title="Letters" presets={LETTER_PRESETS} expected={expected} onExpected={onExpected} />
      <label htmlFor="other-characters">Other characters</label>
      <input
        id="other-characters"
        className="input"
        placeholder="Such as / or °"
        autoComplete="off"
        autoCapitalize="off"
        spellCheck={false}
        value={typing?.text ?? otherCharacters(expected)}
        onChange={(event) => {
          const start = typing?.start ?? expected;
          setTyping({ start, text: event.target.value });
          onExpected(withOther(start, event.target.value));
        }}
        onBlur={() => {
          setTyping(null);
        }}
      />
      {tooMany(expected) ? (
        <span className="warn" role="alert">
          {TOO_MANY}
        </span>
      ) : null}
    </div>
  );
}
