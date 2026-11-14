// Whether AppyTappy's background part runs: it runs every automation, so the Automations page and the automation
// editor warn while it does not.

/** What the Automations page and the automation editor say while the background part does not run. */
export const RUNTIME_STOPPED =
  "AppyTappy's background part isn't running, so no automation runs. It starts each time you sign in to Windows while Start AppyTappy when I sign in to Windows is on in Settings. To start it now, open appytappy-runtime.exe where AppyTappy is installed, %LOCALAPPDATA%\\AppyTappy unless you picked another folder.";

/** Whether the background part runs after `read`, a read of it: not known when the read failed. */
export function runtimeAfter(read: PromiseSettledResult<boolean | null>): boolean | null {
  return read.status === 'fulfilled' ? read.value : null;
}

/** The warning for `runs`, whether the background part runs or null while that is not known: none unless it does not. */
export function runtimeWarning(runs: boolean | null): string | null {
  return runs === false ? RUNTIME_STOPPED : null;
}
