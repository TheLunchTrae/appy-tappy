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

/** Where an application's live readings show: over its window, or in a panel kept on top. */
export type OverlayMode = 'overlay' | 'pictureInPicture';

/** How the setup app shows an application's live readings on top of it. */
interface OverlayDef {
  readonly enabled: boolean;
  readonly mode: OverlayMode;
}

/** A saved application's file. `overlay` is absent until the overlay was first set. */
interface AppFile {
  readonly format: number;
  readonly name: string;
  readonly window: WindowDef;
  readonly antiCheat: AntiCheat;
  readonly values: readonly ValueDef[];
  readonly overlay?: OverlayDef;
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
      /** Readings of this session's recording, which must be of `region`. */
      readonly recorded: readonly RecordedLabel[];
    }
  | { readonly kind: 'barFill'; readonly region: Rect; readonly filled: Color; readonly empty: Color }
  | { readonly kind: 'color'; readonly region: Rect; readonly color: Color; readonly tolerance: number }
  | { readonly kind: 'reference'; readonly region: Rect; readonly screenshot: number; readonly tolerance: number };

/** What a recorded reading shows, as the user confirmed it in review. */
export interface RecordedLabel {
  /** The reading's index in the recording. */
  readonly reading: number;
  readonly text: string;
}

/** What the text reader made of a recorded reading. */
type RecordedText =
  | { readonly kind: 'pending' }
  /** The text it read, which is empty when it found none. */
  | { readonly kind: 'read'; readonly text: string }
  /** Why it could not read it, such as a missing model. */
  | { readonly kind: 'failed'; readonly text: string };

/** One reading a recording kept. */
export interface RecordedReading {
  /** When it first showed, from the start of the recording. */
  readonly firstMs: number;
  /** How many frames showed it. */
  readonly seen: number;
  readonly text: RecordedText;
}

/** A recording of a region, as the setup app shows it. */
export interface RecordingView {
  /** Its number this session; a new recording gets a new one. */
  readonly id: number;
  /** Whether it still records. */
  readonly recording: boolean;
  /** How long it has recorded, not counting pauses. */
  readonly elapsedMs: number;
  /** Whether it holds as many readings as it keeps. */
  readonly full: boolean;
  /** Why the latest frame could not be recorded, such as a resized window. */
  readonly problem: string | null;
  /** Its readings, in the order they first showed. */
  readonly readings: readonly RecordedReading[];
}

/** What a check read. */
export type Reading =
  | { readonly kind: 'number'; readonly value: number }
  | { readonly kind: 'flag'; readonly value: boolean };

/** What an application's overlay shows now. */
export interface OverlayView {
  readonly name: string;
  readonly mode: OverlayMode;
  /** Why there are no readings, such as that the application is not running. */
  readonly message: string | null;
  readonly values: readonly {
    readonly name: string;
    readonly reading: Reading | null;
    readonly problem: string | null;
  }[];
}

/** What a watched value reads on its window now, for the automation editor. */
export type Live =
  | { readonly kind: 'waiting' }
  /** `holds` says whether the condition asked about is true, or is null when none was asked about. */
  | { readonly kind: 'read'; readonly reading: Reading; readonly holds: boolean | null }
  | { readonly kind: 'problem'; readonly message: string }
  /** The capture stopped, such as when the window closed. */
  | { readonly kind: 'lost'; readonly message: string };

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

/**
 * One action. A null `app` means the media session Windows considers current, or the whole system's volume.
 * Keys, such as `Ctrl+Shift+M`, go to whichever window has the keyboard focus.
 */
export type ActionDef =
  | { readonly kind: 'media'; readonly command: MediaCommand; readonly app: string | null }
  | { readonly kind: 'volume'; readonly percent: number; readonly app: string | null }
  | { readonly kind: 'keys'; readonly keys: string };

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

/** Records `region` of the picked window, drawn on `screenshot`; goes on with a stopped recording of the same region and ink. */
export async function startRecording(region: Rect, ink: Color, tolerance: number, screenshot: number): Promise<void> {
  await invoke('start_recording', { region, ink, tolerance, screenshot });
}

/** Stops the recording, keeping its readings for review. */
export async function stopRecording(): Promise<void> {
  await invoke('stop_recording');
}

/** Forgets the recording and its readings. */
export async function discardRecording(): Promise<void> {
  await invoke('discard_recording');
}

/** The recording and its readings so far, or null without one. */
export async function recordingState(): Promise<RecordingView | null> {
  return invoke<RecordingView | null>('recording_state');
}

/** A recorded reading at most `maxWidth` wide, or null for an unknown index. */
export async function recordedReading(index: number, maxWidth: number): Promise<Picture | null> {
  return decodePicture(await invoke<ArrayBuffer>('recorded_reading', { index, maxWidth }));
}

/** Why the number check cannot learn each of `readings`, in order, or null for one it can. */
export async function reviewProblems(readings: readonly RecordedLabel[]): Promise<readonly (string | null)[]> {
  return invoke<readonly (string | null)[]>('review_problems', { readings });
}

/** Captures a saved value's region on its application's window, replacing any capture, and returns the watch's number. */
export async function watchValue(application: string, value: string): Promise<number> {
  return invoke<number>('watch_value', { application, value });
}

/** What the watch's value reads now, and whether `when` holds for it. Fails once the watch was stopped or replaced. */
export async function watchedReading(watch: number, when: WhenDef | null): Promise<Live> {
  return invoke<Live>('watched_reading', { watch, when });
}

/** Stops the watch, unless another capture replaced it already. */
export async function stopWatching(watch: number): Promise<void> {
  await invoke('stop_watching', { watch });
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

/** Sets how the application `application` protects itself from other programs. */
export async function setAntiCheat(application: string, antiCheat: AntiCheat): Promise<void> {
  await invoke('set_anti_cheat', { application, class: antiCheat });
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

/** Opens a window for each overlay that is on, and closes the rest. */
export async function syncOverlays(): Promise<void> {
  await invoke('sync_overlays');
}

/** Turns an application's overlay on or off and sets where it shows. */
export async function setOverlay(application: string, enabled: boolean, mode: OverlayMode): Promise<void> {
  await invoke('set_overlay', { application, enabled, mode });
}

/** What an application's overlay shows now. */
export async function overlayState(application: string): Promise<OverlayView> {
  return invoke<OverlayView>('overlay_state', { application });
}

/** Tells the setup app the size of an overlay's card in CSS pixels, which its window over the application takes. */
export async function overlayCard(application: string, width: number, height: number): Promise<void> {
  await invoke('overlay_card', { application, width, height });
}
