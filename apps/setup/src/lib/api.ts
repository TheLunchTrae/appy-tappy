// The setup app's commands, typed as its Rust side defines them.

import { invoke } from '@tauri-apps/api/core';

import { type Color, type Picture, decodePicture } from './pixels.ts';
import type { Rect, Size } from './region.ts';
import type { SelectionSpec, SelectionView } from './selection.ts';

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

/** One of the things an application's window is recognized by. */
export type MatchField = 'title' | 'class' | 'executable';

/** A field that may be allowed to differ; the executable always has to match. */
export type LooseField = Exclude<MatchField, 'executable'>;

/** What to take when no window qualifies: nothing, or any window of the executable. */
export type Fallback = 'none' | 'executable';

/**
 * How an application's window is recognized. `*` and `?` are wildcards; case is ignored. A window qualifies when
 * its executable and every field in `mustMatch` match; among those, the one matching the most fields wins.
 */
export interface WindowDef {
  readonly title: string;
  readonly class: string;
  readonly executable: string;
  readonly mustMatch: Readonly<Record<LooseField, boolean>>;
  readonly fallback: Fallback;
}

/** A running window an application's window match takes. */
export interface WindowFit {
  readonly handle: number;
  /** Whether it is taken only because no window qualifies. */
  readonly fallback: boolean;
  /**
   * Whether no other window fits better. The runtime reads one of the best windows, the one Windows lists first,
   * so a value has to be drawn on one of them.
   */
  readonly best: boolean;
}

/** How strongly an application's anti-cheat protects it. */
export type AntiCheat = 'none' | 'userMode' | 'kernelLevel' | 'unknown';

/** A saved check, as the application file spells it. */
export type CheckDef =
  /** Reads a number with the characters of the reference `reference`; `whole` ignores every digit after the decimal point. */
  | { readonly kind: 'number'; readonly reference: string; readonly whole: boolean }
  | { readonly kind: 'barFill'; readonly filled: Color; readonly empty: Color }
  | { readonly kind: 'color'; readonly color: Color; readonly tolerance: number }
  /** Which of `states` the region shows, each a name and the file name of its picture. */
  | { readonly kind: 'state'; readonly states: readonly StateDef[]; readonly tolerance: number };

/** One look a state check knows: its name, and its picture's file name. */
interface StateDef {
  readonly name: string;
  readonly image: string;
}

/** How an application's characters look: their color, and the pictures they were learned from. */
export interface ReferenceDef {
  readonly id: string;
  readonly name: string;
  readonly ink: Color;
  readonly tolerance: number;
  readonly samples: readonly { readonly image: string; readonly text: string }[];
}

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
export interface AppFile {
  readonly format: number;
  readonly name: string;
  readonly window: WindowDef;
  readonly antiCheat: AntiCheat;
  readonly values: readonly ValueDef[];
  readonly references: readonly ReferenceDef[];
  readonly overlay?: OverlayDef;
}

/** A saved application, with the handles of the running windows its window match takes, the best first. */
export interface AppSummary {
  readonly id: string;
  readonly file: AppFile | null;
  readonly error: string | null;
  readonly windows: readonly number[];
}

/** An application's name and how its window is recognized, as the editor saves them. */
export interface AppSpec {
  readonly name: string;
  readonly window: WindowDef;
}

/** A saved reference of a saved application. */
export interface ReferenceRef {
  readonly app: string;
  readonly id: string;
}

/** The characters a reference is taught, which the capture screen tries and the reference editor saves. */
export interface CharactersSpec {
  readonly kind: 'characters';
  readonly region: Rect;
  /** The screenshot the region was drawn on; every labeled screenshot and the recording must have its window size. */
  readonly screenshot: number;
  /** The saved reference these add to, or null for a new one. */
  readonly reference: ReferenceRef | null;
  readonly ink: Color;
  readonly tolerance: number;
  readonly labels: readonly { readonly screenshot: number; readonly text: string }[];
  /** Readings of this session's recording, which must be of `region`. */
  readonly recorded: readonly RecordedLabel[];
}

/** A check as the capture screen describes it, naming this session's screenshots by index. */
export type CheckSpec =
  | { readonly kind: 'number'; readonly region: Rect; readonly reference: ReferenceRef; readonly whole: boolean }
  | CharactersSpec
  | { readonly kind: 'barFill'; readonly region: Rect; readonly filled: Color; readonly empty: Color }
  | { readonly kind: 'color'; readonly region: Rect; readonly color: Color; readonly tolerance: number }
  | {
      readonly kind: 'state';
      readonly region: Rect;
      /** The screenshot the region was drawn on; every state's screenshot must have its window size. */
      readonly drawnOn: number;
      readonly states: readonly StateSpec[];
      readonly tolerance: number;
    };

/** One state of a state check, taken from a screenshot. */
export interface StateSpec {
  readonly name: string;
  readonly screenshot: number;
  /** The thing to compare, selected on the screenshot; without one the whole region is compared. */
  readonly selection?: SelectionSpec;
}

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
  /** What it records with; Record more needs the same. */
  readonly settings: RecordingSettings;
}

/** The region, color and tolerance a recording reads with, and the window size the region was drawn at. */
export interface RecordingSettings {
  readonly region: Rect;
  readonly ink: Color;
  readonly tolerance: number;
  readonly windowSize: Size;
}

/** What a check read. */
export type Reading =
  | { readonly kind: 'number'; readonly value: number }
  | { readonly kind: 'flag'; readonly value: boolean }
  /** A state's name, or `none` when the region shows none of them. */
  | { readonly kind: 'state'; readonly value: string };

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

