import { describe, expect, test } from 'bun:test';

import { OVERLAY_LABELS } from '../src/lib/bindings.ts';
import { overlayOf } from '../src/lib/overlay.ts';

describe('overlayOf', () => {
  test('names the application and mode of an overlay window and nothing else', () => {
    const { overlay, pictureInPicture } = OVERLAY_LABELS;
    expect(overlayOf(`${overlay}example-2`)).toEqual({ app: 'example-2', mode: 'overlay' });
    expect(overlayOf(`${pictureInPicture}example`)).toEqual({ app: 'example', mode: 'pictureInPicture' });
    for (const label of ['main', overlay, pictureInPicture, 'overlays', `x${pictureInPicture}example`]) {
      expect(overlayOf(label)).toBeNull();
    }
  });
});
