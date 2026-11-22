import { afterEach, expect, test } from 'bun:test';
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { useState } from 'react';

import type { RecordingView, SelectionView } from '../../src/lib/bindings.ts';
import { type Draft, EMPTY_DRAFT, withKind } from '../../src/lib/check.ts';
import { useRecorder } from '../../src/lib/useRecorder.ts';
import { useRecording } from '../../src/lib/useRecording.ts';
import { useScreenshots } from '../../src/lib/useScreenshots.ts';
import { useStateSelections } from '../../src/lib/useStateSelections.ts';

// Deferred IPC makes each race deterministic: the test controls when the old work answers.
afterEach(() => {
  cleanup();
  clearMocks();
});

test('a state selection cannot move a different kind of check after switching kinds', async () => {
  const pending = Promise.withResolvers<SelectionView>();
  let asked = false;
  mockIPC((command) => {
    if (command === 'select') {
      asked = true;
      return pending.promise;
    }
    return undefined;
  });
  const { result } = renderHook(() => {
    const [draft, setDraft] = useState<Draft>({
      ...EMPTY_DRAFT,
      kind: 'state',
      states: [{ name: 'Ready', screenshot: 0, steps: [{ kind: 'click', x: 1, y: 1, combine: 'replace' }] }],
    });
    useStateSelections(draft, setDraft, 0);
    return { draft, setDraft };
  });
  await waitFor(() => expect(asked).toBe(true));
  const region = { x: 20, y: 20, width: 30, height: 30 };
  act(() => result.current.setDraft((draft) => ({ ...withKind(draft, 'number'), region })));
  await act(async () => {
    pending.resolve({ region: { x: 1, y: 1, width: 2, height: 2 }, spans: [[0, 4]] });
    await pending.promise;
  });
  expect(result.current.draft.region).toEqual(region);
});

test('a screenshot failure after leaving capture cannot remove a new captures screenshot', async () => {
  const pixels = Promise.withResolvers<ArrayBuffer>();
  let asked = false;
  const removed: unknown[] = [];
  mockIPC((command, args) => {
    if (command === 'take_screenshot') {
      return 0;
    }
    if (command === 'screenshot') {
      asked = true;
      return pixels.promise;
    }
    if (command === 'remove_screenshots') {
      removed.push(args);
    }
    return undefined;
  });
  const { result, unmount } = renderHook(() => useScreenshots());
  let taken: Promise<unknown> | undefined;
  act(() => {
    taken = result.current.take();
  });
  await waitFor(() => expect(asked).toBe(true));
  unmount();
  await act(async () => {
    pixels.reject(new Error('old capture ended'));
    await taken;
  });
  expect(removed).toEqual([]);
});

test('closing the reference editor stops a late recording start without reviving review', async () => {
  const pending = Promise.withResolvers<number>();
  const stopped: unknown[] = [];
  let read = false;
  let started = false;
  mockIPC((command, args) => {
    if (command === 'start_recording') {
      return pending.promise;
    }
    if (command === 'stop_recording') {
      stopped.push(args);
    }
    if (command === 'recording_state') {
      read = true;
      return null;
    }
    return undefined;
  });
  const { result, rerender } = renderHook(({ active }) => useRecording(active), { initialProps: { active: true } });
  let start: Promise<void> | undefined;
  act(() => {
    start = result.current.start(
      {
        app: 'example',
        region: { x: 0, y: 0, width: 4, height: 2 },
        drawnOn: 0,
        scope: 'place',
        examples: [],
      },
      false,
      () => {
        started = true;
      },
    );
  });
  rerender({ active: false });
  await act(async () => {
    pending.resolve(7);
    await start;
  });
  expect(stopped).toEqual([{ recording: 7 }]);
  expect(started).toBe(false);
  expect(read).toBe(false);
  expect(result.current.view).toBeNull();
});

test('closing the reference editor cancels a stop waiting for its final state', async () => {
  const pending = Promise.withResolvers<RecordingView>();
  let asked = false;
  mockIPC((command) => {
    if (command === 'recording_state') {
      asked = true;
      return pending.promise;
    }
    return undefined;
  });
  const { result, rerender } = renderHook(({ active }) => useRecording(active), { initialProps: { active: true } });
  let stop: Promise<boolean> | undefined;
  act(() => {
    stop = result.current.stop();
  });
  await waitFor(() => expect(asked).toBe(true));
  rerender({ active: false });
  await act(async () => {
    pending.resolve({
      id: 7,
      recording: false,
      elapsedMs: 0,
      full: null,
      problem: null,
      settings: {
        region: { x: 0, y: 0, width: 4, height: 2 },
        windowSize: { width: 4, height: 2 },
        scope: 'place',
      },
      readings: [],
      shapes: [],
      sizes: [],
      spots: [],
      at: { width: 4, height: 2 },
    });
    expect(await stop).toBe(false);
  });
  expect(result.current.view).toBeNull();
});

test('repeated Record more clicks share one pending command without stopping its ticket', async () => {
  const pending = Promise.withResolvers<number>();
  let requests = 0;
  let started = 0;
  const stopped: unknown[] = [];
  mockIPC((command, args) => {
    if (command === 'start_recording') {
      requests += 1;
      return pending.promise;
    }
    if (command === 'stop_recording') {
      stopped.push(args);
    }
    return null;
  });
  const { result } = renderHook(() => useRecording());
  const settings = {
    app: 'example',
    region: { x: 0, y: 0, width: 4, height: 2 },
    ink: null,
    tolerance: 40,
    drawnOn: 0,
    pin: null,
    scope: 'place' as const,
    examples: [],
  };
  const onStarted = (): void => {
    started += 1;
  };
  await act(async () => {
    const first = result.current.start(settings, true, onStarted);
    const second = result.current.start(settings, true, onStarted);
    pending.resolve(7);
    await Promise.all([first, second]);
  });
  expect(requests).toBe(1);
  expect(started).toBe(1);
  expect(stopped).toEqual([]);
});

test('a state read begun during a command cannot replace the command result after it finishes', async () => {
  const reading = Promise.withResolvers<boolean>();
  const changing = Promise.withResolvers<void>();
  const read = async (): Promise<boolean> => reading.promise;
  const live = (): boolean => false;
  const { result } = renderHook(() => useRecorder(read, live));
  let command: Promise<boolean> | undefined;
  let state: Promise<void> | undefined;
  act(() => {
    command = result.current.command(async (current) => {
      await changing.promise;
      if (current()) {
        result.current.show(false);
      }
    });
    state = result.current.readState();
  });
  await act(async () => {
    changing.resolve();
    expect(await command).toBe(true);
  });
  expect(result.current.view).toBe(false);
  await act(async () => {
    reading.resolve(true);
    await state;
  });
  expect(result.current.view).toBe(false);
});
