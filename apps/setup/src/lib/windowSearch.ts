// How a value's or reference's capture screen gets its application's window: it opens without one, looks for the
// window, and captures it once it is found.

import type { WindowChoice } from './bindings.ts';
import { type PickerStep, fallbackNote, lookFailedText, offersMatchEdit } from './picker.ts';

/**
 * The capture of the application's window: not asked for, starting, refused with the reason, or running. `fallback`
 * says whether the window was taken only as the fallback, because none matched everything that must match.
 */
export type WindowCapture =
  | { readonly kind: 'none' }
  | { readonly kind: 'starting'; readonly window: WindowChoice; readonly fallback: boolean }
  | { readonly kind: 'refused'; readonly error: string }
  | { readonly kind: 'running'; readonly window: WindowChoice; readonly fallback: boolean };

export const NO_CAPTURE: WindowCapture = { kind: 'none' };

export type SearchEvent =
  /** A look answered, or one started: where the picker's steps stand for the flow's application. */
  | { readonly kind: 'looked'; readonly step: PickerStep }
  /** The user picked `window` among several that fit equally well, as the fallback or not. */
  | { readonly kind: 'chose'; readonly window: WindowChoice; readonly fallback: boolean }
  /** The capture that was starting runs. */
  | { readonly kind: 'started' }
  /** The capture that was starting was refused. */
  | { readonly kind: 'refused'; readonly error: string }
  /** The capture of the window with the handle `window` ended because that window closed. */
  | { readonly kind: 'ended'; readonly window: number }
  /** The user asked to look again. */
  | { readonly kind: 'refreshed' }
  /** The flow stopped its capture to start over. */
  | { readonly kind: 'stopped' };

/**
 * The capture after `event`. The only window that fits is captured without asking, and one of several once the
 * user picks it. A refusal stays until the user looks again or picks a window, so a window that cannot be captured
 * is not asked for over and over. A capture whose window closed is asked for again like one that never ran. An
 * answer that arrives while no capture is starting is dropped, and so is the end of a capture of another window
 * than the one that runs.
 */
export function searchStep(capture: WindowCapture, event: SearchEvent): WindowCapture {
  switch (event.kind) {
    case 'looked':
      return capture.kind === 'none' && event.step.kind === 'one'
        ? { kind: 'starting', window: event.step.window, fallback: event.step.fallback }
        : capture;
    case 'chose':
      return capture.kind === 'none' || capture.kind === 'refused'
        ? { kind: 'starting', window: event.window, fallback: event.fallback }
        : capture;
    case 'started':
      return capture.kind === 'starting' ? { ...capture, kind: 'running' } : capture;
    case 'refused':
      return capture.kind === 'starting' ? { kind: 'refused', error: event.error } : capture;
    case 'ended':
      return capture.kind === 'running' && capture.window.handle === event.window ? NO_CAPTURE : capture;
    case 'refreshed':
      return capture.kind === 'refused' ? NO_CAPTURE : capture;
    case 'stopped':
      return NO_CAPTURE;
  }
}

/** What the capture screen shows in place of the window while it has none. */
export interface WindowAbsence {
  readonly heading: string;
  readonly text: string;
  /** Whether a look or a capture's start is under way, which Refresh and the windows wait for. */
  readonly busy: boolean;
  /** The windows to pick from, when several fit equally well, and the one a session that started now would read. */
  readonly windows: readonly WindowChoice[];
  readonly reads: number | null;
  /** Whether changing how the application is found is a way forward. */
  readonly offersMatchEdit: boolean;
  /** Why what needs the window waits, for the controls it turns off. */
  readonly reason: string;
}

/**
 * What shows while the window of the application called `name` is not captured, or null once it is. `name` is null
 * until a look has read the application.
 */
