// The matching pixels preview: its switch and what it found, in the inspector, and its tint over the screenshot.

import { type ReactNode, useEffect, useMemo, useRef } from 'react';

import {
  type Answered,
  type Tint,
  barLegend,
  inkLines,
  layersOf,
  nearLine,
  paint,
  tintsFor,
} from '../lib/matchPreview.ts';
import { hex } from '../lib/pixels.ts';
import type { Rect, Size } from '../lib/region.ts';
import type { MatchPreview } from '../lib/useMatchPreview.ts';

/** What each target's switch is called, and what its tint means. */
const SWITCHES = {
  ink: {
    name: 'Show matching pixels',
    help: 'Tints every pixel on the screenshot that counts as part of the characters at this color and tolerance.',
  },
  barFill: {
    name: 'Show matching pixels',
    help: 'Tints the pixels in the region that count as filled and as empty.',
  },
  color: {
    name: 'Show pixels within the tolerance',
    help: "Tints the pixels in the region that are within the tolerance on their own. The value itself compares the region's average color, so the tint is a guide, not the result.",
  },
} as const;

/** A tint as its legend shows it. */
function Key({ tint }: { readonly tint: Tint | null }) {
  if (tint === null) {
    return <span className="swatch match-key clear" />;
  }
  return (
    <span
      className={tint.striped ? 'swatch match-key striped' : 'swatch match-key'}
      style={{ color: hex(tint.color) }}
    />
  );
}

/** What the answer found, in words, so the tint is not the only sign. */
function Found({ tint, region }: { readonly tint: Answered; readonly region: Rect | null }): ReactNode {
  const { request, view } = tint;
  const tints = tintsFor(request.rule);
  switch (view.kind) {
    case 'ink':
      return (
        <p className="note">
          <Key tint={tints[0] ?? null} />
          {inkLines(view.ink, request.area, region).join(' ')}
        </p>
      );
    case 'barFill': {
      const legend = barLegend(view.filled, view.empty, request.area, tints);
      return (
        <>
          <ul className="match-legend">
            {legend.entries.map((entry) => (
              <li key={entry.name}>
                <Key tint={entry.tint} />
                {entry.name}: {entry.count}
              </li>
            ))}
          </ul>
          {legend.note === null ? null : <p className="note">{legend.note}</p>}
        </>
      );
    }
    case 'color': {
      const tolerance = request.rule.kind === 'color' ? ` (tolerance ${request.rule.tolerance})` : '';
      return (
        <>
          <p className="note">
            <Key tint={tints[0] ?? null} />
            {nearLine(view.near, request.area)}
          </p>
          <dl className="match-facts">
            <dt>Region&apos;s average color</dt>
            <dd>
              <span className="swatch" style={{ background: hex(view.average) }} />
              {hex(view.average)}
            </dd>
            <dt>Distance from the color to look for</dt>
            <dd>
              {view.distance}
              {tolerance}
            </dd>
            <dt>Value on this screenshot</dt>
            <dd>
              <strong>{view.matches ? 'Yes' : 'No'}</strong>
            </dd>
          </dl>
          <p className="note">
            The distance is the largest difference in red, green, or blue between the two colors. The value is yes while
            it is at most the tolerance.
          </p>
        </>
      );
    }
  }
}

/** The preview's switch, and under it what the tint shows or why there is none. */
export function MatchPreviewField({
  preview,
  region,
}: {
  readonly preview: MatchPreview;
  /** The draft's region, which a reference's count is for. */
  readonly region: Rect | null;
}) {
  const { target, on, plan, tint, error } = preview;
  if (target === null) {
    return null;
  }
  const { name, help } = SWITCHES[target];
  return (
    <div className="field">
      <label className="toggle-row">
        <input
          type="checkbox"
          role="switch"
          className="switch"
          aria-checked={on}
          aria-describedby="match-preview-help"
          checked={on}
          onChange={(event) => {
            preview.setOn(event.target.checked);
          }}
        />
        {name}
      </label>
      <span id="match-preview-help" className="note">
        {help}
      </span>
      {plan === null ? null : plan.kind === 'blocked' ? (
        <p className="note" role="status">
          {plan.why}
        </p>
      ) : error !== null ? (
        <p className="warn">{error}</p>
      ) : tint === null ? (
        <p className="note">Looking for matching pixels…</p>
      ) : (
        <div className="match-found" aria-live="polite">
          <Found tint={tint} region={region} />
        </div>
      )}
    </div>
  );
}

/**
 * The tint, drawn at the screenshot's own pixels inside the frame that holds the screenshot, so it stays on its
 * pixels at any size the frame shows at. It takes no pointer events.
 */
export function MatchOverlay({
  tint,
  size,
}: {
  readonly tint: Answered | null;
  /** The screenshot's size in its own pixels. */
  readonly size: Size;
}) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const area = tint?.request.area ?? null;
  const pixels = useMemo(
    () =>
      tint === null
        ? null
        : paint(layersOf(tint.view), tintsFor(tint.request.rule), tint.request.area.width, tint.request.area.height),
    [tint],
  );
  useEffect(() => {
    const context = canvas.current?.getContext('2d');
    if (context === null || context === undefined || pixels === null || area === null) {
      return;
    }
    context.putImageData(new ImageData(pixels, area.width, area.height), 0, 0);
  }, [pixels, area]);
  if (area === null || size.width === 0 || size.height === 0) {
    return null;
  }
  const percent = (part: number, whole: number): string => `${(part / whole) * 100}%`;
  return (
    <div
      className="match-box"
      style={{
        left: percent(area.x, size.width),
        top: percent(area.y, size.height),
        width: percent(area.width, size.width),
        height: percent(area.height, size.height),
      }}
    >
      <canvas ref={canvas} width={area.width} height={area.height} aria-hidden="true" />
    </div>
  );
}
