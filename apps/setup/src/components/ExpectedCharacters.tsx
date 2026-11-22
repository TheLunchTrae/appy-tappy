import { useId, useState } from 'react';

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
import { EXPECTED_HINT, READS_HINT } from '../lib/hints.ts';
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

/** What the chooser says for the characters of a value, or of a reference, where "it" is the reference. */
const WORDS = {
  value: {
    hint: EXPECTED_HINT,
    none: 'Nothing chosen, so the value reads every character its reference reads, and any it learns later.',
    some: 'Turn on everything the value shows: its digits and any sign right after them, such as the % in 87%.',
    letters:
      'Letters. A number value reads no text. Turn these on for letters its region shows with the number, such as the Lv in Lv.5.',
  },
  reference: {
    hint: READS_HINT,
    none: 'Nothing chosen, so it reads every character it learns, and finds and records text of any characters.',
    some: 'Turn on every character it is for. It finds, records, and reads only these.',
    letters: 'Letters. Turn these on for letters it reads beside its numbers, such as the Lv in Lv.5.',
  },
} as const;

/**
 * The characters a number value needs, or a reference reads: presets to turn on, and a field for any other character.
 * It edits one set of characters. A value reads only these of its reference, and a reference finds, records, and
 * reads only its own.
 */
export function ExpectedCharacters({
  expected,
  onExpected,
  owner = 'value',
  label = 'Characters it needs',
}: Props & {
  /** Whose characters these are, which decides what the chooser says and whether it holds them to a most. */
  readonly owner?: keyof typeof WORDS;
  /** What the field is called: the value's own name for it, or the reference editor's, where "it" is the reference. */
  readonly label?: string;
}) {
  // The field's text as typed, and the set it started from, while it is being typed in. Outside that, the field
  // shows the set's own other characters, in order.
  const [typing, setTyping] = useState<{ readonly start: readonly string[]; readonly text: string } | null>(null);
  // The reference editor can show a reference's chooser and its value's at once.
  const otherId = useId();
  const words = WORDS[owner];
  return (
    <div className="field">
      <span className="name">
        <span>{label}</span>
        <Hint topic={label}>{words.hint}</Hint>
      </span>
      <span className="note">{expected.length === 0 ? words.none : words.some}</span>
      <Presets title="Numbers" presets={NUMBER_PRESETS} expected={expected} onExpected={onExpected} />
      <span className="note">{words.letters}</span>
      <Presets title="Letters" presets={LETTER_PRESETS} expected={expected} onExpected={onExpected} />
      <label htmlFor={otherId}>Other characters</label>
      <input
        id={otherId}
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
      {owner === 'value' && tooMany(expected) ? (
        <span className="warn" role="alert">
          {TOO_MANY}
        </span>
      ) : null}
    </div>
  );
}
