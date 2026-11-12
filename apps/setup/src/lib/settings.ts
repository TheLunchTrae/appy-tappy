// The settings, as the pages read and change them, and the context that hands them to every page.

import { createContext, use } from 'react';

import {
  DEFAULT_SETTINGS,
  LIMITS,
  type ReadIntervalDef,
  type SettingsFile,
  TRACE_CATEGORIES,
  type TraceCategory,
  type TraceDef,
} from './bindings.ts';

/** The whole number `text` names, or null when it is not a whole number from `min` to `max`. */
export function readWhole(text: string, min: number, max: number): number | null {
  const trimmed = text.trim();
  if (!/^\d+$/u.test(trimmed)) {
    return null;
  }
  const value = Number(trimmed);
  return value >= min && value <= max ? value : null;
}

/** The whole milliseconds `text` names, or null when it is not a whole number in the range the runtime takes. */
export function readMs(text: string): number | null {
  return readWhole(text, LIMITS.fastestReadMs, LIMITS.slowestReadMs);
}

/** How long a trace records once it is turned on: a number of minutes, or until it is turned off. */
export type TraceLength = 10 | 60 | 'off';

/** When a trace turned on at `now` stops, for `length`: milliseconds since 1970, or null for no end. */
export function traceEnd(length: TraceLength, now: number): number | null {
  return length === 'off' ? null : now + length * 60_000;
}

/**
 * Whether the trace records at `now`, as the runtime reads `trace`: it is on, and its end time, if it has one, is
 * still to come.
 */
export function traceRecords(trace: TraceDef, now: number): boolean {
  return trace.on && (trace.until === null || trace.until > now);
}

/** `trace` with `category` recorded or not, the categories kept in the page's order. */
export function withCategory(trace: TraceDef, category: TraceCategory, on: boolean): TraceDef {
  const record = TRACE_CATEGORIES.filter((each) => (each === category ? on : trace.record.includes(each)));
  return { ...trace, record };
}

/**
 * `interval` with one of its waits set to `ms`, or what to fix: the fastest read cannot wait longer than the slowest.
 */
export function withWait(
  interval: ReadIntervalDef,
  which: keyof ReadIntervalDef,
  ms: number,
): { readonly interval: ReadIntervalDef } | { readonly problem: string } {
  const next = { ...interval, [which]: ms };
  return next.fastestMs > next.slowestMs
    ? { problem: 'The fastest read cannot wait longer than the slowest.' }
    : { interval: next };
}

/** A change to the settings: the new settings, made from the current ones, or what to fix. */
export type SettingsUpdate = (current: SettingsFile) => SettingsFile | { readonly problem: string };

/** The settings every page reads, and how a page changes them: it saves, then the new settings show. */
export interface SettingsHandle {
  readonly settings: SettingsFile;
  /**
   * Saves what `update` makes of the saved settings, after the settings have loaded and after every change asked for
   * before it, so no change is made from settings that are out of date. Resolves to what to fix when `update` refuses,
   * or null; rejects with why the save failed, and the saved settings stay.
   */
  readonly change: (update: SettingsUpdate) => Promise<string | null>;
}

/** Outside the app's provider there are no saved settings to change, so a change fails rather than save the defaults. */
export const SettingsContext = createContext<SettingsHandle>({
  settings: DEFAULT_SETTINGS,
  change: () => Promise.reject(new Error('The settings have not loaded.')),
});

/** Whether two traces' settings are the same. */
function sameTrace(one: TraceDef, other: TraceDef): boolean {
  return (
    one.on === other.on &&
    one.until === other.until &&
    one.fileLimitMb === other.fileLimitMb &&
    one.keepFiles === other.keepFiles &&
    one.record.length === other.record.length &&
    one.record.every((category) => other.record.includes(category))
  );
}

/** `until` as the user reads it: the time, with the day when it is not today. */
export function endText(until: number): string {
  const end = new Date(until);
  const today = new Date().toDateString() === end.toDateString();
  return today
    ? end.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })
    : end.toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' });
}

/** Whether two settings are the same, so saving one over the other changes nothing. */
export function sameSettings(one: SettingsFile, other: SettingsFile): boolean {
  return (
    one.format === other.format &&
    one.readInterval.fastestMs === other.readInterval.fastestMs &&
    one.readInterval.slowestMs === other.readInterval.slowestMs &&
    one.screenshotShortcut === other.screenshotShortcut &&
    sameTrace(one.trace, other.trace) &&
    one.rememberEnabled === other.rememberEnabled &&
    one.showDebug === other.showDebug
  );
}

export function useSettings(): SettingsHandle {
  return use(SettingsContext);
}
