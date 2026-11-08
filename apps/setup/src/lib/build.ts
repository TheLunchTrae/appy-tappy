// Whether the build of an application that runs now is the one its values were checked against.

import type { AppFile, BuildDef } from './api.ts';

function sameBuild(a: BuildDef, b: BuildDef): boolean {
  return a.version === b.version && a.size === b.size && a.modified === b.modified;
}

/** Whether an application's values need checking against the build that runs. */
export type BuildState =
  /** Another build runs; `change` says how it differs, in a few words. */
  | { readonly kind: 'changed'; readonly change: string }
  /** A build runs, but none was recorded as checked, such as for values made before builds were recorded. */
  | { readonly kind: 'unrecorded' };

/**
 * How the build that runs, `running`, stands against the one `file`'s values were checked against: both versions when
 * they differ, and otherwise that the file does. Null when they match or no build is known to run.
 */
export function buildState(file: AppFile, running: BuildDef | null): BuildState | null {
  if (running === null) {
    return null;
  }
  const checked = file.checked?.build;
  if (checked === undefined) {
    return { kind: 'unrecorded' };
  }
  if (sameBuild(checked, running)) {
    return null;
  }
  if (checked.version !== running.version) {
    return {
      kind: 'changed',
      change: `${checked.version ?? 'no version'} then, ${running.version ?? 'no version'} now`,
    };
  }
  return {
    kind: 'changed',
    change:
      checked.version === undefined
        ? 'another file, with no version either time'
        : `still ${checked.version}, but another file`,
  };
}

/** What the values were checked against and on which day, such as "checked against 1.4.2.0 on Nov 8, 2026". */
export function checkedText(file: AppFile, day: (at: Date) => string): string | null {
  const { checked } = file;
  if (checked === undefined) {
    return null;
  }
  const on = day(new Date(checked.at * 1000));
  return checked.build.version === undefined ? `checked on ${on}` : `checked against ${checked.build.version} on ${on}`;
}
