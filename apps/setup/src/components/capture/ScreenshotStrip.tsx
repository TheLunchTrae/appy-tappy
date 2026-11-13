import type { useScreenshots } from '../../lib/useScreenshots.ts';
import { PictureCanvas } from '../canvas.tsx';

interface Props {
  /** The screenshots listed. */
  readonly shots: ReturnType<typeof useScreenshots>['shots'];
  /** The screenshot shown, or null while the live window shows. */
  readonly selected: number | null;
  /** The screenshot's number as the strip shows it. */
  readonly position: (index: number) => number;
  readonly hidden: boolean;
  /**
   * The way to remove the screenshot shown, and why it cannot be removed now, or null when it can; null on a screen
   * whose screenshots stay.
   */
  readonly removal: { readonly unremovable: string | null; readonly onRemove: (index: number) => void } | null;
  /** How long the screenshots last, and what else the screen says of them. */
  readonly note: string;
  readonly onSelect: (index: number | null) => void;
}

/** The live window and the screenshots, to pick what the stage shows, and the way to remove the one shown. */
export function ScreenshotStrip({ shots, selected, position, hidden, removal, note, onSelect }: Props) {
  const unremovable = removal?.unremovable ?? null;
  return (
    <div className="strip" hidden={hidden}>
      <button
        type="button"
        className="thumb"
        aria-pressed={selected === null}
        onClick={() => {
          onSelect(null);
        }}
      >
        <span className="shot">
          <span className="dot live" />
        </span>
        Live
      </button>
      {shots.map((shot) => (
        <button
          key={shot.index}
          type="button"
          className="thumb"
          aria-pressed={selected === shot.index}
          onClick={() => {
            onSelect(shot.index);
          }}
        >
          <span className="shot">{shot.thumb === null ? null : <PictureCanvas picture={shot.thumb} label="" />}</span>
          <span className="when">
            {shot.recorded ? <span className="dot" role="img" aria-label="Recorded" /> : null}
            {position(shot.index)} · {shot.time}
          </span>
        </button>
      ))}
      {selected === null || removal === null ? null : (
        <button
          type="button"
          className="btn small"
          aria-disabled={unremovable !== null}
          aria-describedby={unremovable === null ? undefined : 'remove-blocked'}
          onClick={() => {
            // A refused removal keeps the focus, so its reason is heard.
            if (unremovable === null) {
              removal.onRemove(selected);
            }
          }}
        >
          Remove screenshot {position(selected)}
        </button>
      )}
      <span className="note">
        {unremovable === null ? null : <span id="remove-blocked">{unremovable} </span>}
        {note}
      </span>
    </div>
  );
}
