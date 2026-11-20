import { afterEach, expect, test } from 'bun:test';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';

import type { RecordedReading, RecordingView } from '../../src/lib/bindings.ts';
import { RecordPanel } from '../../src/components/Recording.tsx';

const size = { width: 640, height: 480 };
const reading: RecordedReading = {
  firstMs: 1000,
  seen: 2,
  size: null,
  text: { kind: 'read', text: '42' },
  shapes: [],
  width: 20,
  characters: [],
};
const empty: RecordingView = {
  id: 1,
  recording: false,
  elapsedMs: 5000,
  full: null,
  problem: null,
  settings: { region: { x: 4, y: 5, width: 20, height: 10 }, tolerance: 40, windowSize: size, scope: 'place' },
  at: size,
  sizes: [],
  spots: [],
  readings: [],
  shapes: [],
};

function panel(view: RecordingView, onDiscard: () => Promise<void>) {
  return (
    <RecordPanel
      view={view}
      found={{ missing: [], unchecked: [], found: [], unnamed: 0 }}
      needs={false}
      reviewing={false}
      kept={0}
      keeping={0}
      blocked={null}
      scope="place"
      onScope={() => undefined}
      appName="Player"
      crops={new Map()}
      error={null}
      onStart={() => undefined}
      onMore={() => undefined}
      onStop={() => undefined}
      onDiscard={onDiscard}
      onReview={() => undefined}
      settingsChanged={false}
      onUseSettings={() => undefined}
      focus={{ title: { current: null }, more: { current: null } }}
    />
  );
}

afterEach(async () => {
  await act(async () => {
    cleanup();
  });
});

test.each([
  ['under way', true],
  ['stopped', false],
])('Discard asks before it drops the readings of a recording that is %s', async (_state, recording) => {
  const pending = Promise.withResolvers<undefined>();
  let discards = 0;
  const user = userEvent.setup();
  render(
    panel({ ...empty, recording, readings: [reading, reading] }, async () => {
      discards += 1;
      await pending.promise;
    }),
  );
  const button = (name: string): HTMLElement => screen.getByRole('button', { name });

  await user.click(button('Discard'));
  expect(discards).toBe(0);
  expect(screen.getByRole('alertdialog').textContent).toContain('You lose its 2 readings');
  expect(document.activeElement).toBe(button('Keep recording'));
  await user.keyboard('{Escape}');
  expect(discards).toBe(0);
  expect(document.activeElement).toBe(button('Discard'));

  await user.click(button('Discard'));
  fireEvent.click(button('Discard recording'));
  fireEvent.click(button('Discard recording'));
  expect(discards).toBe(1);
  await act(async () => {
    pending.resolve(undefined);
    await pending.promise;
  });
});

test('Discard drops a recording with no readings at once', async () => {
  let discards = 0;
  const user = userEvent.setup();
  render(
    panel({ ...empty, recording: true }, async () => {
      discards += 1;
    }),
  );

  await user.click(screen.getByRole('button', { name: 'Discard' }));
  expect(discards).toBe(1);
  expect(screen.queryByRole('alertdialog')).toBeNull();
});
