import { afterEach, expect, test } from 'bun:test';
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { act, cleanup, render, renderHook, screen } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';

import type { AppSummary, AppWindows, WindowChoice } from '../../src/lib/bindings.ts';
import { useWindowSearch } from '../../src/lib/useWindowSearch.ts';
import { Picker } from '../../src/components/Picker.tsx';

function summary(id: string, name: string): AppSummary {
  return {
    id,
    error: null,
    running: null,
    windows: [],
    file: {
      format: 1,
      name,
      window: {
        title: name,
        class: 'Window',
        executable: `${name}.exe`,
        mustMatch: { title: false, class: false },
        fallback: 'none',
      },
      sizeChanges: 'often',
      antiCheat: 'none',
      references: [],
      values: [],
    },
  };
}

const window: WindowChoice = {
  handle: 7,
  title: 'Player',
  class: 'Window',
  executable: 'Player.exe',
  width: 640,
  height: 480,
};

/** The saved applications `apps`, Player with its one window open when `open` says so. */
function windows(apps: readonly AppSummary[], open: boolean): AppWindows {
  return {
    open: { windows: open ? [window] : [], antiCheat: [] },
    apps: apps.map((app) => ({
      app,
      fits: open && app.id === 'player' ? [{ handle: 7, fallback: false, best: true, tied: true }] : [],
    })),
  };
}

/** Answers the window listing with `look`, and nothing else. */
function native(look: () => AppWindows): void {
  mockIPC((command) => (command === 'list_app_windows' ? look() : null));
}

afterEach(async () => {
  await act(async () => {
    cleanup();
  });
  clearMocks();
});

const nothing = (): void => undefined;

function picker(onChoose: (app: string) => void) {
  return (
    <Picker
      heading="New reference"
      trail={['References']}
      continued={false}
      onCancel={nothing}
      onChoose={onChoose}
      onAddApplication={nothing}
    />
  );
}

test('the only saved application is taken without asking', async () => {
  native(() => windows([summary('player', 'Player')], true));
  const chosen: string[] = [];
  render(picker((app) => chosen.push(app)));
  await act(async () => {});
  expect(chosen).toEqual(['player']);
});

test('one of several saved applications is taken once it is chosen, with no window to pick', async () => {
  native(() => windows([summary('player', 'Player'), summary('other', 'Other')], true));
  const chosen: string[] = [];
  render(picker((app) => chosen.push(app)));
  await act(async () => {});
  expect(chosen).toEqual([]);
  await userEvent.click(screen.getByRole('button', { name: /^Player/u }));
  expect(chosen).toEqual(['player']);
});

test('an application that is not open is captured once it opens, with nothing pressed', async () => {
  let open = false;
  native(() => windows([summary('player', 'Player')], open));
  const picked: number[] = [];
  const { result } = renderHook(() =>
    useWindowSearch('player', async (handle) => {
      picked.push(handle);
    }),
  );
  await act(async () => {});
  expect(result.current.absence?.heading).toBe("Player isn't open");
  open = true;
  await act(async () => {
    await new Promise((resolve) => {
      setTimeout(resolve, 2100);
    });
  });
  expect(picked).toEqual([7]);
  expect(result.current.absence).toBeNull();
});
