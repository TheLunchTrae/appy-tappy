import { afterEach, expect, test } from 'bun:test';
import { useState } from 'react';
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';

import type { AppSummary, OverlayView, ValueDef } from '../../src/lib/bindings.ts';
import { ValuesPage } from '../../src/pages/ValuesPage.tsx';

const now = { width: 1280, height: 720 };
const region = { x: 4, y: 5, width: 10, height: 10 };
const unfixed: ValueDef = {
  id: 'icon',
  name: 'Playback icon',
  origin: { kind: 'byHand' },
  screen: {
    windowSize: { width: 640, height: 480 },
    region,
    check: { kind: 'color', color: { red: 255, green: 255, blue: 255 }, tolerance: 10 },
    fixes: [],
  },
};
const fixed: ValueDef = { ...unfixed, screen: { ...unfixed.screen, fixes: [{ windowSize: now, region, images: [] }] } };

function appWith(value: ValueDef): AppSummary {
  return {
    id: 'player',
    error: null,
    running: null,
    windows: [1],
    file: {
      format: 1,
      name: 'Player',
      window: {
        title: 'Player',
        class: 'Window',
        executable: 'Player.exe',
        mustMatch: { title: false, class: false },
        fallback: 'none',
      },
      sizeChanges: 'often',
      antiCheat: 'none',
      references: [],
      values: [value],
    },
  };
}

const view: OverlayView = {
  name: 'Player',
  mode: 'overlay',
  message: null,
  windowSize: now,
  values: [{ id: 'icon', name: 'Playback icon', reading: { kind: 'flag', value: true }, problem: null }],
};

/** The page over files that lose the fix once the page reads them again. */
function Page() {
  const [value, setValue] = useState(fixed);
  return (
    <ValuesPage
      apps={[appWith(value)]}
      automations={[]}
      loadError={null}
      onChanged={async () => {
        setValue(unfixed);
      }}
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

test('Remove fix asks first, removes once, and leaves the focus in the row', async () => {
  const pending = Promise.withResolvers<undefined>();
  let removals = 0;
  mockIPC((command) => {
    if (command === 'live_values') {
      return [['player', { Ok: view }]];
    }
    if (command === 'remove_fix') {
      removals += 1;
      return pending.promise;
    }
    return undefined;
  });
  const user = userEvent.setup();
  render(<Page />);
  const removeFix = async (): Promise<HTMLElement> =>
    screen.findByRole('button', { name: 'Remove fix of Playback icon' });

  await user.click(await removeFix());
  expect(removals).toBe(0);
  expect(screen.getByRole('alertdialog').textContent).toContain('Remove the fix of Playback icon at 1280 × 720?');
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Keep' }));
  await user.keyboard('{Escape}');
  expect(removals).toBe(0);
  expect(document.activeElement).toBe(await removeFix());

  await user.click(await removeFix());
  const answer = screen.getByRole('button', { name: 'Remove fix' });
  fireEvent.click(answer);
  fireEvent.click(answer);
  expect(removals).toBe(1);

  await act(async () => {
    pending.resolve(undefined);
    await pending.promise;
  });
  expect(screen.queryByRole('button', { name: 'Remove fix of Playback icon' })).toBeNull();
  expect(screen.getByRole('row', { name: /Playback icon/ }).contains(document.activeElement)).toBe(true);
});
