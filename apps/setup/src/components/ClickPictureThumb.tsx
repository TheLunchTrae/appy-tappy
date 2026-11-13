import { useEffect, useState } from 'react';

import { clickPicture } from '../lib/api.ts';
import type { Picture } from '../lib/pixels.ts';
import { background } from '../lib/repeat.ts';
import { PictureCanvas } from './canvas.tsx';

/** The widest a click picture's thumbnail is asked for. */
const THUMB_WIDTH = 220;

/** A click picture shown small, such as beside a click in the automation editor. */
export function ClickPictureThumb({ image, label }: { readonly image: string; readonly label: string }) {
  const [picture, setPicture] = useState<Picture | null>(null);
  useEffect(() => {
    let current = true;
    background(
      clickPicture(image, THUMB_WIDTH)
        .then((loaded) => {
          if (current) {
            setPicture(loaded);
          }
        })
        .catch(() => {
          // The summary beside it still says what the click does.
        }),
    );
    return () => {
      current = false;
    };
  }, [image]);
  return (
    <span className="click-thumb">{picture === null ? null : <PictureCanvas picture={picture} label={label} />}</span>
  );
}
