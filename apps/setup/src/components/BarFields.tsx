// The bar fill check's own fields: finding the bar's colors, and showing what two colors measure.

import { type PickedColors, detectingText, readingLine } from '../lib/detect.ts';
import { hex } from '../lib/pixels.ts';
import type { BarSetup } from '../lib/useBarSetup.ts';
import { Question, useQuestion } from './Question.tsx';

interface Props {
  readonly bar: BarSetup;
  /** A screenshot's number in the strip. */
  readonly position: (index: number) => number;
}

/**
 * Detects the bar's colors, and says how they were found or why none were. `blocked` says what an edit needs before
 * there is anything to look at, or is null.
 */
export function BarDetect({
  bar,
  picked,
  position,
  blocked,
}: Props & { readonly picked: PickedColors; readonly blocked: string | null }) {
  const { detecting } = bar;
  const asking = detecting.kind === 'found' && detecting.asking;
  const question = useQuestion<HTMLButtonElement>(asking);
  return (
    <div className="field">
      {detecting.kind === 'found' && detecting.asking ? (
        <Question
          refs={question.refs}
          label="Use the detected colors?"
          keepLabel="Keep my colors"
          actLabel="Use detected colors"
          onKeep={bar.keep}
          onAct={bar.use}
        >
          Detected <span className="swatch" style={{ background: hex(detecting.proposal.filled) }} />
          {hex(detecting.proposal.filled)} as filled and{' '}
          <span className="swatch" style={{ background: hex(detecting.proposal.empty) }} />
          {hex(detecting.proposal.empty)} as empty. Use them in place of the colors you picked?
        </Question>
      ) : (
        <button
          ref={question.control}
          type="button"
          className="btn primary detect"
          disabled={!bar.canDetect || detecting.kind === 'detecting'}
          title={bar.canDetect || blocked !== null ? undefined : 'Draw the region on a screenshot first'}
          onClick={bar.detect}
        >
          Detect the bar
        </button>
      )}
      {/* A paragraph, since a field colors its own spans as labels. */}
      <p className={detecting.kind === 'declined' ? 'warn' : 'note'} role="status">
        {!bar.canDetect && blocked !== null ? blocked : detectingText(detecting, picked, position)}
      </p>
    </div>
  );
}

/** What the bar's two colors read on each screenshot the detection looks at, and a way to swap them. */
export function BarReadings({ bar, position }: Props) {
  if (bar.colors === null) {
    return null;
  }
  const asking = bar.detecting.kind === 'found' && bar.detecting.asking;
  const swap = asking ? null : (
    <button type="button" className="btn small swap" onClick={bar.swap}>
      Swap colors
    </button>
  );
  // No screenshot holds the region yet, so there is nothing to read.
  if (!bar.canDetect) {
    return swap === null ? null : <div className="field">{swap}</div>;
  }
  const title = <span>{asking ? 'The detected colors read' : 'These colors read'}</span>;
  if (bar.measureError !== null) {
    return (
      <div className="field">
        {title}
        <p className="warn">{bar.measureError}</p>
      </div>
    );
  }
  return (
    <div className="field">
      {title}
      {bar.readings === null ? (
        <span className="note">Reading the screenshots…</span>
      ) : (
        <ul className="bar-readings">
          {bar.readings.map((reading) => (
            <li key={reading.screenshot}>{readingLine(reading, position)}</li>
          ))}
        </ul>
      )}
      <span className="note">
        Compare each with how full the bar looks on that screenshot.{bar.leftOut === null ? '' : ` ${bar.leftOut}`}
      </span>
      {swap}
    </div>
  );
}
