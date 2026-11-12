// How an application's window match is edited and described.

import type { FallbackDef, WindowChoice, WindowDef, WindowFit, WindowScore } from './bindings.ts';

/** One of the things an application's window is recognized by. */
export type MatchField = 'title' | 'class' | 'executable';

/** A field that may be allowed to differ; the executable always has to match. */
export type LooseField = Exclude<MatchField, 'executable'>;

/** The fields in the order the editor shows them. */
export const FIELDS: readonly MatchField[] = ['title', 'class', 'executable'];

/** Each field's name. */
export const FIELD_NAMES: Readonly<Record<MatchField, string>> = {
  title: 'Window title',
  class: 'Window class',
  executable: 'Executable',
};

/** What each field is, for someone who has never heard of it, with an example. */
export const FIELD_HINTS: Readonly<Record<MatchField, { readonly text: string; readonly example: string }>> = {
  title: {
    text: "The name in the window's title bar. Many apps change it as they run, such as to show a song or file name, so let it differ or use * for the part that changes.",
    example: 'Example App - Save 3',
  },
  class: {
    text: 'A hidden type name Windows gives every kind of window; you never see it on screen. It stays the same while the app runs, but apps built with the same toolkit can share one, so on its own it may not tell two apps apart.',
    example: 'ExampleWindow',
  },
  executable: {
    text: 'The program file the window belongs to. It always has to match, because AppyTappy notices the application starting and closing by this program.',
    example: 'Example.exe',
  },
};

/** What the Must match checkboxes do. */
export const MUST_MATCH_HINT =
  'Ticked: a window has to match this to count. Not ticked: it may differ, but a window that does match it still wins over one that does not.';

/** Why the executable's Must match cannot be unticked. */
export const EXECUTABLE_NOTE = 'The executable always has to match: AppyTappy finds the application by its program.';

/** What the fallback menu does. */
export const FALLBACK_HINT =
  'What AppyTappy uses when the program is running but none of its windows matches everything you ticked. None treats the application as closed until one does.';

/** Each fallback, as the menu says it. */
export const FALLBACK_NAMES: Readonly<Record<FallbackDef, string>> = {
  none: 'None (treat it as closed)',
  executable: 'Any window of the same executable',
};

/** The fallbacks in the order the menu shows them. */
export const FALLBACKS: readonly FallbackDef[] = ['none', 'executable'];

/** A match for the picked window that requires only its executable, which suits apps whose title changes. */
export function exactWindow(window: Pick<WindowChoice, MatchField>): WindowDef {
  return {
    title: window.title,
    class: window.class,
    executable: window.executable,
    mustMatch: { title: false, class: false },
    fallback: 'none',
  };
}

/** Whether `window` requires `field`; the executable always. */
export function requires(window: WindowDef, field: MatchField): boolean {
  return field === 'executable' || window.mustMatch[field];
}

/** The fields `window` requires, in the editor's order. */
export function required(window: WindowDef): readonly MatchField[] {
  return FIELDS.filter((field) => requires(window, field));
}

/**
 * Whether a fallback could take a window that does not qualify. With only the executable required, every window
 * of the executable qualifies already, so the editor hides the fallback.
 */
export function fallbackUseful(window: WindowDef): boolean {
  return window.mustMatch.title || window.mustMatch.class;
}

/** `window` with `field` required or not; a fallback that could take nothing more goes back to none. */
export function withMustMatch(window: WindowDef, field: LooseField, must: boolean): WindowDef {
  const changed = { ...window, mustMatch: { ...window.mustMatch, [field]: must } };
  return fallbackUseful(changed) ? changed : { ...changed, fallback: 'none' };
}

/** How `window` finds its window, in two short lines for the Applications page. */
export function matchSummary(window: WindowDef): { readonly must: string; readonly then: string | null } {
  const part = (field: MatchField): string =>
    `${FIELD_NAMES[field].replace('Window ', '').toLowerCase()} ${window[field]}`;
  const must = `Must match ${required(window).map(part).join(', ')}`;
  const preferred = FIELDS.filter((field) => !requires(window, field));
  const prefers = preferred.length === 0 ? null : `prefers ${preferred.map(part).join(', ')}`;
  const otherwise = window.fallback === 'none' ? null : 'otherwise any window of that executable';
  const then = [prefers, otherwise].filter((line) => line !== null).join('; ');
  return { must, then: then.length === 0 ? null : then.charAt(0).toUpperCase() + then.slice(1) };
}

