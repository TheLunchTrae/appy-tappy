import { describe, expect, test } from 'bun:test';

import type { AppSummary, WindowChoice } from '../src/lib/bindings.ts';
import { exactWindow } from '../src/lib/matching.ts';
import type { PickerApp, PickerStep } from '../src/lib/picker.ts';
import {
  type SearchEvent,
  type WindowCapture,
  NO_CAPTURE,
  fallbackWarning,
  searchStep,
  windowAbsence,
  windowLanding,
} from '../src/lib/windowSearch.ts';

function open(handle: number, title: string): WindowChoice {
  return { handle, title, class: 'ExampleWindow', executable: 'Example.exe', width: 1280, height: 720 };
}

const main = open(3, 'Example App - Save 3');
const second = open(8, 'Example App - Save 4');

const summary: AppSummary = {
  id: 'example',
  file: {
    format: 1,
    name: 'Example App',
    window: exactWindow({ title: 'Example App', class: 'ExampleWindow', executable: 'Example.exe' }),
    sizeChanges: 'often',
    antiCheat: 'unknown',
    values: [],
    references: [],
  },
  error: null,
  windows: [],
  running: null,
};
const app: PickerApp = { id: 'example', name: 'Example App', executable: 'Example.exe', fits: [], summary };

const one: PickerStep = { kind: 'one', app, window: main, fallback: false };
const several: PickerStep = { kind: 'several', app, windows: [main, second], picked: null, reads: 8, fallback: false };
const closed: PickerStep = { kind: 'none', app, unlisted: false };

const starting: WindowCapture = { kind: 'starting', window: main, fallback: false };
const refused: WindowCapture = { kind: 'refused', error: 'That window closed.' };
const running: WindowCapture = { kind: 'running', window: main, fallback: false };
const fallbackRunning: WindowCapture = { kind: 'running', window: second, fallback: true };

describe('the capture of a saved value’s or reference’s window', () => {
  const cases: readonly (readonly [string, WindowCapture, SearchEvent, WindowCapture])[] = [
    ['the only window that fits is captured without asking', NO_CAPTURE, { kind: 'looked', step: one }, starting],
    ['several that fit wait for a choice', NO_CAPTURE, { kind: 'looked', step: several }, NO_CAPTURE],
    ['an application that is not open waits', NO_CAPTURE, { kind: 'looked', step: closed }, NO_CAPTURE],
    ['a look under way starts nothing', NO_CAPTURE, { kind: 'looked', step: { kind: 'resolving', app } }, NO_CAPTURE],
    [
      'a choice among several is captured',
      NO_CAPTURE,
      { kind: 'chose', window: second, fallback: false },
      { kind: 'starting', window: second, fallback: false },
    ],
    [
      'a window taken as the fallback is captured as one',
      NO_CAPTURE,
      { kind: 'looked', step: { ...one, window: second, fallback: true } },
      { kind: 'starting', window: second, fallback: true },
    ],
    ['a capture that starts runs', starting, { kind: 'started' }, running],
    [
      'a fallback that starts runs as the fallback',
      { kind: 'starting', window: second, fallback: true },
      { kind: 'started' },
      fallbackRunning,
    ],
    ['a capture that is refused says why', starting, { kind: 'refused', error: 'That window closed.' }, refused],
    ['a refusal is not asked for again by the look it came from', refused, { kind: 'looked', step: one }, refused],
    ['Refresh clears a refusal, so the next look captures', refused, { kind: 'refreshed' }, NO_CAPTURE],
    [
      'another choice replaces a refusal',
      refused,
      { kind: 'chose', window: second, fallback: true },
      { kind: 'starting', window: second, fallback: true },
    ],
    ['a look changes nothing while a capture starts', starting, { kind: 'looked', step: closed }, starting],
    ['a look changes nothing while the capture runs', running, { kind: 'looked', step: closed }, running],
    [
      'a choice changes nothing while a capture starts',
      starting,
      { kind: 'chose', window: second, fallback: false },
      starting,
    ],
    ['Refresh keeps a running capture', running, { kind: 'refreshed' }, running],
    ['an answer with no capture starting is dropped', NO_CAPTURE, { kind: 'started' }, NO_CAPTURE],
    ['a refusal with no capture starting is dropped', running, { kind: 'refused', error: 'late' }, running],
    ['a stopped capture starts over', running, { kind: 'stopped' }, NO_CAPTURE],
    ['a capture whose window closed waits for the next look', running, { kind: 'ended', window: 3 }, NO_CAPTURE],
    ['the end of a capture of another window is dropped', running, { kind: 'ended', window: 8 }, running],
    ['the end of a capture that is not running is dropped', starting, { kind: 'ended', window: 3 }, starting],
    ['the look after a window closed captures it again', NO_CAPTURE, { kind: 'looked', step: one }, starting],
  ];
  test.each(cases)('%s', (_name, capture, event, next) => {
    expect(searchStep(capture, event)).toEqual(next);
  });
});

