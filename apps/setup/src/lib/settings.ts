// The settings file, as its Rust side defines it, and the context that hands the settings to every page.

import { invoke } from '@tauri-apps/api/core';
import { createContext, use } from 'react';

import { DEFAULT_SHORTCUT } from './shortcut.ts';

/** How often the runtime reads a value: as often as `fastestMs` allows while it changes, and at least every `slowestMs`. */
export interface ReadInterval {
  readonly fastestMs: number;
  readonly slowestMs: number;
}

/** Every setting in the settings file. */
export interface Settings {
  readonly format: number;
  readonly readInterval: ReadInterval;
  readonly screenshotShortcut: string;
}

/** The saved settings, or the defaults and why the file cannot be read. */
export interface SettingsView {
  readonly settings: Settings;
  readonly problem: string | null;
}

/** The shortest and longest wait between two reads that the runtime takes, in milliseconds. */
export const FASTEST_READ_MS = 1;
export const SLOWEST_READ_MS = 10_000;

/** The settings a new install starts with, as the Rust side defines them. */
export const DEFAULT_SETTINGS: Settings = {
  format: 1,
  readInterval: { fastestMs: 16, slowestMs: 500 },
  screenshotShortcut: DEFAULT_SHORTCUT,
};

export async function loadSettings(): Promise<SettingsView> {
  return invoke<SettingsView>('settings');
}

/** Saves every setting; rejects with what to fix. */
export async function saveSettings(settings: Settings): Promise<void> {
  await invoke('save_settings', { settings });
}

/** Whether the runtime starts when the user signs in to Windows. */
export async function startsWithWindows(): Promise<boolean> {
  return invoke<boolean>('starts_with_windows');
}

export async function setStartsWithWindows(on: boolean): Promise<void> {
  await invoke('set_starts_with_windows', { on });
}

/**
 * The whole milliseconds `text` names, or null when it is not a whole number from {@link FASTEST_READ_MS} to
 * {@link SLOWEST_READ_MS}.
 */
export function readMs(text: string): number | null {
  const trimmed = text.trim();
  if (!/^\d+$/u.test(trimmed)) {
    return null;
  }
  const ms = Number(trimmed);
  return ms >= FASTEST_READ_MS && ms <= SLOWEST_READ_MS ? ms : null;
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

export const SettingsContext = createContext<SettingsHandle>({
  settings: DEFAULT_SETTINGS,
  change: async (update) => {
    const next = update(DEFAULT_SETTINGS);
    if ('problem' in next) {
      return next.problem;
    }
    await saveSettings(next);
    return null;
  },
});

/** Whether two settings are the same, so saving one over the other changes nothing. */
export function sameSettings(one: Settings, other: Settings): boolean {
  return JSON.stringify(one) === JSON.stringify(other);
}

export function useSettings(): SettingsHandle {
  return use(SettingsContext);
}
