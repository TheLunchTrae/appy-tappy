// What the picker of a value or reference shows: the application first, then the window that application reads.

import type { AppSummary, WindowChoice, WindowDef, WindowFit } from './bindings.ts';
import { sentence } from './matching.ts';

/** A saved application a value or reference can belong to. */
export interface PickerApp {
  readonly id: string;
  readonly name: string;
  /** How its window is found. */
  readonly window: WindowDef;
}

/** The saved applications whose files read, in the order they are listed. */
export function pickerApps(apps: readonly AppSummary[]): readonly PickerApp[] {
  return apps.flatMap((app) =>
    app.file === null ? [] : [{ id: app.id, name: app.file.name, window: app.file.window }],
  );
}

/**
 * The application chosen when the picker opens: the one the flow belongs to already, then the one to start with,
 * such as the last choice or an application just added for the flow, then the only one there is.
 */
export function startingApp(saved: readonly PickerApp[], locked: string | null, start: string | null): string | null {
  const [only, ...others] = saved;
  return locked ?? start ?? (only !== undefined && others.length === 0 ? only.id : null);
}

/** One look at the open windows: each of them, and the ones each saved application's match takes, by its ID. */
export type Look =
  | { readonly kind: 'looking' }
  | { readonly kind: 'failed'; readonly error: string }
  | {
      readonly kind: 'found';
      readonly windows: readonly WindowChoice[];
      readonly fits: ReadonlyMap<string, readonly WindowFit[]>;
    };

/**
 * The open windows a session of an application may read, out of the `fits` its match takes: one, so nothing is
 * left to choose; several that fit equally well; or none. `unlisted` says a session reads a window that is not
 * among `windows`, such as one with no title, which cannot be captured.
 */
type Readable =
  | { readonly kind: 'one'; readonly window: WindowChoice }
  | { readonly kind: 'several'; readonly windows: readonly WindowChoice[] }
  | { readonly kind: 'none'; readonly unlisted: boolean };

function readable(fits: readonly WindowFit[], windows: readonly WindowChoice[]): Readable {
  // A session reads only a window that fits as well as any other, so a value drawn on a worse fit is never read.
  const tied = fits.flatMap((fit) => {
    const window = fit.tied ? windows.find((each) => each.handle === fit.handle) : undefined;
    return window === undefined ? [] : [window];
  });
  const [first, ...others] = tied;
  if (first === undefined) {
    return { kind: 'none', unlisted: fits.length > 0 };
  }
  return others.length === 0 ? { kind: 'one', window: first } : { kind: 'several', windows: tied };
}

/** What the picker knows: the applications, the one chosen, the open windows, and the window picked among several. */
export interface Picking {
  readonly saved: readonly PickerApp[];
  /** Whether the flow belongs to an application already, so there is none to choose. */
  readonly locked: boolean;
  /** The chosen application's ID. */
  readonly chosen: string | null;
  readonly look: Look;
  /** The handle of the window the user picked among several. */
  readonly picked: number | null;
}

/** Where the picker stands. Only `one`, and `several` with a window picked, have a window to capture. */
export type PickerStep =
  | { readonly kind: 'no-applications' }
  /** The application the flow belongs to is not among the saved ones. */
  | { readonly kind: 'missing' }
  | { readonly kind: 'choosing' }
  | { readonly kind: 'resolving'; readonly app: PickerApp }
  | { readonly kind: 'failed'; readonly app: PickerApp; readonly error: string }
  | { readonly kind: 'none'; readonly app: PickerApp; readonly unlisted: boolean }
  | { readonly kind: 'one'; readonly app: PickerApp; readonly window: WindowChoice }
  | {
      readonly kind: 'several';
      readonly app: PickerApp;
      readonly windows: readonly WindowChoice[];
      readonly picked: WindowChoice | null;
    };

export function pickerStep({ saved, locked, chosen, look, picked }: Picking): PickerStep {
  const app = saved.find((each) => each.id === chosen);
  if (app === undefined) {
    if (locked) {
      return { kind: 'missing' };
    }
    return saved.length === 0 ? { kind: 'no-applications' } : { kind: 'choosing' };
  }
  if (look.kind === 'failed') {
    return { kind: 'failed', app, error: look.error };
  }
  // A look made before the application was saved has no windows for it yet.
  const fits = look.kind === 'found' ? look.fits.get(app.id) : undefined;
  if (look.kind !== 'found' || fits === undefined) {
    return { kind: 'resolving', app };
  }
  const found = readable(fits, look.windows);
  switch (found.kind) {
    case 'one':
      return { kind: 'one', app, window: found.window };
    case 'several':
      return {
        kind: 'several',
        app,
        windows: found.windows,
        picked: found.windows.find((window) => window.handle === picked) ?? null,
      };
    case 'none':
      return { kind: 'none', app, unlisted: found.unlisted };
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

/** What the picker says at `step`, and whether it is a problem that keeps the capture off. */
export function stepStatus(step: PickerStep): { readonly text: string; readonly warn: boolean } {
  switch (step.kind) {
    case 'no-applications':
      return {
        text: 'Values and references belong to an application, and none is saved yet. Add the application to carry on.',
        warn: false,
      };
    case 'missing':
      return {
        text: "The application this belongs to can't be read any more. Cancel, then check the Applications page.",
        warn: true,
      };
    case 'choosing':
      return { text: 'Choose the application, or add it if it is not in the list.', warn: false };
    case 'resolving':
      return { text: `Looking for ${step.app.name}'s window…`, warn: false };
    case 'failed':
      return {
        text: `Couldn't look for ${step.app.name}'s window: ${sentence(step.error)} Refresh windows to try again.`,
        warn: true,
      };
    case 'none':
      return {
        text: step.unlisted
          ? `AppyTappy reads a window of ${step.app.name} that can't be captured, such as one with no title. Change how it's found so that it finds the window you want.`
          : `${step.app.name} isn't open, or its window doesn't match how ${step.app.name} is found. Open it and refresh the windows, or change how it's found.`,
        warn: true,
      };
    case 'one':
      return { text: `${step.app.name}'s window is “${step.window.title}”.`, warn: false };
    case 'several':
      return {
        text: `${step.windows.length} open windows fit ${step.app.name} equally well, and AppyTappy reads whichever is in front. Pick the one to capture, or change how it's found so that only one fits.`,
        warn: false,
      };
  }
}

/** Whether the application `app` is running, which is to say its match takes an open window, or null until a look says. */
export function isRunning(look: Look, app: string): boolean | null {
  const fits = look.kind === 'found' ? look.fits.get(app) : undefined;
  return fits === undefined ? null : fits.length > 0;
}
