import { UNLEARNED_HINT } from '../lib/hints.ts';
import type { Unlearned } from '../lib/useUnlearned.ts';
import { PictureCanvas } from './canvas.tsx';
import { Hint } from './Hint.tsx';

interface Props {
  /** The saved pictures the reference can't learn as it finds its characters now. */
  readonly unlearned: Unlearned;
  /** Whether the reference has a text color, which the user can change so a picture is learned again. */
  readonly hasTextColor: boolean;
  /** How many of its saved pictures the editor has left out. */
  readonly removed: number;
  /** Leaves the saved picture with this file name out. */
  readonly onRemove: (image: string) => void;
  /** Takes every saved picture left out back in. */
  readonly onPutBack: () => void;
}

/**
 * The saved pictures a reference can't learn: each one's pixels, which no box or screenshot shows, what it was typed
 * as, why it teaches nothing now, and a way to leave it out. It shows nothing while every saved picture is learned
 * and none is left out.
 */
export function UnlearnedPictures({ unlearned, hasTextColor, removed, onRemove, onPutBack }: Props) {
  const { samples, pictures } = unlearned;
  // A missing picture teaches nothing whatever the text color. One cut for a text color learns again once it is picked.
  const colorHelps = samples.some((sample) => !sample.missing);
  const colorAdvice = hasTextColor
    ? ', or change the text color or its tolerance until the list is empty'
    : ', or pick a text color until the list is empty';
  if (samples.length === 0 && removed === 0) {
    return null;
  }
  return (
    <div className="field">
      <span className="name">
        <span id="unlearned-title">Saved pictures it can't learn</span>
        <Hint topic="Saved pictures it can't learn">{UNLEARNED_HINT}</Hint>
      </span>
      {samples.length === 0 ? null : (
        <>
          <p className="warn">
            It can't be saved while it keeps {samples.length === 1 ? 'this picture' : 'these pictures'}. Remove{' '}
            {samples.length === 1 ? 'it' : 'them'}
            {colorHelps ? colorAdvice : ''}.
          </p>
          <ul className="box-list unlearned" aria-labelledby="unlearned-title">
            {samples.map((sample) => {
              const picture = pictures.get(sample.image);
              return (
                <li key={sample.image}>
                  <span className="crop">
                    {picture === undefined ? null : (
                      <PictureCanvas picture={picture} label={`The saved picture of ${sample.text}`} />
                    )}
                  </span>
                  <span>{sample.text}</span>
                  <button
                    type="button"
                    className="btn small"
                    aria-label={`Remove the saved picture of ${sample.text}`}
                    onClick={() => {
                      onRemove(sample.image);
                    }}
                  >
                    Remove
                  </button>
                  <span className="where">{sample.problem}</span>
                </li>
              );
            })}
          </ul>
        </>
      )}
      {removed === 0 ? null : (
        <p className="note">
          {removed === 1 ? '1 saved picture' : `${removed} saved pictures`} removed. Saving the reference deletes{' '}
          {removed === 1 ? 'it' : 'them'}.{' '}
          <button type="button" className="btn small" onClick={onPutBack}>
            Put back
          </button>
        </p>
      )}
    </div>
  );
}
