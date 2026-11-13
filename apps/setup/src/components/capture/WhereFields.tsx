import type { Dispatch, SetStateAction } from 'react';

import type { SizeDef, ValueDef } from '../../lib/bindings.ts';
import { type Draft, CHECK_KINDS, KIND_NAMES, withKind } from '../../lib/check.ts';
import { CHECK_HINT } from '../../lib/hints.ts';
import type { MovingContent } from '../../lib/moving.ts';
import type { PinView } from '../../lib/pin.ts';
import type { SelectTool } from '../../lib/selection.ts';
import type { useSelectedPreview } from '../../lib/useMatchPreview.ts';
import { SelectFields } from '../CheckFields.tsx';
import { RegionFields } from '../Fields.tsx';
import { Hint } from '../Hint.tsx';
import { SelectedPixelsField } from '../MatchPreview.tsx';
import { MovingContentField } from '../MovingContentField.tsx';
import { PinPicker } from '../PinPicker.tsx';
import { EditIntro } from '../ValueEdit.tsx';

interface Props {
  /** The saved value an edit changes, or null for a new value. */
  readonly edited: ValueDef | null;
  /** The size of the screenshot shown, when an edited value was drawn at another. */
  readonly otherSize: SizeDef | null;
  readonly valueName: string;
  readonly draft: Draft;
  readonly tool: SelectTool;
  /** Whether a screenshot shows, which the selection tools select on. */
  readonly canSelect: boolean;
  readonly selectedPreview: ReturnType<typeof useSelectedPreview>;
  /** The screenshot the region shows on, which a region typed before any was drawn goes on. */
  readonly shownOn: number | null;
  readonly moving: MovingContent;
  readonly appName: string;
  /** What the region stays at. */
  readonly staysAt: PinView;
  /** Whether the application's values scale with its window. */
  readonly scales: boolean;
  readonly onValueName: (name: string) => void;
  readonly onDraft: Dispatch<SetStateAction<Draft>>;
  readonly onTool: (tool: SelectTool) => void;
  readonly onMayMove: (mayMove: boolean) => void;
  /** Ends picking a color, which a change of check or tool leaves behind. */
  readonly onStopPicking: () => void;
}

/** The first step's fields for a new or an edited value: its name, its check, and where it is read. */
export function WhereFields({
  edited,
  otherSize,
  valueName,
  draft,
  tool,
  canSelect,
  selectedPreview,
  shownOn,
  moving,
  appName,
  staysAt,
  scales,
  onValueName,
  onDraft,
  onTool,
  onMayMove,
  onStopPicking,
}: Props) {
  return (
    <>
      {edited === null ? null : <EditIntro value={edited} shown={otherSize} />}
      <div className="field">
        <label htmlFor="value-name">Value name</label>
        <input
          id="value-name"
          className="input"
          placeholder="Progress"
          value={valueName}
          onChange={(event) => {
            onValueName(event.target.value);
          }}
        />
      </div>
      <div className="field">
        <span className="name">
          <span id="check-kind">Check</span>
          <Hint topic="Check">{CHECK_HINT}</Hint>
        </span>
        <div className="seg" role="group" aria-labelledby="check-kind">
          {CHECK_KINDS.map((kind) => (
            <button
              key={kind}
              type="button"
              aria-pressed={draft.kind === kind}
              onClick={() => {
                onDraft((current) => withKind(current, kind));
                onStopPicking();
              }}
            >
              {KIND_NAMES[kind]}
            </button>
          ))}
        </div>
      </div>
      {draft.kind === 'state' ? (
        <SelectFields
          tool={tool}
          colorMatch={draft.colorMatch}
          canSelect={canSelect}
          preview={<SelectedPixelsField preview={selectedPreview} />}
          onTool={(next) => {
            onTool(next);
            onStopPicking();
          }}
          onColorMatch={(value) => {
            onDraft((current) => ({ ...current, colorMatch: value }));
          }}
        />
      ) : null}
      <RegionFields
        region={draft.region}
        locked={edited !== null && draft.drawnOn === null}
        onRegion={(region) => {
          // Typed pixels keep to the screenshot the region was drawn on.
          onDraft((current) => ({ ...current, region, drawnOn: current.drawnOn ?? shownOn }));
        }}
      />
      <MovingContentField moving={moving} appName={appName} onMayMove={onMayMove} />
      <PinPicker
        view={staysAt}
        scales={scales}
        onChoice={(pin) => {
          onDraft((current) => ({ ...current, pin }));
        }}
      />
    </>
  );
}
