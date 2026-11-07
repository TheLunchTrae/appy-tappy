// The setup app's commands, typed as its Rust side defines them.

import { invoke } from '@tauri-apps/api/core';

import { type Color, type Picture, decodePicture } from './pixels.ts';
import type { Rect } from './region.ts';

/** A window the user can pick. */
export interface WindowChoice {
  readonly handle: number;
  readonly title: string;
  readonly class: string;
  readonly executable: string;
  readonly width: number;
  readonly height: number;
}

/** The windows the user can pick, and the anti-cheat products running now. */
export interface WindowList {
  readonly windows: readonly WindowChoice[];
  readonly antiCheat: readonly string[];
}

/** How far a window match falls back when no window fits all its patterns. */
export type MatchPriority = 'title' | 'titleElseClass' | 'titleElseExecutable';

/** How an application's window is recognized. `*` and `?` are wildcards; case is ignored. */
export interface WindowDef {
  readonly title: string;
  readonly class: string;
  readonly executable: string;
  readonly priority: MatchPriority;
}

/** How strongly an application's anti-cheat protects it. */
export type AntiCheat = 'none' | 'userMode' | 'kernelLevel' | 'unknown';

/** A saved check, as the application file spells it. */
export type CheckDef =
  | {
      readonly kind: 'number';
      readonly ink: Color;
      readonly tolerance: number;
      readonly samples: readonly { readonly image: string; readonly text: string }[];
    }
  | { readonly kind: 'barFill'; readonly filled: Color; readonly empty: Color }
  | { readonly kind: 'color'; readonly color: Color; readonly tolerance: number }
  | { readonly kind: 'reference'; readonly image: string; readonly tolerance: number };

/** A saved value. */
export interface ValueDef {
  readonly id: string;
  readonly name: string;
  readonly screen: {
    readonly windowSize: { readonly width: number; readonly height: number };
    readonly region: Rect;
    readonly check: CheckDef;
  };
}

/** A saved application's file. */
interface AppFile {
  readonly format: number;
  readonly name: string;
  readonly window: WindowDef;
  readonly antiCheat: AntiCheat;
  readonly values: readonly ValueDef[];
}

/** A saved application, with the running windows that match it. */
export interface AppSummary {
  readonly id: string;
  readonly file: AppFile | null;
  readonly error: string | null;
  readonly windows: readonly number[];
}

/** A check as the capture screen describes it, naming this session's screenshots by index. */
export type CheckSpec =
  | {
      readonly kind: 'number';
      readonly region: Rect;
      readonly ink: Color;
      readonly tolerance: number;
      readonly labels: readonly { readonly screenshot: number; readonly text: string }[];
    }
  | { readonly kind: 'barFill'; readonly region: Rect; readonly filled: Color; readonly empty: Color }
  | { readonly kind: 'color'; readonly region: Rect; readonly color: Color; readonly tolerance: number }
  | { readonly kind: 'reference'; readonly region: Rect; readonly screenshot: number; readonly tolerance: number };

/** What a check read. */
export type Reading =
  | { readonly kind: 'number'; readonly value: number }
  | { readonly kind: 'flag'; readonly value: boolean };

/** A value to save, with a new application when `app` is null. */
export interface SaveSpec {
  readonly app: string | null;
  readonly name: string;
  readonly window: WindowDef;
  readonly valueName: string;
  readonly check: CheckSpec;
  /** The screenshot the region was drawn on, whose size the value keeps. */
  readonly screenshot: number;
}

/** Where a value was saved. */
export interface SavedValue {
  readonly app: string;
  readonly value: string;
}

/** How a value relates to the target it is compared with. */
export type Comparison = 'below' | 'atMost' | 'equals' | 'notEquals' | 'atLeast' | 'above';

/** What a media action does. */
export type MediaCommand = 'play' | 'pause' | 'next' | 'previous';

/** One action. A null `app` means the media session Windows considers current, or the whole system's volume. */
export type ActionDef =
  | { readonly kind: 'media'; readonly command: MediaCommand; readonly app: string | null }
  | { readonly kind: 'volume'; readonly percent: number; readonly app: string | null };

/** The condition on one saved value that makes an automation run. */
export interface WhenDef {
  /** The application's ID. */
  readonly app: string;
  /** The value's ID. */
  readonly value: string;
  readonly is: Comparison;
  /** A number for a value that reads a number; true or false for one that reads yes or no. */
  readonly target: number | boolean;
}

/** A saved automation, as the automations file spells it. */
export interface AutomationDef {
  readonly id: string;
  readonly name: string;
  readonly enabled: boolean;
  readonly when: WhenDef;
  readonly actions: readonly ActionDef[];
  /** Whether to undo the actions when the condition stops being true. */
  readonly undo: boolean;
  readonly cooldownMs: number;
}

/** A saved automation, with why it cannot run as saved, if so. */
export interface AutomationSummary {
  readonly automation: AutomationDef;
  readonly problem: string | null;
}

/** The message of a failed command, which the Rust side writes for the user. */
export function errorText(error: unknown): string {
  return typeof error === 'string' ? error : 'Something went wrong in AppyTappy setup.';
}

export async function listWindows(): Promise<WindowList> {
  return invoke<WindowList>('list_windows');
}

export async function pickWindow(window: number): Promise<void> {
  await invoke('pick_window', { window });
}

/** Stops capturing and forgets the session's screenshots and check. */
export async function stopCapture(): Promise<void> {
  await invoke('stop_capture');
}

/** The newest frame at most `maxWidth` wide, or null before one arrives. */
export async function preview(maxWidth: number): Promise<Picture | null> {
  return decodePicture(await invoke<ArrayBuffer>('preview', { maxWidth }));
}

/** Keeps the newest frame as a screenshot and returns its index. */
export async function takeScreenshot(): Promise<number> {
  return invoke<number>('take_screenshot');
}

/** A screenshot at most `maxWidth` wide, or null for an unknown index. */
export async function screenshot(index: number, maxWidth: number): Promise<Picture | null> {
  return decodePicture(await invoke<ArrayBuffer>('screenshot', { index, maxWidth }));
}

export async function setCheck(spec: CheckSpec): Promise<void> {
  await invoke('set_check', { spec });
}

/** What the check reads on the newest frame, or null without a check or a frame. */
export async function readValue(): Promise<Reading | null> {
  return invoke<Reading | null>('read_value');
}

/** The handles of the running windows `window` matches, the best fit first. */
export async function matchWindows(window: WindowDef): Promise<readonly number[]> {
  return invoke<readonly number[]>('match_windows', { window });
}

export async function listApps(): Promise<readonly AppSummary[]> {
  return invoke<readonly AppSummary[]>('list_apps');
}

export async function saveValue(spec: SaveSpec): Promise<SavedValue> {
  return invoke<SavedValue>('save_value', { spec });
}

export async function deleteValue(application: string, value: string): Promise<void> {
  await invoke('delete_value', { application, value });
}

export async function listAutomations(): Promise<readonly AutomationSummary[]> {
  return invoke<readonly AutomationSummary[]>('list_automations');
}

/** Saves `automation` over the automation `existing`, or as a new one; returns its ID. */
export async function saveAutomation(existing: string | null, automation: AutomationDef): Promise<string> {
  return invoke<string>('save_automation', { existing, automation });
}

export async function setAutomationEnabled(id: string, enabled: boolean): Promise<void> {
  await invoke('set_automation_enabled', { id, enabled });
}

export async function deleteAutomation(id: string): Promise<void> {
  await invoke('delete_automation', { id });
}