/** What the window list says about a window an application's match takes, or null for one it does not. */
export function fitLabel(
  fits: readonly WindowFit[] | null,
  handle: number,
): { readonly text: string; readonly className: string } | null {
  const fit = fits?.find((each) => each.handle === handle);
  if (fit === undefined) {
    return null;
  }
  if (fit.best) {
    return {
      text: fit.fallback ? 'best match, fallback' : 'best match',
      className: fit.fallback ? 'fit fallback' : 'ok',
    };
  }
  return fit.fallback ? { text: 'fallback', className: 'fit fallback' } : { text: 'matches', className: 'note' };
}

/** The parts of a window match that say why it leaves a window out. */
export type MatchRules = Pick<WindowDef, 'mustMatch' | 'fallback'>;

/**
 * Why the match leaves out a window that `score` describes. A window of the executable that matches every field
 * ticked is always taken, so one left out either has another executable or differs in a field ticked. With a fallback,
 * such a window would be taken as the fallback, but a window that qualifies takes its place.
 */
function leftOut(score: WindowScore, rules: MatchRules): string {
  if (!score.executable) {
    return 'its executable differs';
  }
  const field = rules.mustMatch.title && !score.title ? 'title' : 'class';
  return rules.fallback === 'none'
    ? `its ${field} must match`
    : `its ${field} must match, and a window that does is open, so the fallback is not used`;
}

/** For the debug details: which of a window's fields match, and why the match leaves it out if it does. */
export function scoreText(score: WindowScore, rules: MatchRules): string {
  const word = (matches: boolean): string => (matches ? 'matches' : 'differs');
  const parts = `Title ${word(score.title)}, class ${word(score.class)}, executable ${word(score.executable)}.`;
  return score.fit === null ? `${parts} Not taken: ${leftOut(score, rules)}.` : parts;
}

/** How many open windows qualify, and which one AppyTappy would use. */
export function fitCount(fits: readonly WindowFit[] | null): string {
  if (fits === null) {
    return 'Looking for matching windows…';
  }
  const qualifying = fits.filter((fit) => !fit.fallback).length;
  if (qualifying === 0) {
    return fits.length === 0
      ? 'No open window qualifies, so AppyTappy treats the application as closed.'
      : `No open window qualifies, so AppyTappy uses the fallback: ${fits.length === 1 ? '1 window' : `${fits.length} windows`}.`;
  }
  if (qualifying === 1) {
    return '1 open window qualifies.';
  }
  return `${qualifying} open windows qualify. AppyTappy uses the one that matches the most fields.`;
}

/** `text` ending in a full stop, so another sentence can follow it. */
export function sentence(text: string): string {
  const trimmed = text.trim();
  return /[.!?]$/.test(trimmed) ? trimmed : `${trimmed}.`;
}

/**
 * Whether the application `name` reads the picked window `handle`, as the value picker says it. A new session reads
 * only the window marked best, so a value drawn on any other window would never be read. `titleOf` names an open
 * window.
 */
export function pickedStatus(
  name: string,
  handle: number,
  fits: readonly WindowFit[] | null,
  error: string | null,
  titleOf: (handle: number) => string | null,
): { readonly text: string; readonly ok: boolean } {
  if (error !== null) {
    return {
      text: `Couldn't check whether ${name} finds this window: ${sentence(error)} Refresh windows to try again.`,
      ok: false,
    };
  }
  if (fits === null) {
    return { text: `Checking whether ${name} finds this window…`, ok: false };
  }
  const read = fits.find((fit) => fit.best);
  if (read?.handle === handle) {
    return { text: `${name} reads this window.`, ok: true };
  }
  const change = `change how ${name} is found on the Applications page`;
  if (fits.length === 0) {
    return {
      text: `${name} doesn't find this window, so the value would never be read. Pick another window, or ${change}.`,
      ok: false,
    };
  }
  const other = read === undefined ? null : titleOf(read.handle);
  return {
    text: `${name} reads ${other === null ? 'another window' : `“${other}”`} instead, so the value would never be read here. Pick that window, or ${change}.`,
    ok: false,
  };
}
