import { afterEach, expect, test } from 'bun:test';
import { Channel } from '@tauri-apps/api/core';
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import type { ShortcutEvent } from '@tauri-apps/plugin-global-shortcut';
import { act, cleanup, renderHook, waitFor } from '@testing-library/react';

import { useScreenshotShortcut } from '../../src/lib/useScreenshotShortcut.ts';

afterEach(async () => {
  await act(async () => {
    cleanup();
  });
  clearMocks();
});

test('changing screens releases the old screenshot shortcut before registering it again', async () => {
  const registered = Promise.withResolvers<void>();
  const commands: string[] = [];
  let presses = 0;
  let handler: Channel<ShortcutEvent> | undefined;
  mockIPC((command, args) => {
    commands.push(command);
    if (commands.length === 1) {
      const channel = args !== undefined && 'handler' in args ? args['handler'] : undefined;
      if (!(channel instanceof Channel)) {
        throw new Error('Expected the screenshot callback');
      }
      handler = channel;
      return registered.promise;
    }
    return undefined;
  });
  const press = async (): Promise<void> => {
    presses += 1;
  };
  const before = renderHook(() => useScreenshotShortcut(press));
  await waitFor(() => {
    expect(commands).toEqual(['plugin:global-shortcut|register']);
  });
  before.unmount();
  expect(!!handler).toBe(true);
  handler?.onmessage({ id: 1, shortcut: 'F9', state: 'Pressed' });
  const after = renderHook(() => useScreenshotShortcut(press));
  await act(async () => {
    registered.resolve();
  });
  expect(presses).toBe(0);
  expect(commands).toEqual([
    'plugin:global-shortcut|register',
    'plugin:global-shortcut|unregister',
    'plugin:global-shortcut|register',
  ]);
  expect(after.result.current.error).toBeNull();
});

test('a refused screenshot shortcut leaves no registration for cleanup', async () => {
  const commands: string[] = [];
  mockIPC((command) => {
    commands.push(command);
    if (command === 'plugin:global-shortcut|register') {
      throw new Error('Already in use');
    }
    return undefined;
  });
  const hook = renderHook(() => useScreenshotShortcut(async () => {}));
  await waitFor(() => {
    expect(hook.result.current.error).toContain('could not be set up');
  });
  await act(async () => {
    hook.unmount();
  });
  expect(commands).toEqual(['plugin:global-shortcut|register']);
});
