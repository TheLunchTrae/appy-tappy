// Generated from the setup app's command table by its bindings test. Do not edit it;
// change the table in `commands/table.rs` and run `UPDATE_BINDINGS=1 cargo test -p appytappy-setup bindings`.

import { invoke } from '@tauri-apps/api/core';

import type {
  AntiCheatDef,
  AppSpec,
  AppSummary,
  AppWindows,
  AutomationDef,
  AutomationSummary,
  BundleSpec,
  CaptureOutline,
  CheckSpec,
  ClickAtDef,
  ClickPictureView,
  ClickSent,
  ClickSpec,
  ColorSpec,
  DebugOverview,
  EditPreview,
  EditSpec,
  FixSpec,
  FoundClick,
  Live,
  LogName,
  OverlayModeDef,
  OverlayView,
  Reading,
  ReadingDetails,
  RecordingView,
  ReferenceSaveSpec,
  RegionSpec,
  RowFilter,
  SaveSpec,
  SavedReference,
  SavedValue,
  SelectionSpec,
  SelectionView,
  SettingsFile,
  SettingsView,
  SizeDef,
  TraceFile,
  TraceMetrics,
  TraceRange,
  TraceRows,
  UpdateOffer,
  WhenDef,
  WindowDef,
  WindowFit,
  WindowList,
  WindowScore,
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

/** How the check reads the newest frame, for the debug details, or null without a check or a frame. */
export async function explainReading(): Promise<ReadingDetails | null> {
  return invoke<ReadingDetails | null>('explain_reading');
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
 * What the runtime copies out of each frame of a saved application's window at `width` by `height`, and
 * where each of its saved values reads there.
 */
export async function captureOutline(application: string, width: number, height: number): Promise<CaptureOutline> {
  return invoke<CaptureOutline>('capture_outline', { application, width, height });
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

/**
 * How the watch's value reads now, for the debug details, or null while it does not read. Fails once the
 * watch was stopped or replaced.
 */
export async function explainWatched(watch: number): Promise<ReadingDetails | null> {
  return invoke<ReadingDetails | null>('explain_watched', { watch });
}

/** Stops the watch, unless another capture replaced it already. */
export async function stopWatching(watch: number): Promise<void> {
  await invoke('stop_watching', { watch });
}

/** The running windows `window` takes, the one a new session would read first. */
export async function matchWindows(window: WindowDef): Promise<ReadonlyArray<WindowFit>> {
  return invoke<ReadonlyArray<WindowFit>>('match_windows', { window });
}

/** How each open window compares with `window`: which fields match, and how the match takes it. */
export async function windowScores(window: WindowDef): Promise<ReadonlyArray<WindowScore>> {
  return invoke<ReadonlyArray<WindowScore>>('window_scores', { window });
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

/**
 * Sends `click` now, once: one real click in its application's window, placed and sent the way the
 * background part does both. Call it only when the user asks for the click. It resolves with the window
 * the click went to once it is sent, which does not say that the application acted on it, and rejects
 * with why it was not sent. The window is the one the application's window rules find now, which may be
 * another one than the picked window.
 */
export async function clickNow(click: ClickSpec): Promise<ClickSent> {
  return invoke<ClickSent>('click_now', { click });
}

export async function listApps(): Promise<ReadonlyArray<AppSummary>> {
  return invoke<ReadonlyArray<AppSummary>>('list_apps');
}

/**
 * The saved applications and the open windows, with the windows each application's match takes, from
 * one listing of the desktop, so the three agree. Rejects when the windows cannot be listed.
 */
export async function listAppWindows(): Promise<AppWindows> {
  return invoke<AppWindows>('list_app_windows');
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
 * Reads the live frame with a saved value as `spec` edits it, as it would be read at the window's size
 * now, and answers what saving the edit would do. Nothing is saved. Rejects with what to fix first.
 */
export async function tryEdit(spec: EditSpec): Promise<EditPreview> {
  return invoke<EditPreview>('try_edit', { spec });
}

/**
 * Saves a saved value as `spec` edits it, under the ID it has. `dropFixes` is the window sizes of the fixes
 * the user agreed to lose: an edit that leaves other fixes unusable than exactly those is refused, as is
 * one that would break an automation.
 */
export async function editValue(spec: EditSpec, dropFixes: ReadonlyArray<SizeDef>): Promise<SavedValue> {
  return invoke<SavedValue>('edit_value', { spec, dropFixes });
}

/**
 * The size of the picked window on its newest frame, which an edit is tried at, or null before one
 * arrives.
 */
export async function windowSize(): Promise<SizeDef | null> {
  return invoke<SizeDef | null>('window_size');
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

/** Whether the runtime runs, and the newest trace file read so far, for the Debug page. */
export async function debugOverview(): Promise<DebugOverview> {
  return invoke<DebugOverview>('debug_overview');
}

/** The trace files, newest first. */
export async function traceFiles(): Promise<ReadonlyArray<TraceFile>> {
  return invoke<ReadonlyArray<TraceFile>>('trace_files');
}

/**
 * The metrics of the trace file `file` over `range`. Each call reads on from where the last one on that
 * file stopped; `progress` says how much is read, and the page asks again at once while it is less than
 * the file.
 */
export async function traceMetrics(file: string, range: TraceRange): Promise<TraceMetrics> {
  return invoke<TraceMetrics>('trace_metrics', { file, range });
}

/** The rows of the trace file `file` that `filter` picks, read on like {@link traceMetrics}. */
export async function traceRows(file: string, filter: RowFilter): Promise<TraceRows> {
  return invoke<TraceRows>('trace_rows', { file, filter });
}

/** Lets go of the trace file the metrics and rows read. */
export async function closeTrace(): Promise<void> {
  await invoke('close_trace');
}

/** The end of `log`, whole lines only, or an empty string before anything was logged. */
export async function readLog(log: LogName): Promise<string> {
  return invoke<string>('read_log', { log });
}

/** Opens the logs folder in File Explorer. */
export async function openLogsFolder(): Promise<void> {
  await invoke('open_logs_folder');
}

/**
 * The debug bundle, as the bytes of a zip file, with the pictures and the newest trace if `contents`
 * asks for them.
 */
export async function debugBundle(contents: BundleSpec): Promise<Blob> {
  return new Blob([await invoke<ArrayBuffer>('debug_bundle', { contents })]);
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
