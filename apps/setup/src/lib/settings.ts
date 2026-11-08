// The settings file, as its Rust side defines it, and the context that hands the settings to every page.

import { invoke } from '@tauri-apps/api/core';
import { createContext, use } from 'react';

import { DEFAULT_SHORTCUT } from './shortcut.ts';

/** How often the runtime reads a value: as often as `fastestMs` allows while it changes, and at least every `slowestMs`. */
export interface ReadInterval {
  readonly fastestMs: number;
  readonly slowestMs: number;
}

/** What a runtime trace can record. */
export type TraceCategory = 'readings' | 'changes' | 'rules' | 'actions' | 'lifecycle';

/** Each category, in the order the page lists them. */
export const TRACE_CATEGORIES: readonly TraceCategory[] = ['readings', 'changes', 'rules', 'actions', 'lifecycle'];

/** The runtime trace: whether it records, until when, what, and how much disk its files may take. */
export interface TraceSettings {
  readonly on: boolean;
  /** When recording ends, in milliseconds since 1970, or null to record until it is turned off. */
  readonly until: number | null;
  readonly record: readonly TraceCategory[];
  readonly fileLimitMb: number;
  readonly keepFiles: number;
}

/** Every setting in the settings file. */
export interface Settings {
  readonly format: number;
  readonly readInterval: ReadInterval;
  readonly screenshotShortcut: string;
  readonly trace: TraceSettings;
}

/** The saved settings, or the defaults and why the file cannot be read. */
export interface SettingsView {
  readonly settings: Settings;
  readonly problem: string | null;
}

/** The shortest and longest wait between two reads that the runtime takes, in milliseconds. */
export const FASTEST_READ_MS = 1;
export const SLOWEST_READ_MS = 10_000;

/** The largest trace file, in megabytes, and the most trace files to keep, that the runtime takes. */
export const MAX_TRACE_FILE_MB = 1000;
export const MAX_TRACE_FILES = 100;

/** The settings a new install starts with, as the Rust side defines them. */
export const DEFAULT_SETTINGS: Settings = {
  format: 1,
  readInterval: { fastestMs: 16, slowestMs: 500 },
  screenshotShortcut: DEFAULT_SHORTCUT,
  trace: {
    on: false,
    until: null,
    record: ['changes', 'rules', 'actions', 'lifecycle'],
    fileLimitMb: 50,
    keepFiles: 5,
  },
};

export async function loadSettings(): Promise<SettingsView> {
  return invoke<SettingsView>('settings');
}

/** Saves every setting; rejects with what to fix. */
export async function saveSettings(settings: Settings): Promise<void> {
  await invoke('save_settings', { settings });
}

/** The folder the runtime writes its trace files to. */
export async function traceFolder(): Promise<string> {
  return invoke<string>('trace_folder');
}

/** Opens the trace folder in File Explorer. */
export async function openTraceFolder(): Promise<void> {
  await invoke('open_trace_folder');
}

/** Whether the runtime starts when the user signs in to Windows. */
export async function startsWithWindows(): Promise<boolean> {
  return invoke<boolean>('starts_with_windows');
}

export async function setStartsWithWindows(on: boolean): Promise<void> {
  await invoke('set_starts_with_windows', { on });
}

/** The whole number `text` names, or null when it is not a whole number from `min` to `max`. */
export function readWhole(text: string, min: number, max: number): number | null {
  const trimmed = text.trim();
  if (!/^\d+$/u.test(trimmed)) {
    return null;
  }
  const value = Number(trimmed);
  return value >= min && value <= max ? value : null;
}

/**
 * The whole milliseconds `text` names, or null when it is not a whole number from {@link FASTEST_READ_MS} to
 * {@link SLOWEST_READ_MS}.
 */
export function readMs(text: string): number | null {
  return readWhole(text, FASTEST_READ_MS, SLOWEST_READ_MS);
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
export function traceRecords(trace: TraceSettings, now: number): boolean {
  return trace.on && (trace.until === null || trace.until > now);
}

/** `trace` with `category` recorded or not, the categories kept in the page's order. */
export function withCategory(trace: TraceSettings, category: TraceCategory, on: boolean): TraceSettings {
  const record = TRACE_CATEGORIES.filter((each) => (each === category ? on : trace.record.includes(each)));
  return { ...trace, record };
}

/**
 * `interval` with one of its waits set to `ms`, or what to fix: the fastest read cannot wait longer than the slowest.
 */
export function withWait(
  interval: ReadInterval,
  which: keyof ReadInterval,
  ms: number,
): { readonly interval: ReadInterval } | { readonly problem: string } {
  const next = { ...interval, [which]: ms };
  return next.fastestMs > next.slowestMs
    ? { problem: 'The fastest read cannot wait longer than the slowest.' }
    : { interval: next };
}

/** A change to the settings: the new settings, made from the current ones, or what to fix. */
export type SettingsUpdate = (current: Settings) => Settings | { readonly problem: string };

/** The settings every page reads, and how a page changes them: it saves, then the new settings show. */
export interface SettingsHandle {
  readonly settings: Settings;
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
function sameTrace(one: TraceSettings, other: TraceSettings): boolean {
  return (
    one.on === other.on &&
    one.until === other.until &&
    one.fileLimitMb === other.fileLimitMb &&
    one.keepFiles === other.keepFiles &&
    one.record.length === other.record.length &&
    one.record.every((category) => other.record.includes(category))
  );
}

/** Whether two settings are the same, so saving one over the other changes nothing. */
export function sameSettings(one: Settings, other: Settings): boolean {
  return (
    one.format === other.format &&
    one.readInterval.fastestMs === other.readInterval.fastestMs &&
    one.readInterval.slowestMs === other.readInterval.slowestMs &&
    one.screenshotShortcut === other.screenshotShortcut &&
    sameTrace(one.trace, other.trace)
  );
}

export function useSettings(): SettingsHandle {
  return use(SettingsContext);
}
