import { afterEach, expect, test } from 'bun:test';
import { cleanup, render } from '@testing-library/react';

import type { RecordingView } from '../../src/lib/bindings.ts';
import { RecordingSpots } from '../../src/components/Recording.tsx';

afterEach(cleanup);

const at = { width: 640, height: 480 };
const view: RecordingView = {
  id: 1,
  recording: true,
  elapsedMs: 1000,
  full: null,
  problem: null,
  settings: { region: { x: 40, y: 20, width: 100, height: 40 }, windowSize: at, scope: 'window' },
  at,
  sizes: [],
  spots: [{ x: 50, y: 25, width: 60, height: 20 }],
  place: { x: 40, y: 20, width: 100, height: 40 },
  readings: [],
  shapes: [],
};

test("a reference's recording outlines its whole region, beside the tighter text it found", () => {
  // Shown at half the size it records at.
  const { container } = render(<RecordingSpots view={view} shown={{ width: 320, height: 240 }} place />);
  const place = container.querySelector<HTMLElement>('.place');
  expect(place?.style.width).toBe('50px');
  expect(place?.style.left).toBe('20px');
  expect(container.querySelectorAll('.spot')).toHaveLength(1);
  cleanup();
  // A value's stage outlines its own region, so only the spots show.
  const { container: value } = render(<RecordingSpots view={view} shown={at} place={false} />);
  expect(value.querySelector('.place')).toBeNull();
});
