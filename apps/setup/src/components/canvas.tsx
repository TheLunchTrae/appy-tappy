import { useCallback, useEffect, useRef } from 'react';

import { preview } from '../lib/api.ts';
import type { Picture } from '../lib/pixels.ts';
import { useRepeat } from '../lib/repeat.ts';

function draw(canvas: HTMLCanvasElement | null, picture: Picture): void {
  if (canvas === null) {
    return;
  }
  const context = canvas.getContext('2d');
  if (context === null) {
    return;
  }
  if (canvas.width !== picture.width || canvas.height !== picture.height) {
    canvas.width = picture.width;
    canvas.height = picture.height;
  }
  context.putImageData(new ImageData(picture.data, picture.width, picture.height), 0, 0);
}

/** A picture drawn at its own pixels; CSS sets the size it shows at. */
export function PictureCanvas({ picture, label }: { readonly picture: Picture; readonly label: string }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    draw(canvas.current, picture);
  }, [picture]);
  return <canvas ref={canvas} role="img" aria-label={label} />;
}

/** How long the live view waits after drawing a frame before asking for the next. */
const LIVE_PAUSE_MS = 50;

interface LiveProps {
  readonly maxWidth: number;
  /** Called with each frame's size, so the stage can lay it out. */
  readonly onFrame: (width: number, height: number) => void;
  readonly onError: (error: unknown) => void;
}

/** The picked window's newest frame, redrawn as frames arrive. */
export function LiveCanvas({ maxWidth, onFrame, onError }: LiveProps) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const next = useCallback(async (): Promise<void> => {
    try {
      const frame = await preview(maxWidth);
      if (frame !== null) {
        draw(canvas.current, frame);
        onFrame(frame.width, frame.height);
      }
    } catch (error: unknown) {
      onError(error);
    }
  }, [maxWidth, onError, onFrame]);
  useRepeat(next, LIVE_PAUSE_MS, true);
  return <canvas ref={canvas} role="img" aria-label="The live window" />;
}
