// Generated from the setup app's command table by its bindings test. Do not edit it;
// change the table in `commands/table.rs` and run `UPDATE_BINDINGS=1 cargo test -p appytappy-setup bindings`.

import { invoke } from '@tauri-apps/api/core';

import type {
  AntiCheatDef,
  AppSpec,
  AppSummary,
  AutomationDef,
  AutomationSummary,
  CheckSpec,
  ClickAtDef,
  ClickPictureView,
  ColorSpec,
  DraftDef,
  DraftSummary,
  FixSpec,
  FoundClick,
  Live,
  OverlayModeDef,
  OverlayView,
  Reading,
  RecordingView,
  ReferenceSaveSpec,
  RegionSpec,
  SaveSpec,
  SavedReference,
  SavedValue,
  SelectionSpec,
  SelectionView,
  SettingsFile,
  SettingsView,
  UpdateOffer,
  WhenDef,
  WindowDef,
  WindowFit,
  WindowList,
} from './bindings.ts';
import { type Picture, decodePicture } from './pixels.ts';

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

/** What `selection`'s steps select on screenshot `screenshot`, or null when they leave nothing selected. */
export async function select(screenshot: number, selection: SelectionSpec): Promise<SelectionView | null> {
  return invoke<SelectionView | null>('select', { screenshot, selection });
}

export async function setCheck(spec: CheckSpec): Promise<void> {
  await invoke('set_check', { spec });
}

/** What the check reads on the newest frame, or null without a check or a frame. */
export async function readValue(): Promise<Reading | null> {
  return invoke<Reading | null>('read_value');
}

/**
 * Records `region` of the picked window, drawn on `screenshot`. With `more`, goes on with the stopped
 * recording, which needs the same region, ink and window size; otherwise starts a new one.
 */
