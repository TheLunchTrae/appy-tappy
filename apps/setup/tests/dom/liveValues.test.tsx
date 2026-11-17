import { afterEach, expect, test } from 'bun:test';
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { act, cleanup, render, screen, waitFor } from '@testing-library/react';

import type { AppSummary, OverlayView } from '../../src/lib/bindings.ts';
import { ValuesPage } from '../../src/pages/ValuesPage.tsx';

const size = { width: 640, height: 480 };
const windowMatch = { title: 'Player', class: 'Window', executable: 'Player.exe' };
const app: AppSummary = {
  id: 'player',
  error: null,
  running: null,
  windows: [1],
  file: {
    format: 1,
    name: 'Player',
    window: { ...windowMatch, mustMatch: { title: false, class: false }, fallback: 'none' },
    sizeChanges: 'often',
    antiCheat: 'none',
    references: [],
    values: [
      {
        id: 'icon',
        name: 'Playback icon',
        origin: { kind: 'byHand' },
        screen: {
          windowSize: size,
          region: { x: 4, y: 5, width: 10, height: 10 },
          check: { kind: 'color', color: { red: 255, green: 255, blue: 255 }, tolerance: 10 },
          fixes: [],
        },
      },
    ],
  },
};

const view: OverlayView = {
  name: 'Player',
  mode: 'overlay',
  message: null,
  windowSize: size,
  values: [{ id: 'icon', name: 'Playback icon', reading: { kind: 'flag', value: true }, problem: null }],
};

function Page({ apps }: { readonly apps: readonly AppSummary[] }) {
  return (
    <ValuesPage
      apps={apps}
      automations={[]}
      loadError={null}
      onChanged={async () => {}}
      onNew={() => undefined}
      onChange={() => undefined}
      drafts={[]}
      onFinish={() => undefined}
      onDeleteDraft={async () => {}}
    />
  );
}

afterEach(async () => {
  await act(async () => {
    cleanup();
  });
  clearMocks();
});

test('closing the last window removes its live reading and leaves the saved value', async () => {
  mockIPC((command, args) => {
    if (command !== 'live_values') {
      return undefined;
    }
    return args !== undefined &&
      'applications' in args &&
      Array.isArray(args['applications']) &&
      args['applications'].length > 0
      ? [['player', { Ok: view }]]
      : [];
  });

  const page = render(<Page apps={[app]} />);
  await screen.findByRole('columnheader', { name: /640 × 480 now/ });
  await act(async () => {
    page.rerender(<Page apps={[{ ...app, windows: [] }]} />);
  });
  expect(screen.queryAllByRole('columnheader', { name: /640 × 480 now/ })).toHaveLength(0);
  expect(screen.getAllByRole('cell', { name: 'Playback icon' })).toHaveLength(1);
});

test('a new Values page waits for the old page to release its pending captures', async () => {
  const oldRead = Promise.withResolvers<readonly (readonly [string, { readonly Ok: OverlayView }])[]>();
  const calls: unknown[] = [];
  mockIPC((command, args) => {
    if (command !== 'live_values' || args === undefined || !('applications' in args)) {
      throw new Error(`Unexpected command: ${command}`);
    }
    calls.push(args['applications']);
    if (calls.length === 1) {
      return oldRead.promise;
    }
    if (Array.isArray(args['applications']) && args['applications'].length > 0) {
      return [['player', { Ok: { ...view, windowSize: { width: 800, height: 600 } } }]];
    }
    return [];
  });
  const before = render(<Page apps={[app]} />);
  await waitFor(() => {
    expect(calls).toEqual([['player']]);
  });
  before.unmount();
  render(<Page apps={[app]} />);
  await act(async () => {
    oldRead.resolve([['player', { Ok: view }]]);
  });
  expect(calls).toEqual([['player'], [], ['player']]);
  expect(screen.queryAllByRole('columnheader', { name: /640 × 480 now/ })).toHaveLength(0);
  expect(screen.getAllByRole('columnheader', { name: /800 × 600 now/ })).toHaveLength(1);
});