export function windowAbsence(step: PickerStep, capture: WindowCapture, name: string | null): WindowAbsence | null {
  if (capture.kind === 'running') {
    return null;
  }
  const app = name ?? 'The application';
  const uncaptured = `${app}'s window isn't captured yet.`;
  const waiting = { busy: true, windows: [], reads: null, offersMatchEdit: false, reason: uncaptured };
  if (capture.kind === 'starting') {
    return { ...waiting, heading: `Capturing ${app}'s window…`, text: `“${capture.window.title}”` };
  }
  const windows = step.kind === 'several' ? step.windows : [];
  const still = {
    busy: false,
    windows,
    reads: step.kind === 'several' ? step.reads : null,
    offersMatchEdit: offersMatchEdit(step),
    reason: uncaptured,
  };
  if (capture.kind === 'refused') {
    const next = windows.length === 0 ? 'Select Refresh to look again.' : 'Pick a window, or select Refresh.';
    return { ...still, heading: `Couldn't capture ${app}'s window`, text: `${capture.error} ${next}` };
  }
  switch (step.kind) {
    // The only window that fits is captured as soon as the look that found it is heard.
    case 'one':
    case 'looking':
    case 'resolving':
      return { ...waiting, heading: `Looking for ${app}'s window…`, text: 'It is captured as soon as it is found.' };
    case 'none':
      return step.unlisted
        ? {
            ...still,
            heading: `${app}'s window can't be captured`,
            text: `AppyTappy reads a window of ${app} that can't be captured, such as one with no title. Change how it's found so that it finds the window you want.`,
          }
        : {
            ...still,
            heading: `${app} isn't open`,
            text: `Open ${app}. Its window is captured as soon as it opens. Until then, you can change everything here that doesn't need its window. If ${app} is open, its window doesn't match how it's found.`,
            reason: `${app} isn't open. Its window is captured as soon as it opens.`,
          };
    case 'several':
      return {
        ...still,
        heading: `Pick ${app}'s window`,
        text: `${step.windows.length} open windows fit ${app} equally well, and AppyTappy reads one of them. Pick the one to capture, or change how it's found so that only one fits.${fallbackNote('They are', step.fallback)}`,
        reason: `Pick ${app}'s window first.`,
      };
    case 'failed':
      return { ...still, heading: "Couldn't look for the window", text: lookFailedText(step.error) };
    case 'missing':
      return {
        ...still,
        heading: "The application can't be read",
        text: "The application this belongs to can't be read anymore. Discard, then check the Applications page.",
      };
  }
}

/** Whether the look for the window goes on by itself: while the application isn't open and nothing is captured. */
export function waitsForOpen(step: PickerStep, capture: WindowCapture): boolean {
  return capture.kind === 'none' && step.kind === 'none' && !step.unlisted;
}

/** The control the focus lands on when the editor shows again after another flow ended. */
export type WindowControl = 'screenshot' | 'windows' | 'match' | 'refresh';

/**
 * Where the focus lands for `absence`: on the way forward, as the picker's landing chooses it, and on taking a
 * screenshot once the window is captured. Null while a look or a capture's start is under way, which the landing
 * waits for.
 */
export function windowLanding(absence: WindowAbsence | null): WindowControl | null {
  if (absence === null) {
    return 'screenshot';
  }
  if (absence.busy) {
    return null;
  }
  if (absence.windows.length > 0) {
    return 'windows';
  }
  return absence.offersMatchEdit ? 'match' : 'refresh';
}

/**
 * What the capture screen says of a captured window that was taken only as the fallback, for the application
 * called `name`, or null for any other capture.
 */
export function fallbackWarning(capture: WindowCapture, name: string): string | null {
  return capture.kind === 'running' && capture.fallback
    ? `${name}'s window is “${capture.window.title}”.${fallbackNote('It is', true)}`
    : null;
}

/** How long after one ask whether the capture ended the next one waits, so a view that fails with every frame asks seldom. */
export const ENDED_ASK_PAUSE_MS = 1000;

/**
 * Asks whether the window's capture ended, with `asks`, each time something suggests the window closed, and calls
 * `ended` when it did. One ask runs at a time, and the next starts no sooner than `pauseMs` after the last one
 * started, as `now` counts them: a live view that fails without ending the capture fails again with every frame. A
 * sign that arrives meanwhile is not lost: `later` runs one ask for it once the pause is over, with the newest
 * `ended`.
 */
export function endedAsks(
  asks: () => Promise<boolean>,
  pauseMs: number,
  now: () => number,
  later: (run: () => Promise<void>, afterMs: number) => void,
): (ended: () => void) => Promise<void> {
  let asking = false;
  let asked: number | null = null;
  let waiting: (() => void) | null = null;
  const sign = async (ended: () => void): Promise<void> => {
    const wait = asked === null ? 0 : asked + pauseMs - now();
    if (asking || wait > 0) {
      const scheduled = waiting !== null;
      waiting = ended;
      if (!scheduled) {
        later(
          async () => {
            const newest = waiting;
            waiting = null;
            if (newest !== null) {
              await sign(newest);
            }
          },
          // An ask that outlasts its pause is waited for one pause more.
          wait > 0 ? wait : pauseMs,
        );
      }
      return;
    }
    asking = true;
    asked = now();
    try {
      if (await asks()) {
        ended();
      }
    } finally {
      asking = false;
    }
  };
  return sign;
}

/**
 * Whether the capture screen shows a sign that its window may have closed which it did not show `before`: a
 * screenshot that was refused, or a recording that stopped or could not start, each with its reason or null.
 */
export function showsNewCloseSign(before: readonly (string | null)[], signs: readonly (string | null)[]): boolean {
  return signs.some((sign, index) => sign !== null && sign !== before[index]);
}
