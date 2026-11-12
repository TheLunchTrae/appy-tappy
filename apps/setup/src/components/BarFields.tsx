// The bar fill check's own fields: finding the bar's colors, and showing what two colors measure.

import { useEffect, useRef } from 'react';

import type { BarMarks } from '../lib/bindings.ts';
import { type PickedColors, detectingText, MARK_TONES, paintMarks, readingLine, toneCss } from '../lib/detect.ts';
import { hex } from '../lib/pixels.ts';
import type { Rect } from '../lib/region.ts';
import type { BarSetup } from '../lib/useBarSetup.ts';
import { Question, useQuestion } from './Question.tsx';

interface Props {
  readonly bar: BarSetup;
  /** A screenshot's number in the strip. */
  readonly position: (index: number) => number;
}

/** Detects the bar's colors, and says how they were found or why none were. */
export function BarDetect({ bar, picked, position }: Props & { readonly picked: PickedColors }) {
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
          title={bar.canDetect ? undefined : 'Draw the region on a screenshot first'}
          onClick={bar.detect}
        >
          Detect the bar
        </button>
      )}
      {/* A paragraph, since a field colors its own spans as labels. */}
      <p className={detecting.kind === 'declined' ? 'warn' : 'note'} role="status">
        {detectingText(detecting, picked, position)}
      </p>
    </div>
  );
}

/** What the bar's two colors read on each screenshot, and whether the pixels they count are marked. */
export function BarReadings({ bar, position }: Props) {
  if (bar.colors === null) {
    return null;
  }
  const asking = bar.detecting.kind === 'found' && bar.detecting.asking;
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
      <label className="toggle">
        <input
          type="checkbox"
          checked={bar.marking}
          onChange={(event) => {
            bar.setMarking(event.target.checked);
          }}
        />
        Mark what counts on the screenshot
      </label>
      {bar.unmarked === null ? (
        <span className="note">
          <span className="swatch" style={{ background: toneCss(MARK_TONES.filled) }} />
          counts as filled, <span className="swatch" style={{ background: toneCss(MARK_TONES.empty) }} />
          as empty. Pixels with no mark, such as a border or a label, are left out.
        </span>
      ) : (
        <span className="note">{bar.unmarked}</span>
      )}
      {asking ? null : (
        <button type="button" className="btn small swap" onClick={bar.swap}>
          Swap colors
        </button>
      )}
    </div>
  );
}

/** The pixels a bar's colors count in `region`, marked over the screenshot, which shows at `scale`. */
export function BarMarksOverlay({
  marks,
  region,
  scale,
}: {
  readonly marks: BarMarks;
  readonly region: Rect;
  readonly scale: number;
}) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const { width, height } = region;
  useEffect(() => {
    const context = canvas.current?.getContext('2d');
    if (context !== null && context !== undefined && width > 0 && height > 0) {
      context.putImageData(new ImageData(paintMarks(marks, width * height), width, height), 0, 0);
    }
  }, [marks, width, height]);
  const place = { left: region.x * scale, top: region.y * scale, width: width * scale, height: height * scale };
  return (
    <div className="bar-marks" style={place}>
      <canvas ref={canvas} width={width} height={height} aria-hidden="true" />
    </div>
  );
}
