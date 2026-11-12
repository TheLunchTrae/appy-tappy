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

/**
 * Moves the focus to the control `within` marks as `data-landing="target"`, once, as soon as `target` names one,
 * when `owed`: the component shows because another flow ended, which took the focused control with it.
 */
export function useLanding(owed: boolean, target: string | null, within: RefObject<HTMLElement | null>): void {
  const pending = useRef(owed);
  useEffect(() => {
    if (pending.current && target !== null) {
      pending.current = false;
      within.current?.querySelector<HTMLElement>(`[data-landing="${target}"]`)?.focus();
    }
  }, [target, within]);
}
