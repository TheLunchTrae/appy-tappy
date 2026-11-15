import { describe, expect, test } from 'bun:test';

import type { AppSummary, WindowChoice } from '../src/lib/bindings.ts';
import { exactWindow } from '../src/lib/matching.ts';
import type { PickerApp, PickerStep } from '../src/lib/picker.ts';
import {
  type SearchEvent,
  type WindowCapture,
  NO_CAPTURE,
  searchStep,
  windowAbsence,
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

const starting: WindowCapture = { kind: 'starting', window: main };
const refused: WindowCapture = { kind: 'refused', error: 'That window closed.' };
const running: WindowCapture = { kind: 'running', window: main };

describe('the capture of a saved value’s or reference’s window', () => {
  const cases: readonly (readonly [string, WindowCapture, SearchEvent, WindowCapture])[] = [
    ['the only window that fits is captured without asking', NO_CAPTURE, { kind: 'looked', step: one }, starting],
    ['several that fit wait for a choice', NO_CAPTURE, { kind: 'looked', step: several }, NO_CAPTURE],
    ['an application that is not open waits', NO_CAPTURE, { kind: 'looked', step: closed }, NO_CAPTURE],
    ['a look under way starts nothing', NO_CAPTURE, { kind: 'looked', step: { kind: 'resolving', app } }, NO_CAPTURE],
    [
      'a choice among several is captured',
      NO_CAPTURE,
      { kind: 'chose', window: second },
      { kind: 'starting', window: second },
    ],
    ['a capture that starts runs', starting, { kind: 'started' }, running],
    ['a capture that is refused says why', starting, { kind: 'refused', error: 'That window closed.' }, refused],
    ['a refusal is not asked for again by the look it came from', refused, { kind: 'looked', step: one }, refused],
    ['Refresh clears a refusal, so the next look captures', refused, { kind: 'refreshed' }, NO_CAPTURE],
    [
      'another choice replaces a refusal',
      refused,
      { kind: 'chose', window: second },
      { kind: 'starting', window: second },
    ],
    ['a look changes nothing while a capture starts', starting, { kind: 'looked', step: closed }, starting],
    ['a look changes nothing while the capture runs', running, { kind: 'looked', step: closed }, running],
    ['a choice changes nothing while a capture starts', starting, { kind: 'chose', window: second }, starting],
    ['Refresh keeps a running capture', running, { kind: 'refreshed' }, running],
    ['an answer with no capture starting is dropped', NO_CAPTURE, { kind: 'started' }, NO_CAPTURE],
    ['a refusal with no capture starting is dropped', running, { kind: 'refused', error: 'late' }, running],
    ['a stopped capture starts over', running, { kind: 'stopped' }, NO_CAPTURE],
  ];
  test.each(cases)('%s', (_name, capture, event, next) => {
    expect(searchStep(capture, event)).toEqual(next);
  });
});

describe('what shows in place of the window', () => {
  test('is nothing once the capture runs', () => {
    expect(windowAbsence(closed, running, 'Example App')).toBeNull();
  });

  test('says to open an application that is not open, and offers to change how it is found', () => {
    const absence = windowAbsence(closed, NO_CAPTURE, 'Example App');
    expect(absence?.heading).toBe("Example App isn't open");
    expect(absence?.reason).toBe("Example App isn't open. Open it, then select Refresh.");
    expect(absence).toMatchObject({ busy: false, windows: [], offersMatchEdit: true });
  });

  test('lists the windows that fit equally well, with the one a session would read', () => {
    expect(windowAbsence(several, NO_CAPTURE, 'Example App')).toMatchObject({
      heading: "Pick Example App's window",
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
      expect(windowAbsence(step, capture, 'Example App')).toMatchObject({ busy: true, windows: [] });
    }
  });

  test('shows why a capture was refused, and keeps the windows to pick from', () => {
    expect(windowAbsence(several, refused, 'Example App')).toMatchObject({
      heading: "Couldn't capture Example App's window",
      text: 'That window closed. Pick a window, or select Refresh.',
      busy: false,
      windows: [main, second],
    });
    expect(windowAbsence(one, refused, 'Example App')?.text).toBe('That window closed. Select Refresh to look again.');
  });

  test('says so when the look failed or the application is gone, before its name is known', () => {
    expect(windowAbsence({ kind: 'failed', error: 'no desktop' }, NO_CAPTURE, null)).toMatchObject({
      heading: "Couldn't look for the window",
      busy: false,
      reason: "The application's window isn't captured yet.",
    });
    expect(windowAbsence({ kind: 'missing', locked: true }, NO_CAPTURE, null)).toMatchObject({
      heading: "The application can't be read",
      offersMatchEdit: false,
    });
  });
});
