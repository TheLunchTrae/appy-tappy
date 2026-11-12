import { useState } from 'react';

import type { ReadingDetails, WorkingsView } from '../lib/bindings.ts';
import { durationText } from '../lib/debug.ts';
import { useRepeat } from '../lib/repeat.ts';
import { useSettings } from '../lib/settings.ts';
import { averageDifference, channelsText, closeness, cssColor, percentOf, readState } from '../lib/workings.ts';

/** How long the details wait between reads. A read scores every glyph, so it runs less often than the reading. */
const DETAILS_PAUSE_MS = 500;

/** Each character's closest glyph, how close it is, and the closest glyph of another character. */
function Characters({ workings }: { readonly workings: Extract<WorkingsView, { kind: 'number' }> }) {
  if (workings.characters.length === 0) {
    return <p className="note">No characters in the region.</p>;
  }
  return (
    <>
      <ul className="glyph-scores" aria-label="Characters read">
        {workings.characters.map((character, index) => {
          const { best, runnerUp } = character;
          return (
            // The characters only ever change as a whole, so their place is their key.
            <li key={index} className={best?.matches === true ? 'glyph-score' : 'glyph-score off'}>
              <span className="glyph-char">{best === null ? '?' : best.character}</span>
              <span className="glyph-close">{best === null ? 'no glyphs' : `${closeness(best)}%`}</span>
              <span className="note">
                {runnerUp === null ? 'no other' : `next: ${runnerUp.character} ${closeness(runnerUp)}%`}
              </span>
            </li>
          );
        })}
      </ul>
      <p className="note">
        Each character read, how closely it matches the reference, and the closest other character. A character reads at
        75% or closer. A close second means the reference needs more samples of both.
      </p>
    </>
  );
}

/** The check's scores, in the form its kind reads with. */
function Workings({ workings }: { readonly workings: WorkingsView }) {
  switch (workings.kind) {
    case 'number':
      return <Characters workings={workings} />;
    case 'barFill': {
      const share = percentOf(workings.filled, workings.counted);
      return (
        <p className="note">
          {share === null
            ? 'No pixel is near either color.'
            : `${workings.filled} of ${workings.counted} pixels near either color are nearer the filled one: ${share}%.`}
        </p>
      );
    }
    case 'color':
      return (
        <p className="note color-scores">
          <span className="swatch" style={{ background: cssColor(workings.average) }} aria-hidden="true" />
          Average {channelsText(workings.average)}
          <span className="swatch" style={{ background: cssColor(workings.color) }} aria-hidden="true" />
          looking for {channelsText(workings.color)}, each within {workings.tolerance}
        </p>
      );
    case 'state': {
      const read = readState(workings.states);
      return (
        <table className="table state-scores">
          <caption className="note">How far the region is from each state, on average per color channel</caption>
          <thead>
            <tr>
              <th scope="col">State</th>
              <th scope="col">Difference</th>
              <th scope="col">Tolerance</th>
            </tr>
          </thead>
          <tbody>
            {workings.states.map((state) => (
              <tr key={state.name} className={state.name === read ? 'read' : undefined}>
                <td>
                  {state.name}
                  {state.name === read ? <span className="note"> (read)</span> : null}
                </td>
                <td>{averageDifference(state) ?? '–'}</td>
                <td>{state.tolerance}</td>
              </tr>
            ))}
          </tbody>
        </table>
      );
    }
  }
}

/**
 * How a live reading came about, shown under it while debug details show: how long the check took, how old the frame
 * was, and the check's scores. `ask` reads them again, or is null while there is nothing to read.
 */
export function ReadingDetailsBox({ ask }: { readonly ask: (() => Promise<ReadingDetails | null>) | null }) {
  const { showDebug } = useSettings().settings;
  const [details, setDetails] = useState<ReadingDetails | null>(null);
  const active = showDebug && ask !== null;
  useRepeat(
    async () => {
      if (ask === null) {
        return;
      }
      try {
        setDetails(await ask());
      } catch {
        setDetails(null);
      }
    },
    DETAILS_PAUSE_MS,
    active,
  );
  if (!active || details === null) {
    return null;
  }
  return (
    <section className="reading-details" aria-label="Debug details">
      <p className="debug-line">
        <span>
          Check <strong>{durationText(details.tookUs * 1000)}</strong>
        </span>
        <span>
          Frame <strong>{details.frameAgeMs} ms</strong> old
        </span>
      </p>
      {details.problem === null ? null : <p className="note">{details.problem}</p>}
      {details.workings === null ? null : <Workings workings={details.workings} />}
    </section>
  );
}
