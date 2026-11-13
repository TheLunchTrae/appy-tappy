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
  /** Why the screenshot shown cannot be removed, or null when it can. */
  readonly unremovable: string | null;
  /** Whether the strip holds the one screenshot the value's place is set on. */
  readonly onePicture: boolean;
  readonly onSelect: (index: number | null) => void;
  readonly onRemove: (index: number) => void;
}

/** The live window and the screenshots, to pick what the stage shows, and the way to remove the one shown. */
export function ScreenshotStrip({
  shots,
  selected,
  position,
  hidden,
  unremovable,
  onePicture,
  onSelect,
  onRemove,
}: Props) {
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
      {selected === null ? null : (
        <button
          type="button"
          className="btn small"
          aria-disabled={unremovable !== null}
          aria-describedby={unremovable === null ? undefined : 'remove-blocked'}
          onClick={() => {
            // A refused removal keeps the focus, so its reason is heard.
            if (unremovable === null) {
              onRemove(selected);
            }
          }}
        >
          Remove screenshot {position(selected)}
        </button>
      )}
      <span className="note">
        {unremovable === null ? null : <span id="remove-blocked">{unremovable} </span>}
        Screenshots last until you remove them or change the window.
        {onePicture ? ' The value needs one screenshot; take another to draw on a newer one.' : ''}
      </span>
    </div>
  );
}
