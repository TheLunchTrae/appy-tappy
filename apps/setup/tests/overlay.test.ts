import { describe, expect, test } from 'bun:test';

import { overlayOf } from '../src/lib/overlay.ts';

describe('overlayOf', () => {
  test('names the application and mode of an overlay window and nothing else', () => {
    expect(overlayOf('overlay-example-2')).toEqual({ app: 'example-2', mode: 'overlay' });
    expect(overlayOf('panel-example')).toEqual({ app: 'example', mode: 'pictureInPicture' });
    for (const label of ['main', 'overlay-', 'panel-', 'overlays', 'xpanel-example']) {
      expect(overlayOf(label)).toBeNull();
    }
  });
});
