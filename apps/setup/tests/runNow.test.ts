import { describe, expect, test } from 'bun:test';

import {
  NOT_ASKED,
  RUN_ASKED,
  type RunAsk,
  type RunEvent,
  type RunNowFacts,
  runNowView,
  runStep,
} from '../src/lib/runNow.ts';

function after(...events: readonly RunEvent[]): RunAsk {
  return events.reduce(runStep, NOT_ASKED);
}

const ready: RunNowFacts = {
  saved: 'saved',
  on: true,
  runtime: true,
  busy: false,
  app: 'Example App',
  needsFront: false,
  ask: NOT_ASKED,
};

describe('asking to run the actions once', () => {
  test('an ask is under way until the background part heard it or nothing was asked', () => {
    expect(after({ kind: 'pressed' })).toEqual({ kind: 'asking' });
    expect(after({ kind: 'pressed' }, { kind: 'heard' })).toEqual({ kind: 'asked' });
    expect(after({ kind: 'pressed' }, { kind: 'refused', message: 'Nothing ran.' })).toEqual({
      kind: 'failed',
      message: 'Nothing ran.',
    });
    expect(after({ kind: 'pressed' }, { kind: 'heard' }, { kind: 'pressed' })).toEqual({ kind: 'asking' });
  });

  test('an answer with no ask under way changes nothing', () => {
    const failed = after({ kind: 'pressed' }, { kind: 'refused', message: 'Nothing ran.' });
    expect(runStep(NOT_ASKED, { kind: 'heard' })).toEqual(NOT_ASKED);
    expect(runStep(failed, { kind: 'heard' })).toEqual(failed);
    expect(runStep({ kind: 'asked' }, { kind: 'refused', message: 'Late.' })).toEqual({ kind: 'asked' });
  });
});

describe('Run actions now', () => {
  test('can be pressed for a saved automation that is on while the background part runs or may run', () => {
    expect(runNowView(ready)).toEqual({ disabled: false, note: '', warn: false });
    expect(runNowView({ ...ready, runtime: null }).disabled).toBe(false);
  });

  test('waits for a save, since it runs the automation as it is saved', () => {
    const fresh = runNowView({ ...ready, saved: 'new' });
    const changed = runNowView({ ...ready, saved: 'changed', ask: { kind: 'asked' } });
    expect(fresh.disabled).toBe(true);
    expect(fresh.note).toStartWith('Save this automation');
    expect(changed.disabled).toBe(true);
    expect(changed.note).toStartWith('This runs the automation as it is saved, without your changes.');
  });

  test('says why it cannot be pressed, the first reason that holds', () => {
    const off = runNowView({ ...ready, on: false, runtime: false });
    const stopped = runNowView({ ...ready, runtime: false, ask: { kind: 'failed', message: 'Nothing ran.' } });
    const asking = runNowView({ ...ready, ask: { kind: 'asking' } });
    expect(off).toEqual({
      disabled: true,
      note: "This automation is turned off, so AppyTappy's background part does not run it. Turn it on first.",
      warn: false,
    });
    expect(stopped).toEqual({
      disabled: true,
      note: "AppyTappy's background part isn't running, so nothing can run the actions.",
      warn: false,
    });
    expect(asking).toEqual({ disabled: true, note: "Asking AppyTappy's background part…", warn: false });
  });

  test('cannot be pressed during a save', () => {
    expect(runNowView({ ...ready, busy: true }).disabled).toBe(true);
    expect(runNowView({ ...ready, busy: true, ask: { kind: 'failed', message: 'Nothing ran.' } }).disabled).toBe(true);
  });

  test('says what the ask came to, and warns only of an ask that failed', () => {
    expect(runNowView({ ...ready, ask: { kind: 'asked' } })).toEqual({ disabled: false, note: RUN_ASKED, warn: false });
    expect(runNowView({ ...ready, ask: { kind: 'failed', message: 'Nothing ran.' } })).toEqual({
      disabled: false,
      note: 'Nothing ran.',
      warn: true,
    });
  });

  test('says that keys and clicks with the cursor are skipped from here', () => {
    const skipped =
      'Keys are pressed, and clicks with the cursor made, only while Example App is the window in front, so from here they are skipped.';
    expect(runNowView({ ...ready, needsFront: true }).note).toBe(skipped);
    expect(runNowView({ ...ready, needsFront: true, ask: { kind: 'asked' } }).note).toBe(`${RUN_ASKED} ${skipped}`);
    expect(runNowView({ ...ready, needsFront: true, app: null }).note).toContain('only while the application is');
  });
});
