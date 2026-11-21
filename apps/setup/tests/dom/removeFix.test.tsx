import { afterEach, expect, test } from 'bun:test';
import { useState } from 'react';
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
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
function Page({ saved = fixed }: { readonly saved?: ValueDef }) {
  const [value, setValue] = useState(saved);
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

test('a window that takes another size with a fix closes the question about the fix at the first', async () => {
  const other = { width: 800, height: 600 };
  let size = now;
  let removals = 0;
  mockIPC((command) => {
    if (command === 'live_values') {
      return [['player', { Ok: { ...view, windowSize: size } }]];
    }
    if (command === 'remove_fix') {
      removals += 1;
    }
    return undefined;
  });
  const user = userEvent.setup();
  const both: ValueDef = {
    ...fixed,
    screen: { ...fixed.screen, fixes: [...fixed.screen.fixes, { windowSize: other, region, images: [] }] },
  };
  render(<Page saved={both} />);

  await user.click(await screen.findByRole('button', { name: 'Remove fix of Playback icon' }));
  expect(screen.getByRole('alertdialog', { name: 'Remove the fix of Playback icon at 1280 × 720?' })).not.toBeNull();
  size = other;
  // The page asks what the values read once a second.
  await waitFor(
    () => {
      expect(screen.getByRole('columnheader', { name: /At 800 × 600 now/u })).not.toBeNull();
    },
    { timeout: 3000 },
  );
  // The fix at the size now is another one, which nobody asked about.
  expect(screen.queryByRole('alertdialog')).toBeNull();
  expect(screen.getByRole('button', { name: 'Remove fix of Playback icon' })).not.toBeNull();
  expect(removals).toBe(0);
});

test('a removal that fails says why on the page and puts the focus back on Remove fix', async () => {
  mockIPC(async (command) => {
    if (command === 'live_values') {
      return [['player', { Ok: view }]];
    }
    return command === 'remove_fix' ? Promise.reject("AppyTappy can't save Player. Try again.") : undefined;
  });
  const user = userEvent.setup();
  render(<Page />);

  await user.click(await screen.findByRole('button', { name: 'Remove fix of Playback icon' }));
  await user.click(screen.getByRole('button', { name: 'Remove fix' }));
  expect((await screen.findByRole('alert')).textContent).toBe("AppyTappy can't save Player. Try again.");
  expect(screen.queryByRole('alertdialog')).toBeNull();
  expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Remove fix of Playback icon' }));
});