/** A value to save to a saved application. */
export interface SaveSpec {
  /** The saved application to add the value to. */
  readonly app: string;
  readonly valueName: string;
  readonly check: CheckSpec;
  /** The screenshot the region was drawn on, whose size the value keeps. */
  readonly screenshot: number;
}

/** A reference to save to a saved application. */
export interface ReferenceSaveSpec {
  /** The saved application the reference belongs to. */
  readonly app: string;
  readonly referenceName: string;
  readonly check: CharactersSpec;
}

/** Where a reference was saved. */
export interface SavedReference {
  readonly app: string;
  readonly reference: string;
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
  | { readonly kind: 'keys'; readonly keys: string }
  /** Opens a program or file by its full path, or an `http` or `https` link. */
  | { readonly kind: 'open'; readonly target: string }
  /** Plays a WAV file by its full path. */
  | { readonly kind: 'sound'; readonly file: string };

/** What a condition compares a value with. */
export type Target = number | boolean | string;

/** The condition on one saved value that makes an automation run. */
export interface WhenDef {
  /** The application's ID. */
  readonly app: string;
  /** The value's ID. */
  readonly value: string;
  readonly is: Comparison;
  /**
   * A number for a value that reads a number; true or false for one that reads yes or no; a state's name, or
   * `none`, for one that reads a state.
   */
  readonly target: Target;
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
  return typeof error === 'string' ? error : 'Something went wrong in AppyTappy Setup.';
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

/** What `selection`'s steps select on screenshot `index`, or null when they leave nothing selected. */
export async function select(index: number, selection: SelectionSpec): Promise<SelectionView | null> {
  return invoke<SelectionView | null>('select', { screenshot: index, selection });
}

export async function setCheck(spec: CheckSpec): Promise<void> {
  await invoke('set_check', { spec });
}

/** What the check reads on the newest frame, or null without a check or a frame. */
export async function readValue(): Promise<Reading | null> {
  return invoke<Reading | null>('read_value');
}

/** Records `region` of the picked window, drawn on `screenshot`. With `more`, goes on with the stopped recording, which needs the same region, ink and window size; otherwise starts a new one. */
export async function startRecording(
  region: Rect,
  ink: Color,
  tolerance: number,
  screenshot: number,
  more: boolean,
): Promise<void> {
  await invoke('start_recording', { region, ink, tolerance, screenshot, more });
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

/** Why the reference `spec` teaches cannot learn each of its recorded readings, in order, or null for one it can. */
export async function reviewProblems(spec: CharactersSpec): Promise<readonly (string | null)[]> {
  return invoke<readonly (string | null)[]>('review_problems', { spec });
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

/** The running windows `window` takes, the best fit first. */
export async function matchWindows(window: WindowDef): Promise<readonly WindowFit[]> {
  return invoke<readonly WindowFit[]>('match_windows', { window });
}

export async function listApps(): Promise<readonly AppSummary[]> {
  return invoke<readonly AppSummary[]>('list_apps');
}

/** Saves `spec` over the application `existing`, or as a new one; returns its ID. */
export async function saveApp(existing: string | null, spec: AppSpec): Promise<string> {
  return invoke<string>('save_app', { existing, spec });
}

/**
 * Removes an application with its values, unless an automation uses one of them, and closes its overlay. The removal
 * worked when this resolves; the answer is why an overlay window could not open or close, or null.
 */
export async function removeApp(application: string): Promise<string | null> {
  return invoke<string | null>('remove_app', { application });
}

export async function saveValue(spec: SaveSpec): Promise<SavedValue> {
  return invoke<SavedValue>('save_value', { spec });
}

export async function deleteValue(application: string, value: string): Promise<void> {
  await invoke('delete_value', { application, value });
}

/** Saves a reference, over the one `spec.check.reference` names when it names one. */
export async function saveReference(spec: ReferenceSaveSpec): Promise<SavedReference> {
  return invoke<SavedReference>('save_reference', { spec });
}

/** Removes a saved reference and its pictures; refused while a value reads with it. */
export async function deleteReference(application: string, reference: string): Promise<void> {
  await invoke('delete_reference', { application, reference });
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

/**
 * Turns an application's overlay on or off and sets where it shows. The setting is saved when this resolves; the
 * answer is why an overlay window could not open or close, or null.
 */
export async function setOverlay(application: string, enabled: boolean, mode: OverlayMode): Promise<string | null> {
  return invoke<string | null>('set_overlay', { application, enabled, mode });
}

/** What an application's overlay shows now. */
export async function overlayState(application: string): Promise<OverlayView> {
  return invoke<OverlayView>('overlay_state', { application });
}

/** Tells the setup app the size of an overlay's card in CSS pixels, which its window over the application takes. */
export async function overlayCard(application: string, width: number, height: number): Promise<void> {
  await invoke('overlay_card', { application, width, height });
}

/** A newer AppyTappy release the user can install. */
export interface UpdateOffer {
  readonly version: string;
}

/** The newer release on GitHub, or null when this one is the newest or this build does not update itself. */
export async function checkForUpdate(): Promise<UpdateOffer | null> {
  return invoke<UpdateOffer | null>('check_for_update');
}

/**
 * Downloads `version` and runs its installer, which closes AppyTappy Setup and the background part, updates both, and
 * starts them again. It resolves only when the update fails before the installer starts.
 */
export async function installUpdate(version: string): Promise<void> {
  await invoke('install_update', { version });
}
