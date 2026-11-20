import { afterEach, expect, test } from 'bun:test';
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { act, cleanup, renderHook } from '@testing-library/react';

import { PICTURES, type FoundView, type RecordedText } from '../../src/lib/bindings.ts';
import type { Box } from '../../src/lib/boxes.ts';
import { EMPTY_DRAFT } from '../../src/lib/check.ts';
import { background } from '../../src/lib/repeat.ts';
import { useFound } from '../../src/lib/useFound.ts';

const EMPTY: readonly string[] = [];
const BOX: Box = { id: 0, screenshot: 0, region: { x: 0, y: 0, width: 20, height: 10 }, text: '12' };
const picture = (): ArrayBuffer => new Uint8Array(PICTURES.sample.encoded).buffer;
const EXPECTED_PICTURE = {
  width: PICTURES.sample.width,
  height: PICTURES.sample.height,
  data: new Uint8ClampedArray(PICTURES.sample.rgba),
};
const found = (id: number, text: RecordedText = { kind: 'read', text: '12' }): FoundView => ({
  id,
  screenshot: 0,
  finds: [{ region: BOX.region, text, tolerance: 40, characters: [] }],
  more: 0,
  passed: 0,
});

afterEach(() => {
  cleanup();
  clearMocks();
});

test('a late picture from a replaced text search keeps the current search pictures', async () => {
  const oldPicture = Promise.withResolvers<ArrayBuffer>();
  let searches = 0;
  mockIPC((command, args) => {
    if (command === 'find_like') {
      searches += 1;
      return found(searches);
    }
    if (command === 'found_picture') {
      return args !== undefined && 'search' in args && args['search'] === 1 ? oldPicture.promise : picture();
    }
    return undefined;
  });
  const { result } = renderHook(() => useFound(EMPTY_DRAFT, () => undefined, null, EMPTY, EMPTY));
  await act(async () => result.current.search(BOX));
  await act(async () => result.current.search(BOX));
  expect(result.current.view?.id).toBe(2);
  const current = new Map([[0, EXPECTED_PICTURE]]);
  expect(result.current.crops).toEqual(current);
  await act(async () => {
    const stale = picture();
    new Uint8Array(stale, 8).fill(255);
    oldPicture.resolve(stale);
    await oldPicture.promise;
  });
  expect(result.current.crops).toEqual(current);
});

test('rereading the same search still accepts its pending pictures', async () => {
  const pending = Promise.withResolvers<ArrayBuffer>();
  let pictures = 0;
  mockIPC((command) => {
    if (command === 'find_like') {
      return found(1, { kind: 'failed', text: 'Read failed.' });
    }
    if (command === 'reread_found') {
      return found(1);
    }
    if (command === 'found_picture') {
      pictures += 1;
      return pending.promise;
    }
    return undefined;
  });
  const { result } = renderHook(() => useFound(EMPTY_DRAFT, () => undefined, null, EMPTY, EMPTY));
  await act(async () => result.current.search(BOX));
  await act(async () => result.current.reread());
  await act(async () => {
    pending.resolve(picture());
    await pending.promise;
  });
  expect(result.current.crops.get(0)).toEqual(EXPECTED_PICTURE);
  expect(pictures).toBe(1);
});

test('a search stays under way until the newest one answers', async () => {
  const answers = [Promise.withResolvers<FoundView>(), Promise.withResolvers<FoundView>()];
  let searches = 0;
  mockIPC((command) => {
    if (command === 'find_like') {
      searches += 1;
      return answers[searches - 1]?.promise;
    }
    return command === 'found_picture' ? picture() : undefined;
  });
  const { result } = renderHook(() => useFound(EMPTY_DRAFT, () => undefined, null, EMPTY, EMPTY));
  const answer = async (search: number): Promise<void> => {
    await act(async () => {
      answers[search - 1]?.resolve(found(search));
      await answers[search - 1]?.promise;
    });
  };
  act(() => {
    background(result.current.search(BOX));
  });
  act(() => {
    background(result.current.search(BOX));
  });
  await answer(1);
  expect(result.current.view).toBeNull();
  expect(result.current.searching).toBe(true);
  await answer(2);
  expect(result.current.view?.id).toBe(2);
  expect(result.current.searching).toBe(false);
});

test('a search that a dismissal or a reread overtakes is no longer under way', async () => {
  const late = Promise.withResolvers<FoundView>();
  let searches = 0;
  mockIPC((command) => {
    if (command === 'find_like') {
      searches += 1;
      return searches === 2 ? found(2, { kind: 'failed', text: 'Read failed.' }) : late.promise;
    }
    if (command === 'reread_found') {
      return found(2);
    }
    return command === 'found_picture' ? picture() : undefined;
  });
  const { result } = renderHook(() => useFound(EMPTY_DRAFT, () => undefined, null, EMPTY, EMPTY));
  act(() => {
    background(result.current.search(BOX));
  });
  expect(result.current.searching).toBe(true);
  act(() => {
    result.current.dismiss();
  });
  expect(result.current.searching).toBe(false);

  await act(async () => result.current.search(BOX));
  act(() => {
    background(result.current.search(BOX));
  });
  expect(result.current.searching).toBe(true);
  await act(async () => result.current.reread());
  expect(result.current.searching).toBe(false);
  expect(result.current.view?.finds[0]?.text).toEqual({ kind: 'read', text: '12' });
});