export async function startRecording(region: RegionSpec, ink: ColorSpec, tolerance: number, screenshot: number, more: boolean): Promise<void> {
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

/**
 * Why the reference `spec` teaches cannot learn each of its recorded readings, in order, or null for one it
 * can.
 */
export async function reviewProblems(spec: CheckSpec): Promise<ReadonlyArray<string | null>> {
  return invoke<ReadonlyArray<string | null>>('review_problems', { spec });
}

/**
 * Captures a saved value's region on its application's window, replacing any capture, and returns the
 * watch's number.
 */
export async function watchValue(application: string, value: string): Promise<number> {
  return invoke<number>('watch_value', { application, value });
}

/**
 * What the watch's value reads now, and whether `when` holds for it. Fails once the watch was stopped or
 * replaced.
 */
export async function watchedReading(watch: number, when: WhenDef | null): Promise<Live> {
  return invoke<Live>('watched_reading', { watch, when });
}

/** Stops the watch, unless another capture replaced it already. */
export async function stopWatching(watch: number): Promise<void> {
  await invoke('stop_watching', { watch });
}

/** The running windows `window` takes, the one a new session would read first. */
export async function matchWindows(window: WindowDef): Promise<ReadonlyArray<WindowFit>> {
  return invoke<ReadonlyArray<WindowFit>>('match_windows', { window });
}

/** The window a new session would read for `window`, or null while none fits. */
export async function readWindow(window: WindowDef): Promise<number | null> {
  return invoke<number | null>('read_window', { window });
}

/**
 * Keeps `region` of screenshot `screenshot` as a new click picture, comparing only what `selection` picks
 * when it is given. One that no saved automation names goes the next time automations are saved.
 */
export async function addClickPicture(screenshot: number, region: RegionSpec, selection: SelectionSpec | null): Promise<ClickPictureView> {
  return invoke<ClickPictureView>('add_click_picture', { screenshot, region, selection });
}

/** The click picture `image`, at most `maxWidth` wide. */
export async function clickPicture(image: string, maxWidth: number): Promise<Picture | null> {
  return decodePicture(await invoke<ArrayBuffer>('click_picture', { image, maxWidth }));
}

/** Where a click at `at` lands on the picked window's newest frame; rejects with why it lands nowhere. */
export async function findClick(at: ClickAtDef): Promise<FoundClick> {
  return invoke<FoundClick>('find_click', { at });
}

export async function listApps(): Promise<ReadonlyArray<AppSummary>> {
  return invoke<ReadonlyArray<AppSummary>>('list_apps');
}

/** Saves `spec` over the application `existing`, or as a new one; returns its ID. */
export async function saveApp(existing: string | null, spec: AppSpec): Promise<string> {
  return invoke<string>('save_app', { existing, spec });
}

/**
 * Removes an application with its values, unless an automation uses one of them, and closes its overlay.
 * The removal worked when this resolves; the answer is why an overlay window could not open or close, or
 * null.
 */
export async function removeApp(application: string): Promise<string | null> {
  return invoke<string | null>('remove_app', { application });
}

export async function saveValue(spec: SaveSpec): Promise<SavedValue> {
  return invoke<SavedValue>('save_value', { spec });
}

/**
 * Where the saved value starts on a window of `width` by `height` in the fix flow, or null when it would
 * not fit.
 */
export async function placeValue(application: string, value: string, width: number, height: number): Promise<RegionSpec | null> {
  return invoke<RegionSpec | null>('place_value', { application, value, width, height });
}

/** Saves a fix of a saved value at the size of the screenshot `spec` names. */
export async function saveFix(spec: FixSpec): Promise<SavedValue> {
  return invoke<SavedValue>('save_fix', { spec });
}

/** Removes the saved value's fix at `width` by `height`. */
export async function removeFix(application: string, value: string, width: number, height: number): Promise<void> {
  await invoke('remove_fix', { application, value, width, height });
}

/** What the application's values read now, for the Values page; the first call starts reading its window. */
export async function liveValues(application: string): Promise<OverlayView> {
  return invoke<OverlayView>('live_values', { application });
}

/** Stops reading values for the Values page. */
export async function stopLiveValues(): Promise<void> {
  await invoke('stop_live_values');
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
export async function setAntiCheat(application: string, antiCheat: AntiCheatDef): Promise<void> {
  await invoke('set_anti_cheat', { application, antiCheat });
}

/** Records the build of `application` that runs now as the one its values were checked against. */
export async function markChecked(application: string): Promise<void> {
  await invoke('mark_checked', { application });
}

export async function listAutomations(): Promise<ReadonlyArray<AutomationSummary>> {
  return invoke<ReadonlyArray<AutomationSummary>>('list_automations');
}

/** Every saved draft, oldest first, including those whose file cannot be read. */
export async function listDrafts(): Promise<ReadonlyArray<DraftSummary>> {
  return invoke<ReadonlyArray<DraftSummary>>('list_drafts');
}

/**
 * Saves `draft` over the draft `id`, or as a new one, and returns its ID. With `screenshots`, it keeps the
 * capture's screenshots with it. Another draft of the same saved item goes, so each has one at most.
 */
export async function saveDraft(id: string | null, draft: DraftDef, screenshots: boolean): Promise<string> {
  return invoke<string>('save_draft', { id, draft, screenshots });
}

/**
 * Makes the screenshots of the draft `id` the capture's, in place of any it took, so each has the index it
 * had when the draft was saved; returns how many there are.
 */
export async function restoreDraftScreenshots(id: string): Promise<number> {
  return invoke<number>('restore_draft_screenshots', { id });
}

/** Deletes the draft `id` with its screenshots. One already gone is fine. */
export async function deleteDraft(id: string): Promise<void> {
  await invoke('delete_draft', { id });
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
 * Turns an application's overlay on or off and sets where it shows. The setting is saved when this
 * resolves; the answer is why an overlay window could not open or close, or null.
 */
export async function setOverlay(application: string, enabled: boolean, mode: OverlayModeDef): Promise<string | null> {
  return invoke<string | null>('set_overlay', { application, enabled, mode });
}

/** What an application's overlay shows now. */
export async function overlayState(application: string): Promise<OverlayView> {
  return invoke<OverlayView>('overlay_state', { application });
}

/**
 * Tells the setup app the size of an overlay's card in CSS pixels, which its window over the application
 * takes.
 */
export async function overlayCard(application: string, width: number, height: number): Promise<void> {
  await invoke('overlay_card', { application, width, height });
}

/** The saved settings, or the defaults and why the file cannot be read. */
export async function loadSettings(): Promise<SettingsView> {
  return invoke<SettingsView>('load_settings');
}

/** Saves every setting; rejects with what to fix. */
export async function saveSettings(settings: SettingsFile): Promise<void> {
  await invoke('save_settings', { settings });
}

/** Whether the runtime starts when the user signs in to Windows. */
export async function startsWithWindows(): Promise<boolean> {
  return invoke<boolean>('starts_with_windows');
}

export async function setStartsWithWindows(on: boolean): Promise<void> {
  await invoke('set_starts_with_windows', { on });
}

/** The folder the runtime writes its trace files to. */
export async function traceFolder(): Promise<string> {
  return invoke<string>('trace_folder');
}

/** Opens the trace folder in File Explorer. */
export async function openTraceFolder(): Promise<void> {
  await invoke('open_trace_folder');
}

/** The newer release on GitHub, or null when this one is the newest or this build does not update itself. */
export async function checkForUpdate(): Promise<UpdateOffer | null> {
  return invoke<UpdateOffer | null>('check_for_update');
}

/**
 * Downloads the newest release and checks its signature; AppyTappy Setup keeps it until
 * {@link installUpdate}. The release it downloaded can be newer than the one offered, if one came out
 * since. It rejects when the check or the download fails.
 */
export async function downloadUpdate(): Promise<UpdateOffer> {
  return invoke<UpdateOffer>('download_update');
}

/**
 * Runs the downloaded installer for `version`, which closes AppyTappy Setup and the background part,
 * updates both, and starts them again. It rejects when the update fails before the installer starts, and
 * never settles otherwise.
 */
export async function installUpdate(version: string): Promise<void> {
  await invoke('install_update', { version });
}
