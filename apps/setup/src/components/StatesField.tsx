// A state check's looks: each one's name, the screenshot it is taken from, and how much of it is compared.

import { useMemo } from 'react';

import type { Reading } from '../lib/api.ts';
import {
  type Draft,
  type DraftState,
  MAX_STATES,
  stateOn,
  withoutState,
  withState,
  withStateAt,
} from '../lib/check.ts';
import { type Picture, cropPicture } from '../lib/pixels.ts';
import type { Rect } from '../lib/region.ts';
import { PictureCanvas } from './canvas.tsx';
import { ToleranceField } from './Fields.tsx';

/** What a state's selection leaves of the region. */
export interface Masking {
  /** Whether its last steps selected anything. */
  readonly found: boolean;
  /** How many of the region's pixels are compared, or null until its selection is made. */
  readonly kept: number | null;
}

interface Props {
  readonly draft: Draft;
  /** The screenshots taken this session, at their own pixels. */
  readonly full: ReadonlyMap<number, Picture>;
  readonly selected: number | null;
  /** What each state's selection leaves of the region, by the screenshot it is taken from. */
  readonly masks: ReadonlyMap<number, Masking>;
  readonly reading: Reading | null;
  /** The screenshot's number as the strip shows it. */
  readonly position: (index: number) => number;
  readonly onDraft: (update: (draft: Draft) => Draft) => void;
  /** Shows the screenshot a state is taken from. */
  readonly onShow: (screenshot: number) => void;
}

/** Why a new state cannot be taken from the screenshot shown, or null when it can. */
function addBlocked(draft: Draft, selected: number | null): string | null {
  if (selected === null) {
    return 'Select a screenshot that shows the state first.';
  }
  if (stateOn(draft, selected) !== -1) {
    return 'A state is already taken from this screenshot.';
  }
  return draft.states.length >= MAX_STATES ? `A value can tell apart at most ${MAX_STATES} states.` : null;
}

interface CardProps {
  readonly state: DraftState;
  readonly region: Rect | null;
  readonly picture: Picture | null;
  readonly masking: Masking | null;
  readonly ignoreBackground: boolean;
  /** Whether its screenshot is the one shown. */
  readonly shown: boolean;
  /** Whether the live window reads as this state now. */
  readonly matches: boolean;
  readonly number: number;
  readonly onName: (name: string) => void;
  readonly onClear: () => void;
  readonly onRemove: () => void;
  readonly onShow: () => void;
}

/** How many of the region's pixels a state compares, in words, or null while that is not known. */
function comparedText(state: DraftState, region: Rect | null, masking: Masking | null, ignore: boolean): string | null {
  if (region === null) {
    return null;
  }
  const selects = ignore && state.steps.length > 0;
  const kept = selects ? (masking?.kept ?? null) : region.width * region.height;
  return kept === null ? null : `${kept.toLocaleString()} ${kept === 1 ? 'pixel' : 'pixels'} compared`;
}

function StateCard(props: CardProps) {
  const { state, region, picture, masking, ignoreBackground, shown, matches, number } = props;
  const crop = useMemo(
    () => (picture === null || region === null ? null : cropPicture(picture, region)),
    [picture, region],
  );
  const compared = comparedText(state, region, masking, ignoreBackground);
  const lost = state.steps.length > 0 && masking?.found === false;
  const label = state.name.trim() === '' ? `State from screenshot ${number}` : state.name.trim();
  return (
    <li className="state" data-matches={matches} aria-current={shown ? 'true' : undefined}>
      <button
        type="button"
        className="state-thumb"
        title={`Show screenshot ${number}`}
        aria-label={`Show screenshot ${number}, which ${label} is taken from`}
        onClick={props.onShow}
      >
        {crop === null ? <span className="note">No region</span> : <PictureCanvas picture={crop} label={label} />}
      </button>
      <div className="state-body">
        <input
          className="input"
          aria-label={`Name of the state from screenshot ${number}`}
          placeholder="Name"
          value={state.name}
          onChange={(event) => {
            props.onName(event.target.value);
          }}
        />
        <small>
          Screenshot {number}
          {compared === null ? null : ` · ${compared}`}
          {matches ? (
            <>
              {' · '}
              <span className="ok">matches now</span>
            </>
          ) : null}
        </small>
        {lost ? <small className="warn">Nothing stood out there. Try again, or raise Color match.</small> : null}
      </div>
      <div className="state-actions">
        <button type="button" className="btn small" onClick={props.onRemove}>
          Remove
        </button>
        {state.steps.length === 0 ? null : (
          <button type="button" className="btn small" onClick={props.onClear}>
            Clear selection
          </button>
        )}
      </div>
    </li>
  );
}

export function StatesField({ draft, full, selected, masks, reading, position, onDraft, onShow }: Props) {
  const blocked = addBlocked(draft, selected);
  const selecting = draft.states.some((state) => state.steps.length > 0);
  return (
    <>
      <div className="field">
        <span id="states-title">States</span>
        <span className="note">Reads as the state the region matches best, or none.</span>
        {draft.states.length === 0 ? (
          <p className="note">
            No states yet. Select a screenshot that shows one, and add it, or select the thing on it.
          </p>
        ) : (
          <ul className="states" aria-labelledby="states-title">
            {draft.states.map((state, index) => (
              <StateCard
                key={state.screenshot}
                state={state}
                region={draft.region}
                picture={full.get(state.screenshot) ?? null}
                masking={masks.get(state.screenshot) ?? null}
                ignoreBackground={draft.ignoreBackground}
                shown={state.screenshot === selected}
                matches={reading?.kind === 'state' && reading.value === state.name.trim()}
                number={position(state.screenshot)}
                onName={(name) => {
                  onDraft((current) => withStateAt(current, index, (old) => ({ ...old, name })));
                }}
                onClear={() => {
                  onDraft((current) => withStateAt(current, index, (old) => ({ ...old, steps: [] })));
                }}
                onRemove={() => {
                  onDraft((current) => withoutState(current, index));
                }}
                onShow={() => {
                  onShow(state.screenshot);
                }}
              />
            ))}
          </ul>
        )}
        <div className="choices">
          <button
            type="button"
            className="btn small"
            disabled={blocked !== null}
            title={blocked ?? undefined}
            onClick={() => {
              if (selected !== null) {
                onDraft((current) => withState(current, selected));
              }
            }}
          >
            Add a state from this screenshot
          </button>
        </div>
      </div>
      <div className="field">
        <label className="toggle-row">
          <input
            type="checkbox"
            role="switch"
            className="switch"
            aria-checked={draft.ignoreBackground}
            checked={draft.ignoreBackground}
            onChange={(event) => {
              const ignoreBackground = event.target.checked;
              onDraft((current) => ({ ...current, ignoreBackground }));
            }}
          />
          Ignore the background
        </label>
        <span className="note">
          {selecting
            ? 'Shaded pixels are skipped, so a state still matches when what is behind it changes.'
            : 'Select the thing on a state’s screenshot, back where the region is drawn, to compare only its pixels.'}
        </span>
      </div>
      <ToleranceField
        help="How far, on average, the compared pixels may differ from a state's image."
        value={draft.tolerance}
        onChange={(value) => {
          onDraft((current) => ({ ...current, tolerance: value }));
        }}
      />
    </>
  );
}