describe('what shows in place of the window', () => {
  test('is nothing once the capture runs', () => {
    expect(windowAbsence(closed, running, 'Example App')).toBeNull();
  });

  test('offers to change how an application that is not open is found, and says why the window waits', () => {
    const absence = windowAbsence(closed, NO_CAPTURE, 'Example App');
    expect(absence).toMatchObject({ busy: false, windows: [], offersMatchEdit: true });
    expect(absence?.reason).not.toBe(windowAbsence(several, refused, 'Example App')?.reason);
  });

  test('lists the windows that fit equally well, with the one a session would read', () => {
    expect(windowAbsence(several, NO_CAPTURE, 'Example App')).toMatchObject({
      busy: false,
      windows: [main, second],
      reads: 8,
      offersMatchEdit: true,
    });
  });

  test('waits while it looks, while the only window is about to be captured, and while the capture starts', () => {
    const looking: PickerStep = { kind: 'looking' };
    for (const [step, capture] of [
      [looking, NO_CAPTURE],
      [one, NO_CAPTURE],
      [several, starting],
    ] as const) {
      expect(windowAbsence(step, capture, 'Example App')).toMatchObject({
        busy: true,
        windows: [],
        offersMatchEdit: false,
      });
    }
  });

  test('shows why a capture was refused, and keeps the windows to pick from', () => {
    const absence = windowAbsence(several, refused, 'Example App');
    expect(absence).toMatchObject({ busy: false, windows: [main, second], offersMatchEdit: true });
    expect(absence?.text).toContain('That window closed.');
  });

  test('stays usable when the look failed or the application is gone, before its name is known', () => {
    expect(windowAbsence({ kind: 'failed', error: 'no desktop' }, NO_CAPTURE, null)).toMatchObject({
      busy: false,
      windows: [],
      offersMatchEdit: false,
    });
    expect(windowAbsence({ kind: 'missing', locked: true }, NO_CAPTURE, null)).toMatchObject({
      busy: false,
      windows: [],
      offersMatchEdit: false,
    });
  });
});

describe('where the focus lands when the editor of a saved value or reference shows again', () => {
  test('is the way forward while there is no window, and taking a screenshot once there is one', () => {
    expect(windowLanding(windowAbsence(closed, NO_CAPTURE, 'Example App'))).toBe('match');
    expect(windowLanding(windowAbsence(several, NO_CAPTURE, 'Example App'))).toBe('windows');
    expect(windowLanding(windowAbsence({ kind: 'failed', error: 'no desktop' }, NO_CAPTURE, null))).toBe('refresh');
    expect(windowLanding(windowAbsence(closed, running, 'Example App'))).toBe('screenshot');
  });

  test('waits for a look or a capture that is under way', () => {
    expect(windowLanding(windowAbsence(one, NO_CAPTURE, 'Example App'))).toBeNull();
    expect(windowLanding(windowAbsence(several, starting, 'Example App'))).toBeNull();
  });
});

test('only a running capture of the fallback window is said to be the fallback', () => {
  expect(fallbackWarning(fallbackRunning, 'Example App')).toContain(second.title);
  expect(fallbackWarning(running, 'Example App')).toBeNull();
  expect(fallbackWarning({ kind: 'starting', window: second, fallback: true }, 'Example App')).toBeNull();
});
