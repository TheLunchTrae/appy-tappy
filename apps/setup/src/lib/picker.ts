// The saved applications and the open windows, the application a new value or reference belongs to, and the
// window of that application a capture takes.

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

/** Where the look for one saved application's window stands. Only `one` has a window to capture at once. */
export type PickerStep =
  /** No look has answered yet, so not even the applications are known. */
  | { readonly kind: 'looking' }
  /** The saved applications or the open windows could not be read. */
  | { readonly kind: 'failed'; readonly error: string }
  /** The application is not among the saved ones: it was removed, or its file can't be read. */
  | { readonly kind: 'missing' }
  | { readonly kind: 'resolving'; readonly app: PickerApp }
  | { readonly kind: 'none'; readonly app: PickerApp; readonly unlisted: boolean }
  | { readonly kind: 'one'; readonly app: PickerApp; readonly window: WindowChoice; readonly fallback: boolean }
  | {
      readonly kind: 'several';
      readonly app: PickerApp;
      readonly windows: readonly WindowChoice[];
      /** The handle of the one a session that started now would read. */
      readonly reads: number | null;
      readonly fallback: boolean;
    };

/** Where the look for the window of the saved application `app` stands at `look`. */
export function pickerStep(app: string, look: Look): PickerStep {
  if (look.kind === 'failed') {
    return { kind: 'failed', error: look.error };
  }
  const found = known(look)?.apps.find((each) => each.id === app);
  if (found === undefined) {
    return known(look) === null ? { kind: 'looking' } : { kind: 'missing' };
  }
  if (look.kind === 'looking') {
    return { kind: 'resolving', app: found };
  }
  const window = readable(found.fits, known(look)?.windows ?? []);
  switch (window.kind) {
    case 'one':
      return { kind: 'one', app: found, window: window.window, fallback: window.fallback };
    case 'several':
      return { kind: 'several', app: found, windows: window.windows, reads: window.reads, fallback: window.fallback };
    case 'none':
      return { kind: 'none', app: found, unlisted: window.unlisted };
  }
}

/** Whether `step` offers to change how its application is found, because no one window can be told from it. */
export function offersMatchEdit(step: PickerStep): boolean {
  return step.kind === 'none' || step.kind === 'several';
}

/** What is said of windows taken only as the fallback, after they are named as `these`. */
export function fallbackNote(these: string, fallback: boolean): string {
  return fallback ? ` ${these} the fallback: no open window matches everything that must match.` : '';
}

/** Why the saved applications and the open windows could not be read. */
export function lookFailedText(error: string): string {
  return `Couldn't read the saved applications and the open windows: ${sentence(error)} Refresh to try again.`;
}

/**
 * Where choosing the application of a new value or reference stands: before any look answered, a failed look,
 * no saved application, several to choose from, or the only one, which needs no choice.
 */
export type ChooserStep =
  | { readonly kind: 'looking' }
  | { readonly kind: 'failed'; readonly error: string }
  | { readonly kind: 'no-applications' }
  | { readonly kind: 'choosing'; readonly apps: readonly PickerApp[] }
  | { readonly kind: 'only'; readonly app: PickerApp };

export function chooserStep(look: Look): ChooserStep {
  if (look.kind === 'failed') {
    return { kind: 'failed', error: look.error };
  }
  const found = known(look);
  if (found === null) {
    return { kind: 'looking' };
  }
  const [only, ...others] = found.apps;
  if (only === undefined) {
    return { kind: 'no-applications' };
  }
  return others.length === 0 ? { kind: 'only', app: only } : { kind: 'choosing', apps: found.apps };
}

/** Whether `app` runs, which is to say its match takes an open window, or null while a look is under way or failed. */
export function isRunning(look: Look, app: PickerApp): boolean | null {
  return look.kind === 'seen' ? app.fits.length > 0 : null;
}
