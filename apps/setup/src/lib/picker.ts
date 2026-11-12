// What the picker of a value or reference shows: the application first, then the window that application reads.

import type { AppSummary, AppWindows, WindowChoice, WindowFit } from './bindings.ts';
import { sentence } from './matching.ts';

/** A saved application a value or reference can belong to, with the open windows its match takes. */
export interface PickerApp {
  readonly id: string;
  readonly name: string;
  /** The executable its window is found by. */
  readonly executable: string;
  readonly fits: readonly WindowFit[];
  /** The application as its editor takes it. */
  readonly summary: AppSummary;
}

/** What one look found: the saved applications whose files read, and the open windows their fits are among. */
export interface Seen {
  readonly apps: readonly PickerApp[];
  readonly windows: readonly WindowChoice[];
  /** The anti-cheat products running now. */
  readonly antiCheat: readonly string[];
}

/** What a look answered, as the picker reads it. An application whose file is broken can own nothing new. */
export function seen(answer: AppWindows): Seen {
  return {
    apps: answer.apps.flatMap(({ app, fits }) =>
      app.file === null
        ? []
        : [{ id: app.id, name: app.file.name, executable: app.file.window.executable, fits, summary: app }],
    ),
    windows: answer.open.windows,
    antiCheat: answer.open.antiCheat,
  };
}

/**
 * The newest look at the saved applications and the open windows: under way, failed, or answered. `before` is
 * what the last look that answered found, which stays on show until the next one answers.
 */
export type Look =
  | { readonly kind: 'looking'; readonly before: Seen | null }
  | { readonly kind: 'failed'; readonly error: string; readonly before: Seen | null }
  | { readonly kind: 'seen'; readonly seen: Seen };

/** What the last look that answered found, or null before any has. */
export function known(look: Look): Seen | null {
  return look.kind === 'seen' ? look.seen : look.before;
}

/**
 * The open windows a session of an application may read, out of the `fits` its match takes: one, so nothing is
 * left to choose; several that fit equally well; or none. `unlisted` says a session reads a window that is not
 * among `windows`, such as one with no title, which cannot be captured.
 */
type Readable =
  | { readonly kind: 'one'; readonly window: WindowChoice; readonly fallback: boolean }
  | {
      readonly kind: 'several';
      readonly windows: readonly WindowChoice[];
      readonly reads: number | null;
      readonly fallback: boolean;
    }
  | { readonly kind: 'none'; readonly unlisted: boolean };

function readable(fits: readonly WindowFit[], windows: readonly WindowChoice[]): Readable {
  // A session reads only a window that fits as well as any other, so a value drawn on a worse fit is never read.
  const tied = fits.flatMap((fit) => {
    const window = fit.tied ? windows.find((each) => each.handle === fit.handle) : undefined;
    return window === undefined ? [] : [{ window, fit }];
  });
  const [first, ...others] = tied;
  if (first === undefined) {
    return { kind: 'none', unlisted: fits.length > 0 };
  }
  // Windows that tie fit alike, so one's fallback is every one's.
  const { fallback } = first.fit;
  if (others.length === 0) {
    return { kind: 'one', window: first.window, fallback };
  }
  return {
    kind: 'several',
    windows: tied.map((each) => each.window),
    reads: tied.find((each) => each.fit.best)?.window.handle ?? null,
    fallback,
  };
}

/** What the picker knows: the look, the application chosen, and the window picked among several. */
interface Picking {
  /** Whether the flow belongs to an application already, so there is none to choose. */
  readonly locked: boolean;
  /** The chosen application's ID: the flow's own, one handed over, or the one the user chose. */
  readonly chosen: string | null;
  readonly look: Look;
  /** The handle of the window the user picked among several. */
  readonly picked: number | null;
}

/** Where the picker stands. Only `one`, and `several` with a window picked, have a window to capture. */
export type PickerStep =
  /** No look has answered yet, so not even the applications are known. */
  | { readonly kind: 'looking' }
  /** The saved applications or the open windows could not be read. */
  | { readonly kind: 'failed'; readonly error: string }
  | { readonly kind: 'no-applications' }
  /** The chosen application is not among the saved ones. `locked` says it is the one the flow belongs to. */
  | { readonly kind: 'missing'; readonly locked: boolean }
  | { readonly kind: 'choosing' }
  | { readonly kind: 'resolving'; readonly app: PickerApp }
  | { readonly kind: 'none'; readonly app: PickerApp; readonly unlisted: boolean }
  | { readonly kind: 'one'; readonly app: PickerApp; readonly window: WindowChoice; readonly fallback: boolean }
  | {
      readonly kind: 'several';
      readonly app: PickerApp;
      readonly windows: readonly WindowChoice[];
      readonly picked: WindowChoice | null;
      /** The handle of the one a session that started now would read. */
      readonly reads: number | null;
      readonly fallback: boolean;
    };

