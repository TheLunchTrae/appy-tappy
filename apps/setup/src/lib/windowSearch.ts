// How a flow that changes a saved value or reference gets its application's window: it opens without one, looks for
// the window, and captures it once it is found.

import type { WindowChoice } from './bindings.ts';
import { type PickerStep, offersMatchEdit, stepStatus } from './picker.ts';

/** The capture of the application's window: not asked for, starting, refused with the reason, or running. */
export type WindowCapture =
  | { readonly kind: 'none' }
  | { readonly kind: 'starting'; readonly window: WindowChoice }
  | { readonly kind: 'refused'; readonly error: string }
  | { readonly kind: 'running'; readonly window: WindowChoice };

export const NO_CAPTURE: WindowCapture = { kind: 'none' };

export type SearchEvent =
  /** A look answered, or one started: where the picker's steps stand for the flow's application. */
  | { readonly kind: 'looked'; readonly step: PickerStep }
  /** The user picked `window` among several that fit equally well. */
  | { readonly kind: 'chose'; readonly window: WindowChoice }
  /** The capture that was starting runs. */
  | { readonly kind: 'started' }
  /** The capture that was starting was refused. */
  | { readonly kind: 'refused'; readonly error: string }
  /** The user asked to look again. */
  | { readonly kind: 'refreshed' }
  /** The flow stopped its capture to start over. */
  | { readonly kind: 'stopped' };

/**
 * The capture after `event`. The only window that fits is captured without asking, and one of several once the
 * user picks it. A refusal stays until the user looks again or picks a window, so a window that cannot be captured
 * is not asked for over and over. An answer that arrives while no capture is starting is dropped.
 */
export function searchStep(capture: WindowCapture, event: SearchEvent): WindowCapture {
  switch (event.kind) {
    case 'looked':
      return capture.kind === 'none' && event.step.kind === 'one'
        ? { kind: 'starting', window: event.step.window }
        : capture;
    case 'chose':
      return capture.kind === 'none' || capture.kind === 'refused'
        ? { kind: 'starting', window: event.window }
        : capture;
    case 'started':
      return capture.kind === 'starting' ? { kind: 'running', window: capture.window } : capture;
    case 'refused':
      return capture.kind === 'starting' ? { kind: 'refused', error: event.error } : capture;
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
        ? { ...still, heading: `${app}'s window can't be captured`, text: stepStatus(step).text }
        : {
            ...still,
            heading: `${app} isn't open`,
            text: `Open ${app}, then select Refresh. Until then, you can change everything here that doesn't need its window. If ${app} is open, its window doesn't match how it's found.`,
            reason: `${app} isn't open. Open it, then select Refresh.`,
          };
    case 'several':
      return {
        ...still,
        heading: `Pick ${app}'s window`,
        text: stepStatus(step).text,
        reason: `Pick ${app}'s window first.`,
      };
    case 'failed':
      return { ...still, heading: "Couldn't look for the window", text: stepStatus(step).text };
    // A flow that has its application never chooses one, so any step without it is an application that is gone.
    case 'missing':
    case 'no-applications':
    case 'choosing':
      return {
        ...still,
        heading: "The application can't be read",
        text: "The application this belongs to can't be read any more. Discard, then check the Applications page.",
      };
  }
}
