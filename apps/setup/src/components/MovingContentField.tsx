// Whether the value's content may move around inside the window, and what that means for the check being set up:
// where to draw, and what a larger reading area can and cannot do.

import { MOVING_HINT, type MovingContent } from '../lib/moving.ts';
import { Hint } from './Hint.tsx';

interface Props {
  /** What the option comes to for the check's kind. */
  readonly moving: MovingContent;
  /** The application whose screenshots the region is drawn on. */
  readonly appName: string;
  readonly onMayMove: (mayMove: boolean) => void;
}

export function MovingContentField({ moving, appName, onMayMove }: Props) {
  return (
    <div className="field">
      <span className="name">
        <label className="toggle">
          <input
            type="checkbox"
            className="switch"
            aria-describedby="hint-may-move"
            checked={moving.kind !== 'fixed'}
            onChange={(event) => {
              onMayMove(event.target.checked);
            }}
          />
          This content may move
        </label>
        <Hint topic="This content may move" id="hint-may-move">
          {MOVING_HINT}
        </Hint>
      </span>
      {/* Always there, so a screen reader hears what the option, or another check, changes. */}
      <div className="moving-says" role="status">
        {moving.kind === 'fixed' ? (
          <p className="note">
            Draw the region where the value shows, on any screenshot of {appName}. Only where it is matters here; how to
            read it comes next.
          </p>
        ) : null}
        {moving.kind === 'area' ? (
          <>
            <p className="note">{moving.guidance}</p>
            <p className="note">{moving.limits}</p>
          </>
        ) : null}
        {moving.kind === 'unsupported' ? <p className="warn">{moving.why}</p> : null}
      </div>
    </div>
  );
}