export function pickerStep({ locked, chosen, look, picked }: Picking): PickerStep {
  if (look.kind === 'failed') {
    return { kind: 'failed', error: look.error };
  }
  const found = known(look);
  if (found === null) {
    return { kind: 'looking' };
  }
  const [only, ...others] = found.apps;
  // With one saved application there is nothing to choose between.
  const id = chosen ?? (others.length === 0 ? only?.id : undefined);
  const app = found.apps.find((each) => each.id === id);
  if (app === undefined) {
    // An application handed over or chosen before that the look does not find is said, not swapped for a choice.
    if (chosen !== null) {
      return { kind: 'missing', locked };
    }
    return only === undefined ? { kind: 'no-applications' } : { kind: 'choosing' };
  }
  if (look.kind === 'looking') {
    return { kind: 'resolving', app };
  }
  const window = readable(app.fits, found.windows);
  switch (window.kind) {
    case 'one':
      return { kind: 'one', app, window: window.window, fallback: window.fallback };
    case 'several':
      return {
        kind: 'several',
        app,
        windows: window.windows,
        picked: window.windows.find((each) => each.handle === picked) ?? null,
        reads: window.reads,
        fallback: window.fallback,
      };
    case 'none':
      return { kind: 'none', app, unlisted: window.unlisted };
  }
}

/** The window **Capture this window** captures at `step`, or null while there is none. */
export function captureWindow(step: PickerStep): WindowChoice | null {
  if (step.kind === 'one') {
    return step.window;
  }
  return step.kind === 'several' ? step.picked : null;
}

/** Whether `step` offers to change how its application is found, because no one window can be told from it. */
export function offersMatchEdit(step: PickerStep): boolean {
  return step.kind === 'none' || step.kind === 'several';
}

/** What the picker says of windows taken only as the fallback, after it names them as `these`. */
function fallbackNote(these: string, fallback: boolean): string {
  return fallback ? ` ${these} the fallback: no open window matches everything that must match.` : '';
}

/** What the picker says at `step`, and whether it is a problem that keeps the capture off. */
export function stepStatus(step: PickerStep): { readonly text: string; readonly warn: boolean } {
  switch (step.kind) {
    case 'looking':
      return { text: 'Reading the saved applications and looking for their windows…', warn: false };
    case 'failed':
      return {
        text: `Couldn't read the saved applications and the open windows: ${sentence(step.error)} Refresh to try again.`,
        warn: true,
      };
    case 'no-applications':
      return {
        text: 'Values and references belong to an application, and none is saved yet. Add the application to carry on.',
        warn: false,
      };
    case 'missing':
      return {
        text: step.locked
          ? "The application this belongs to can't be read any more. Cancel, then check the Applications page."
          : "The application chosen here isn't among the saved applications: it was removed, or its file can't be read. Choose another, or add it again.",
        warn: true,
      };
    case 'choosing':
      return { text: 'Choose the application, or add it if it is not in the list.', warn: false };
    case 'resolving':
      return { text: `Looking for ${step.app.name}'s window…`, warn: false };
    case 'none':
      return {
        text: step.unlisted
          ? `AppyTappy reads a window of ${step.app.name} that can't be captured, such as one with no title. Change how it's found so that it finds the window you want.`
          : `${step.app.name} isn't open, or its window doesn't match how ${step.app.name} is found. Open it and refresh, or change how it's found.`,
        warn: true,
      };
    case 'one':
      return {
        text: `${step.app.name}'s window is “${step.window.title}”.${fallbackNote('It is', step.fallback)}`,
        warn: false,
      };
    case 'several':
      return {
        text: `${step.windows.length} open windows fit ${step.app.name} equally well, and AppyTappy reads one of them. Pick the one to capture, or change how it's found so that only one fits.${fallbackNote('They are', step.fallback)}`,
        warn: false,
      };
  }
}

/** Whether `app` runs, which is to say its match takes an open window, or null while a look is under way or failed. */
export function isRunning(look: Look, app: PickerApp): boolean | null {
  return look.kind === 'seen' ? app.fits.length > 0 : null;
}
