// Whether the build of an application that runs now is the one its values were checked against.

import type { AppFile, BuildDef } from './api.ts';

function sameBuild(a: BuildDef, b: BuildDef): boolean {
  return a.version === b.version && a.size === b.size && a.modified === b.modified;
}

/**
 * How the build that runs, `running`, differs from the one `file`'s values were checked against, in a few words: both
 * versions when they differ, and otherwise that the file does. Null when the builds match, nothing was checked yet, or
 * no build is known to run.
 */
export function buildChange(file: AppFile, running: BuildDef | null): string | null {
  const checked = file.checked?.build;
  if (checked === undefined || running === null || sameBuild(checked, running)) {
    return null;
  }
  if (checked.version !== running.version) {
    return `${checked.version ?? 'no version'} then, ${running.version ?? 'no version'} now`;
  }
  return checked.version === undefined
    ? 'another file, with no version either time'
    : `still ${checked.version}, but another file`;
}
