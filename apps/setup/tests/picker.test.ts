import { describe, expect, test } from 'bun:test';

import type { AppSummary, WindowChoice, WindowFit } from '../src/lib/bindings.ts';
import { exactWindow } from '../src/lib/matching.ts';
import {
  type Look,
  type PickerApp,
  type Seen,
  captureWindow,
  isRunning,
  known,
  offersMatchEdit,
  pickerStep,
  seen,
  stepStatus,
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
            origin: { kind: 'byHand' },
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

/** Where the picker stands with Example App chosen. */
function chosen(look: Look, picked: number | null = null) {
  return pickerStep({ locked: false, chosen: 'example', look, picked });
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

describe('the picker', () => {
  test('knows nothing until the first look answers', () => {
    const step = pickerStep({
      locked: false,
      chosen: 'example',
      look: { kind: 'looking', before: null },
      picked: null,
    });
    expect(step).toEqual({ kind: 'looking' });
    expect(captureWindow(step)).toBeNull();
  });

  test('says why when the applications or the windows cannot be read, whatever was found before', () => {
    for (const before of [null, found([read])]) {
      const failed = chosen({ kind: 'failed', error: 'Windows did not answer', before });
      expect(failed).toEqual({ kind: 'failed', error: 'Windows did not answer' });
      expect(captureWindow(failed)).toBeNull();
      expect(stepStatus(failed)).toEqual({
        text: "Couldn't read the saved applications and the open windows: Windows did not answer. Refresh to try again.",
        warn: true,
      });
    }
  });

  test('asks for an application before it looks for a window', () => {
    const before = { locked: false, chosen: null, picked: null };
    const none: Look = { kind: 'seen', seen: { apps: [], windows: [main], antiCheat: [] } };
    expect(pickerStep({ ...before, look: none })).toEqual({ kind: 'no-applications' });
    expect(pickerStep({ ...before, look: answered([read]) })).toEqual({ kind: 'choosing' });
  });

  test('takes the only saved application without asking', () => {
    const step = pickerStep({ locked: false, chosen: null, look: answered([read], []), picked: null });
    expect(step).toEqual({ kind: 'one', app: example, window: main, fallback: false });
  });

  test('says so when the chosen application is not among the saved ones, and offers no choice in its place', () => {
    const look = answered([read]);
    const handed = pickerStep({ locked: false, chosen: 'gone', look, picked: null });
    expect(handed).toEqual({ kind: 'missing', locked: false });
    expect(stepStatus(handed).text).toStartWith("The application chosen here isn't among the saved applications");
    expect(stepStatus(handed).warn).toBe(true);
    const own = pickerStep({ locked: true, chosen: 'gone', look, picked: null });
    expect(own).toEqual({ kind: 'missing', locked: true });
    expect(stepStatus(own).text).toStartWith("The application this belongs to can't be read any more");
    expect(captureWindow(own)).toBeNull();
  });

  test('waits for the look under way, naming the application the last one found', () => {
    expect(chosen({ kind: 'looking', before: found([read]) })).toEqual({ kind: 'resolving', app: example });
  });

  test('captures the one window the application reads without asking', () => {
    const one = chosen(answered([read, worse]));
    expect(one).toEqual({
      kind: 'one',
      app: app('example', 'Example App', [read, worse]),
      window: main,
      fallback: false,
    });
    expect(captureWindow(one)).toEqual(main);
    expect(stepStatus(one)).toEqual({ text: "Example App's window is “Example App - Save 3”.", warn: false });
    expect(offersMatchEdit(one)).toBe(false);
  });

  test('says when the one window is only the fallback', () => {
    const one = chosen(answered([{ ...read, fallback: true }]));
    expect(one.kind === 'one' && one.fallback).toBe(true);
    expect(stepStatus(one).text).toBe(
      "Example App's window is “Example App - Save 3”. It is the fallback: no open window matches everything that must match.",
    );
  });

  test('asks which of the windows that fit equally well, and offers no other', () => {
    const several = chosen(answered([read, twin, worse]));
    expect(several).toMatchObject({
      kind: 'several',
      windows: [main, second],
      picked: null,
      reads: 3,
      fallback: false,
    });
    expect(captureWindow(several)).toBeNull();
    expect(stepStatus(several)).toEqual({
      text: "2 open windows fit Example App equally well, and AppyTappy reads one of them. Pick the one to capture, or change how it's found so that only one fits.",
      warn: false,
    });
    expect(offersMatchEdit(several)).toBe(true);

    expect(captureWindow(chosen(answered([read, twin, worse]), 8))).toEqual(second);
    // A window that fits worse is never read, so it cannot be the one picked.
    expect(captureWindow(chosen(answered([read, twin, worse]), 5))).toBeNull();
  });

  test('marks the window a session would read now among several, and says when they are the fallback', () => {
    const fallbacks = chosen(
      answered([
        { ...twin, best: true, fallback: true },
        { ...read, best: false, fallback: true },
      ]),
    );
    expect(fallbacks).toMatchObject({ kind: 'several', reads: 8, fallback: true });
    expect(stepStatus(fallbacks).text).toEndWith(
      'They are the fallback: no open window matches everything that must match.',
    );
  });

  test('says so when no window fits, and offers no window', () => {
    const none = chosen(answered([]));
    expect(none).toMatchObject({ kind: 'none', unlisted: false });
    expect(captureWindow(none)).toBeNull();
    expect(stepStatus(none).text).toStartWith("Example App isn't open, or its window doesn't match");
    expect(stepStatus(none).warn).toBe(true);
    expect(offersMatchEdit(none)).toBe(true);
  });

  test('offers no window when a session reads one that is not listed', () => {
    const unlisted = chosen(answered([{ ...read, best: false, tied: false }, worse]));
    expect(unlisted).toMatchObject({ kind: 'none', unlisted: true });
    expect(stepStatus(unlisted).text).toStartWith("AppyTappy reads a window of Example App that can't be captured");
  });
});
