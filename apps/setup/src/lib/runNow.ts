// Run actions now: when the automation editor can ask the background part to run the saved automation's actions once,
// and what it says about the ask. The background part runs them, never AppyTappy Setup, and its answer does not
// come back here: its log has it.

/** An ask to run the actions once: none yet, one under way, or how the newest ended. */
export type RunAsk =
  | { readonly kind: 'idle' }
  | { readonly kind: 'asking' }
  /** The background part heard the ask. Whether the actions ran is in its log. */
  | { readonly kind: 'asked' }
  /** Nothing was asked, and `message` says why. */
  | { readonly kind: 'failed'; readonly message: string };

/** What moves an ask on. */
export type RunEvent =
  | { readonly kind: 'pressed' }
  | { readonly kind: 'heard' }
  | { readonly kind: 'refused'; readonly message: string };

export const NOT_ASKED: RunAsk = { kind: 'idle' };

/** The ask after `event`. One ask is under way at a time, and only it can end. */
export function runStep(ask: RunAsk, event: RunEvent): RunAsk {
  switch (event.kind) {
    case 'pressed':
      return { kind: 'asking' };
    case 'heard':
      return ask.kind === 'asking' ? { kind: 'asked' } : ask;
    case 'refused':
      return ask.kind === 'asking' ? { kind: 'failed', message: event.message } : ask;
  }
}

/** What Run actions now does, for its ?. */
export const RUN_NOW_HINT =
  "Asks AppyTappy's background part to run this automation's actions once, as if the condition had just become true. It runs them as they are saved, only while the application is open and automations are enabled in the tray icon's menu, and never undoes them, whatever Afterwards says. The wait before it can run again does not start.";

/** What the editor says once the background part heard an ask. */
export const RUN_ASKED =
  "Asked AppyTappy's background part to run the actions. If nothing happens, its log says why: runtime.log in %LOCALAPPDATA%\\AppyTappy\\logs, which the Debug page shows too.";

/** What the editor knows when it shows Run actions now. */
export interface RunNowFacts {
  /**
   * How what the editor shows compares with the saved automation: `new` before it was ever saved, `changed` while
   * the editor or a draft of it holds a change, and `saved` when the two are the same.
   */
  readonly saved: 'new' | 'changed' | 'saved';
  /** Whether the saved automation is turned on. */
  readonly on: boolean;
  /** Whether the background part runs, or null while that is not known. */
  readonly runtime: boolean | null;
  /** Whether a save is under way. */
  readonly busy: boolean;
  /** The name of the application the automation watches, or null before one is picked. */
  readonly app: string | null;
  /** Whether an action presses keys or clicks with the cursor, which happens only while that application is in front. */
  readonly needsFront: boolean;
  readonly ask: RunAsk;
}

/** Whether Run actions now can be pressed, and what the editor says under it. */
export interface RunNowView {
  readonly disabled: boolean;
  readonly note: string;
  /** Whether the note is a warning: why the ask failed. */
  readonly warn: boolean;
}

/**
 * Run actions now for `facts`. It runs the saved automation, so it waits for a save while the editor holds anything
 * else, and for the background part, which is what runs the actions.
 */
export function runNowView(facts: RunNowFacts): RunNowView {
  const off = (note: string): RunNowView => ({ disabled: true, note, warn: false });
  if (facts.saved === 'new') {
    return off('Save this automation, then open it again to run its actions once from here.');
  }
  if (facts.saved === 'changed') {
    return off(
      'This runs the automation as it is saved, without your changes. Save them, then open it again to run its actions.',
    );
  }
  if (!facts.on) {
    return off("This automation is turned off, so AppyTappy's background part does not run it. Turn it on first.");
  }
  if (facts.runtime === false) {
    return off("AppyTappy's background part isn't running, so nothing can run the actions.");
  }
  if (facts.ask.kind === 'asking') {
    return off("Asking AppyTappy's background part…");
  }
  if (facts.ask.kind === 'failed') {
    return { disabled: facts.busy, note: facts.ask.message, warn: true };
  }
  const front = facts.needsFront
    ? ` Keys are pressed, and clicks with the cursor made, only while ${facts.app ?? 'the application'} is the window in front, so from here they are skipped.`
    : '';
  return { disabled: facts.busy, note: `${facts.ask.kind === 'asked' ? RUN_ASKED : ''}${front}`.trim(), warn: false };
}
