import { describe, expect, test } from 'bun:test';

import type { AppSummary, WindowChoice, WindowFit } from '../src/lib/bindings.ts';
import { exactWindow } from '../src/lib/matching.ts';
import {
  type Look,
  type PickerApp,
  type Seen,
  chooserStep,
  isRunning,
  known,
  offersMatchEdit,
  pickerStep,
  seen,
} from '../src/lib/picker.ts';

function open(handle: number, title: string): WindowChoice {
  return { handle, title, class: 'ExampleWindow', executable: 'Example.exe', width: 1280, height: 720 };
}

const main = open(3, 'Example App - Save 3');
const second = open(8, 'Example App - Save 4');
const settings = open(5, 'Settings');

const read: WindowFit = { handle: 3, fallback: false, best: true, tied: true };
const twin: WindowFit = { handle: 8, fallback: false, best: false, tied: true };
const worse: WindowFit = { handle: 5, fallback: false, best: false, tied: false };

/** A saved application as the look answers it. */
function summary(id: string, name: string | null): AppSummary {
  return {
    id,
    file:
      name === null
        ? null
        : {
            format: 1,
            name,
            window: exactWindow({ title: name, class: 'ExampleWindow', executable: `${id}.exe` }),
            sizeChanges: 'often',
            antiCheat: 'unknown',
            values: [],
            references: [],
          },
    error: name === null ? 'broken' : null,
    windows: [],
    running: null,
  };
}

function app(id: string, name: string, fits: readonly WindowFit[]): PickerApp {
  return { id, name, executable: `${id}.exe`, fits, summary: summary(id, name) };
}

const player = app('player', 'Player', []);

/** What a look found: Example App with `fits` and Player, among every window above. */
function found(fits: readonly WindowFit[], others: readonly PickerApp[] = [player]): Seen {
  return { apps: [app('example', 'Example App', fits), ...others], windows: [main, second, settings], antiCheat: [] };
}

function answered(fits: readonly WindowFit[], others?: readonly PickerApp[]): Look {
  return { kind: 'seen', seen: found(fits, others) };
}

/** Where the look for Example App's window stands. */
function chosen(look: Look) {
  return pickerStep('example', look);
}

const example = app('example', 'Example App', [read]);

describe('a look', () => {
  test('keeps the applications whose files read, with their fits', () => {
    const answer = {
      open: { windows: [main], antiCheat: ['BattlEye'] },
      apps: [
        { app: summary('broken', null), fits: [] },
        { app: summary('example', 'Example App'), fits: [read] },
      ],
    };
    expect(seen(answer)).toEqual({ apps: [example], windows: [main], antiCheat: ['BattlEye'] });
  });

  test('keeps what the last one found on show while the next is made or fails', () => {
    const before = found([read]);
    expect(known({ kind: 'seen', seen: before })).toBe(before);
    expect(known({ kind: 'looking', before })).toBe(before);
    expect(known({ kind: 'failed', error: 'no answer', before })).toBe(before);
    expect(known({ kind: 'looking', before: null })).toBeNull();
  });

  test('says whether each application runs only once it has answered', () => {
    expect(isRunning(answered([read]), example)).toBe(true);
    expect(isRunning(answered([]), player)).toBe(false);
    expect(isRunning({ kind: 'looking', before: found([read]) }, example)).toBeNull();
    expect(isRunning({ kind: 'failed', error: 'no answer', before: found([read]) }, example)).toBeNull();
  });
});

describe('choosing the application', () => {
  test('knows nothing until the first look answers, and says why a look failed', () => {
    expect(chooserStep({ kind: 'looking', before: null })).toEqual({ kind: 'looking' });
    expect(chooserStep({ kind: 'failed', error: 'Windows did not answer', before: found([read]) })).toEqual({
      kind: 'failed',
      error: 'Windows did not answer',
    });
  });

  test('asks only when there are several saved applications, and takes the only one without asking', () => {
    const none: Look = { kind: 'seen', seen: { apps: [], windows: [main], antiCheat: [] } };
    expect(chooserStep(none)).toEqual({ kind: 'no-applications' });
    expect(chooserStep(answered([read]))).toMatchObject({ kind: 'choosing' });
    expect(chooserStep(answered([read], []))).toEqual({ kind: 'only', app: example });
  });
});

describe("the look for an application's window", () => {
  test('knows nothing until the first look answers', () => {
    expect(chosen({ kind: 'looking', before: null })).toEqual({ kind: 'looking' });
  });

  test('says the look failed, whatever was found before', () => {
    for (const before of [null, found([read])]) {
      expect(chosen({ kind: 'failed', error: 'Windows did not answer', before })).toEqual({
        kind: 'failed',
        error: 'Windows did not answer',
      });
    }
  });

  test('says so when the application is not among the saved ones', () => {
    expect(pickerStep('gone', answered([read]))).toEqual({ kind: 'missing' });
  });

  test('waits for the look under way, naming the application the last one found', () => {
    expect(chosen({ kind: 'looking', before: found([read]) })).toEqual({ kind: 'resolving', app: example });
  });

  test('has the one window the application reads', () => {
    const one = chosen(answered([read, worse]));
    expect(one).toEqual({
      kind: 'one',
      app: app('example', 'Example App', [read, worse]),
      window: main,
      fallback: false,
    });
    expect(offersMatchEdit(one)).toBe(false);
    expect(chosen(answered([{ ...read, fallback: true }]))).toMatchObject({ kind: 'one', fallback: true });
  });

  test('lists only the windows that fit equally well, and marks the one a session would read now', () => {
    const several = chosen(answered([read, twin, worse]));
    expect(several).toEqual({
      kind: 'several',
      app: app('example', 'Example App', [read, twin, worse]),
      windows: [main, second],
      reads: 3,
      fallback: false,
    });
    expect(offersMatchEdit(several)).toBe(true);
    const fallbacks = chosen(
      answered([
        { ...twin, best: true, fallback: true },
        { ...read, best: false, fallback: true },
      ]),
    );
    expect(fallbacks).toMatchObject({ kind: 'several', reads: 8, fallback: true });
  });

  test('has no window when none fits', () => {
    const none = chosen(answered([]));
    expect(none).toMatchObject({ kind: 'none', unlisted: false });
    expect(offersMatchEdit(none)).toBe(true);
  });

  test('has no window when a session reads one that is not listed', () => {
    expect(chosen(answered([{ ...read, best: false, tied: false }, worse]))).toMatchObject({
      kind: 'none',
      unlisted: true,
    });
  });
});
