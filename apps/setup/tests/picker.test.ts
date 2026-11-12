import { describe, expect, test } from 'bun:test';

import type { AppSummary, WindowChoice, WindowFit } from '../src/lib/bindings.ts';
import { exactWindow } from '../src/lib/matching.ts';
import {
  type Look,
  type PickerApp,
  captureWindow,
  isRunning,
  offersMatchEdit,
  pickerApps,
  pickerStep,
  startingApp,
  stepStatus,
} from '../src/lib/picker.ts';

const example: PickerApp = {
  id: 'example',
  name: 'Example App',
  window: exactWindow({ title: 'Example App', class: 'ExampleWindow', executable: 'Example.exe' }),
};
const player: PickerApp = { ...example, id: 'player', name: 'Player' };

function open(handle: number, title: string): WindowChoice {
  return { handle, title, class: 'ExampleWindow', executable: 'Example.exe', width: 1280, height: 720 };
}

const main = open(3, 'Example App - Save 3');
const second = open(8, 'Example App - Save 4');
const settings = open(5, 'Settings');

const read: WindowFit = { handle: 3, fallback: false, best: true, tied: true };
const twin: WindowFit = { handle: 8, fallback: false, best: false, tied: true };
const worse: WindowFit = { handle: 5, fallback: false, best: false, tied: false };

/** A look that found `fits` for Example App among every window above. */
function found(fits: readonly WindowFit[]): Look {
  return { kind: 'found', windows: [main, second, settings], fits: new Map([['example', fits]]) };
}

/** Where the picker stands with Example App chosen. */
function chosen(look: Look, picked: number | null = null) {
  return pickerStep({ saved: [example, player], locked: false, chosen: 'example', look, picked });
}

describe('the applications', () => {
  test('are the saved ones whose files read', () => {
    const summary = (id: string, file: AppSummary['file']): AppSummary => ({
      id,
      file,
      error: file === null ? 'broken' : null,
      windows: [],
      running: null,
    });
    const file: AppSummary['file'] = {
      format: 1,
      name: 'Example App',
      window: example.window,
      sizeChanges: 'often',
      antiCheat: 'unknown',
      values: [],
      references: [],
    };
    expect(pickerApps([summary('broken', null), summary('example', file)])).toEqual([example]);
  });

  test('start with the one the flow belongs to, then the one handed over, then the only one', () => {
    expect(startingApp([example, player], 'player', 'example')).toBe('player');
    expect(startingApp([example, player], null, 'example')).toBe('example');
    expect(startingApp([example], null, null)).toBe('example');
    expect(startingApp([example, player], null, null)).toBeNull();
    expect(startingApp([], null, null)).toBeNull();
  });

  test('say whether each runs once a look has answered', () => {
    expect(isRunning({ kind: 'looking' }, 'example')).toBeNull();
    expect(isRunning({ kind: 'failed', error: 'no answer' }, 'example')).toBeNull();
    expect(isRunning(found([read]), 'example')).toBe(true);
    expect(isRunning(found([]), 'example')).toBe(false);
    expect(isRunning(found([read]), 'player')).toBeNull();
  });
});

describe('the picker', () => {
  test('asks for an application before it looks for a window', () => {
    const look = found([read]);
    const before = { locked: false, chosen: null, look, picked: null };
    expect(pickerStep({ ...before, saved: [] })).toEqual({ kind: 'no-applications' });
    expect(pickerStep({ ...before, saved: [example, player] })).toEqual({ kind: 'choosing' });
    // An application just added shows once the list that has it arrives.
    expect(pickerStep({ ...before, saved: [player], chosen: 'example' })).toEqual({ kind: 'choosing' });
  });

  test('has nothing to choose when the application a flow belongs to has gone', () => {
    const gone = pickerStep({ saved: [player], locked: true, chosen: 'example', look: found([read]), picked: null });
    expect(gone).toEqual({ kind: 'missing' });
    expect(captureWindow(gone)).toBeNull();
  });

  test('waits for the look, and for one that knows the chosen application', () => {
    expect(chosen({ kind: 'looking' })).toEqual({ kind: 'resolving', app: example });
    const before: Look = { kind: 'found', windows: [main], fits: new Map() };
    expect(chosen(before)).toEqual({ kind: 'resolving', app: example });
  });

  test('says why when the look fails', () => {
    const failed = chosen({ kind: 'failed', error: 'Windows did not answer' });
    expect(failed).toEqual({ kind: 'failed', app: example, error: 'Windows did not answer' });
    expect(captureWindow(failed)).toBeNull();
    expect(stepStatus(failed)).toEqual({
      text: "Couldn't look for Example App's window: Windows did not answer. Refresh windows to try again.",
      warn: true,
    });
  });

  test('captures the one window the application reads without asking', () => {
    const one = chosen(found([read, worse]));
    expect(one).toEqual({ kind: 'one', app: example, window: main });
    expect(captureWindow(one)).toEqual(main);
    expect(stepStatus(one)).toEqual({ text: "Example App's window is “Example App - Save 3”.", warn: false });
    expect(offersMatchEdit(one)).toBe(false);
  });

  test('asks which of the windows that fit equally well, and offers no other', () => {
    const several = chosen(found([read, twin, worse]));
    expect(several).toEqual({ kind: 'several', app: example, windows: [main, second], picked: null });
    expect(captureWindow(several)).toBeNull();
    expect(stepStatus(several).text).toStartWith('2 open windows fit Example App equally well');
    expect(stepStatus(several).warn).toBe(false);
    expect(offersMatchEdit(several)).toBe(true);

    expect(captureWindow(chosen(found([read, twin, worse]), 8))).toEqual(second);
    // A window that fits worse is never read, so it cannot be the one picked.
    expect(captureWindow(chosen(found([read, twin, worse]), 5))).toBeNull();
  });

  test('says so when no window fits, and offers no window', () => {
    const none = chosen(found([]));
    expect(none).toEqual({ kind: 'none', app: example, unlisted: false });
    expect(captureWindow(none)).toBeNull();
    expect(stepStatus(none).text).toStartWith("Example App isn't open, or its window doesn't match");
    expect(stepStatus(none).warn).toBe(true);
    expect(offersMatchEdit(none)).toBe(true);
  });

  test('offers no window when a session reads one that is not listed', () => {
    const unlisted = chosen(found([{ ...read, best: false, tied: false }, worse]));
    expect(unlisted).toEqual({ kind: 'none', app: example, unlisted: true });
    expect(stepStatus(unlisted).text).toStartWith("AppyTappy reads a window of Example App that can't be captured");
  });

  test('leaves out a window that closed after it was matched', () => {
    const closed: WindowFit = { handle: 21, fallback: false, best: false, tied: true };
    expect(chosen(found([read, closed]))).toEqual({ kind: 'one', app: example, window: main });
  });
});
