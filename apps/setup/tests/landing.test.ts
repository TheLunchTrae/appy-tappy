import { describe, expect, test } from 'bun:test';

import type { ValueChoice } from '../src/lib/automation.ts';
import { type PickerControl, type WatchControl, editorLanding, pickerLanding } from '../src/lib/landing.ts';
import type { PickerApp, PickerStep } from '../src/lib/picker.ts';
import type { WatchStep } from '../src/lib/watch.ts';

describe('where the focus lands when the automation editor shows again', () => {
  const app = { id: 'example', name: 'Example App' };

  const choice: ValueChoice = {
    app: 'example',
    appName: 'Example App',
    antiCheat: 'none',
    value: {
      id: 'progress',
      name: 'Progress',
      screen: {
        windowSize: { width: 100, height: 50 },
        region: { x: 0, y: 0, width: 10, height: 10 },
        check: { kind: 'number', reference: 'percent', whole: false },
        fixes: [],
      },
    },
    kind: 'number',
    states: [],
  };

  test('is the control the next step is taken with', () => {
    const steps: readonly (readonly [WatchStep, WatchControl])[] = [
      [{ kind: 'ready', app, values: [choice], choice }, 'comparison'],
      [{ kind: 'no-applications', unreadable: false }, 'add-application'],
      [{ kind: 'choosing' }, 'application'],
      [{ kind: 'missing', id: 'example', added: true }, 'application'],
      [{ kind: 'value', app, lost: false }, 'new-value'],
      [{ kind: 'pick', app, values: [], lost: false }, 'value'],
    ];
    for (const [step, control] of steps) {
      expect(editorLanding(step)).toBe(control);
    }
  });

  test('waits while what a flow saved is still read', () => {
    expect(editorLanding({ kind: 'arriving', id: 'example', error: null })).toBeNull();
  });
});

describe('where the focus lands when the picker shows after another flow', () => {
  const window = {
    handle: 3,
    title: 'Example App',
    class: 'ExampleWindow',
    executable: 'Example.exe',
    width: 1280,
    height: 720,
  };
  const app: PickerApp = {
    id: 'example',
    name: 'Example App',
    executable: 'Example.exe',
    fits: [],
    summary: { id: 'example', file: null, error: null, windows: [], running: null },
  };

  test('is the capture once there is a window to capture', () => {
    expect(pickerLanding({ kind: 'one', app, window, fallback: false })).toBe('capture');
    const several = { kind: 'several', app, windows: [window], reads: null, fallback: false } as const;
    expect(pickerLanding({ ...several, picked: window })).toBe('capture');
    expect(pickerLanding({ ...several, picked: null })).toBe('windows');
  });

  test('is the way forward when there is none', () => {
    const steps: readonly (readonly [PickerStep, PickerControl])[] = [
      [{ kind: 'none', app, unlisted: false }, 'match'],
      [{ kind: 'missing', locked: false }, 'refresh'],
      [{ kind: 'failed', error: 'access is denied' }, 'refresh'],
      [{ kind: 'choosing' }, 'add-application'],
      [{ kind: 'no-applications' }, 'add-application'],
    ];
    for (const [step, control] of steps) {
      expect(pickerLanding(step)).toBe(control);
    }
  });

  test('waits while the look is under way', () => {
    expect(pickerLanding({ kind: 'looking' })).toBeNull();
    expect(pickerLanding({ kind: 'resolving', app })).toBeNull();
  });
});
