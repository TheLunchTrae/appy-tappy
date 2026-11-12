// Where the keyboard focus lands when a flow shows again after another: on the control the next step is taken with.

import { type RefObject, useEffect, useRef } from 'react';

import type { PickerStep } from './picker.ts';
import type { WatchStep } from './watch.ts';

/** A control of the automation editor's When card that can take the focus. */
export type WatchControl = 'application' | 'add-application' | 'new-value' | 'value' | 'comparison';

/**
 * The control of the automation editor that takes the focus at `step`. Null while what was saved is still read,
 * so the focus moves once it is known.
 */
export function editorLanding(step: WatchStep): WatchControl | null {
  switch (step.kind) {
    case 'no-applications':
      return 'add-application';
    case 'choosing':
    case 'missing':
      return 'application';
    case 'arriving':
      return null;
    case 'value':
      return 'new-value';
    case 'pick':
      return 'value';
    case 'ready':
      return 'comparison';
  }
}

/** A control of the picker that can take the focus. */
export type PickerControl = 'add-application' | 'refresh' | 'windows' | 'match' | 'capture';

/**
 * The control of the picker that takes the focus at `step`. Null while the look is under way, so the focus moves
 * once the chosen application's window is known.
 */
export function pickerLanding(step: PickerStep): PickerControl | null {
  switch (step.kind) {
    case 'looking':
    case 'resolving':
      return null;
    case 'no-applications':
    case 'choosing':
      return 'add-application';
    case 'failed':
    case 'missing':
      return 'refresh';
    case 'none':
      return 'match';
    case 'one':
      return 'capture';
    case 'several':
      return step.picked === null ? 'windows' : 'capture';
  }
}

/** What a landing that may still be owed does now. */
export interface Landing {
  /** The control to move the focus to now, or null to leave the focus alone. */
  readonly land: string | null;
  /** Whether the landing is still owed afterwards. */
  readonly owed: boolean;
}

/**
 * What a landing does when `target` names the control to land on, or null while it is not known, and `held` says
 * something has the keyboard focus. A landing is for a user whose focus fell to the page because the control they
 * pressed went away: it lands only while nothing holds the focus, and it is dropped for good once the user has
 * put the focus somewhere themselves, so it never takes the focus from what they are doing.
 */
export function settle(owed: boolean, target: string | null, held: boolean): Landing {
  if (!owed || held) {
    return { land: null, owed: false };
  }
  return target === null ? { land: null, owed: true } : { land: target, owed: false };
}

/** Whether something other than the page itself has the keyboard focus. */
function focusHeld(): boolean {
  return document.activeElement !== null && document.activeElement !== document.body;
}

/**
 * Lands the focus once on the control `within` marks as `data-landing="target"`, as {@link settle} decides, when
 * `owed`: the component shows because another flow ended, which took the focused control with it.
 */
export function useLanding(owed: boolean, target: string | null, within: RefObject<HTMLElement | null>): void {
  const pending = useRef(owed);
  // A focus the user moves while the landing waits ends the wait, also if they leave that control again.
  useEffect(() => {
    const moved = (): void => {
      pending.current = settle(pending.current, null, true).owed;
    };
    document.addEventListener('focusin', moved);
    return () => {
      document.removeEventListener('focusin', moved);
    };
  }, []);
  useEffect(() => {
    const now = settle(pending.current, target, focusHeld());
    pending.current = now.owed;
    if (now.land !== null) {
      within.current?.querySelector<HTMLElement>(`[data-landing="${now.land}"]`)?.focus();
    }
  }, [target, within]);
}
