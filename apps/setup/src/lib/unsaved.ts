// What a flow holds that leaving it would lose, so the window asks before it drops the flow.

import { useEffect } from 'react';

/** What leaving a flow would lose: nothing, work not saved yet, or a save still under way. */
export type Held = 'nothing' | 'unsaved' | 'saving';

function isRecord(value: unknown): value is Readonly<Record<string, unknown>> {
  return typeof value === 'object' && value !== null;
}

/**
 * Whether `now` has the same content as `start`: equal primitives, and arrays, maps and plain objects whose entries
 * have the same content, whatever objects hold them. An edit undone by hand leaves the same content.
 */
export function sameContent(start: unknown, now: unknown): boolean {
  if (Object.is(start, now)) {
    return true;
  }
  if (start instanceof Map || now instanceof Map) {
    return (
      start instanceof Map &&
      now instanceof Map &&
      start.size === now.size &&
      [...start].every(([key, value]) => now.has(key) && sameContent(value, now.get(key)))
    );
  }
  if (Array.isArray(start) || Array.isArray(now)) {
    return (
      Array.isArray(start) &&
      Array.isArray(now) &&
      start.length === now.length &&
      start.every((value, index) => sameContent(value, now[index]))
    );
  }
  if (!isRecord(start) || !isRecord(now)) {
    return false;
  }
  const keys = Object.keys(start);
  return (
    keys.length === Object.keys(now).length &&
    keys.every((key) => Object.hasOwn(now, key) && sameContent(start[key], now[key]))
  );
}

/** What a flow holds: a save under way, or work unless what it shows is still what it started with. */
export function held(saving: boolean, start: unknown, now: unknown): Held {
  if (saving) {
    return 'saving';
  }
  return sameContent(start, now) ? 'nothing' : 'unsaved';
}

/** What a press that leaves a flow does: leave at once, ask first, or wait for the save. */
export type Leave = 'leave' | 'ask' | 'wait';

/** What a press that leaves a flow holding `now` does: work not saved is lost only once the user agrees. */
export function onLeave(now: Held): Leave {
  switch (now) {
    case 'nothing':
      return 'leave';
    case 'unsaved':
      return 'ask';
    case 'saving':
      return 'wait';
  }
}

/** Tells `report` what the flow holds whenever that changes, and that it holds nothing once the flow goes. */
export function useReportHeld(report: (held: Held) => void, now: Held): void {
  useEffect(() => {
    report(now);
  }, [report, now]);
  useEffect(
    () => () => {
      report('nothing');
    },
    [report],
  );
}
