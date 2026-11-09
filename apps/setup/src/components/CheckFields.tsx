// The capture screen's fields for selecting the thing in a region, and for the checks that read with no reference.

import { LIMITS } from '../lib/bindings.ts';
import type { Draft } from '../lib/check.ts';
import { type SelectTool, SELECT_TOOLS, TOOL_NAMES } from '../lib/selection.ts';
import { BAR_COLORS_HINT } from '../lib/hints.ts';
import { ColorField, ToleranceField } from './Fields.tsx';

/** Where a picked color goes. */
export type ColorTarget = 'ink' | 'filled' | 'empty' | 'color';

/** How each selection tool is used, shown under the tools. */
const TOOL_HELP: Readonly<Record<SelectTool, string | null>> = {
  box: null,
  snap: 'Drag a rough box around the thing. It shrinks to fit, and the plain background around and between its parts is left out. Shift+drag adds a part; Alt+drag removes one.',
  click:
    'Click the thing. The selection spreads over similar colors and fills any holes. Shift+click adds a part; Alt+click removes one.',
};

/** The selection tools, and how alike colors must be to count as the same while selecting. */
export function SelectFields({
  tool,
  colorMatch,
  canSelect,
  onTool,
  onColorMatch,
}: {
  readonly tool: SelectTool;
  readonly colorMatch: number;
  readonly canSelect: boolean;
  readonly onTool: (tool: SelectTool) => void;
  readonly onColorMatch: (value: number) => void;
}) {
  const help = TOOL_HELP[tool];
  const set = (value: number): void => {
    if (Number.isInteger(value) && value >= 0 && value <= LIMITS.maxTolerance) {
      onColorMatch(value);
    }
  };
  return (
    <>
      <div className="field">
        <span id="select-tool">Select with</span>
        <div className="seg" role="group" aria-labelledby="select-tool">
          {SELECT_TOOLS.map((option) => (
            <button
              key={option}
              type="button"
              aria-pressed={tool === option}
              onClick={() => {
                onTool(option);
              }}
            >
              {TOOL_NAMES[option].name}
              <small>{TOOL_NAMES[option].hint}</small>
            </button>
          ))}
        </div>
        {help === null ? null : <span className="note">{canSelect ? help : `Select a screenshot first. ${help}`}</span>}
      </div>
      {tool === 'box' ? null : (
        <div className="field">
          <label htmlFor="color-match">Color match</label>
          <div className="range">
            <input
              id="color-match"
              type="range"
              min={0}
              max={LIMITS.maxTolerance}
              value={colorMatch}
              onChange={(event) => {
                set(Number(event.target.value));
              }}
            />
            <input
              className="input"
              type="number"
              min={0}
              max={LIMITS.maxTolerance}
              aria-label="Color match"
              value={colorMatch}
              onChange={(event) => {
                set(Number.parseInt(event.target.value, 10));
              }}
            />
          </div>
          <span className="note">
            How close a color must be to count as the same. Raise it if the selection stops short; lower it if it spills
            over.
          </span>
        </div>
      )}
    </>
  );
}

interface ColorProps {
  readonly draft: Draft;
  readonly picking: ColorTarget | null;
  readonly canPick: boolean;
  readonly onDraft: (update: (draft: Draft) => Draft) => void;
  readonly onPicking: (target: ColorTarget | null) => void;
}

/** The fields of a bar fill or a color check, whose colors are picked from a screenshot. */
export function ColorFields({ draft, picking, canPick, onDraft, onPicking }: ColorProps) {
  const color = (target: ColorTarget, label: string, hint?: string) => (
    <ColorField
      key={target}
      label={label}
      hint={hint}
      color={draft[target]}
      picking={picking === target}
      canPick={canPick}
      onPick={() => {
        onPicking(picking === target ? null : target);
      }}
    />
  );
  if (draft.kind === 'barFill') {
    return (
      <>
        {color('filled', 'Filled color', BAR_COLORS_HINT)}
        {color('empty', 'Empty color')}
      </>
    );
  }
  return (
    <>
      {color('color', 'Color to look for')}
      <ToleranceField
        help="How far the region's average color may be."
        value={draft.tolerance}
        onChange={(value) => {
          onDraft((current) => ({ ...current, tolerance: value }));
        }}
      />
    </>
  );
}
