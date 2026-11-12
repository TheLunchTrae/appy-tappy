// Generated from the setup app's Rust types by its bindings test. Do not edit it;
// change the Rust types and run `UPDATE_BINDINGS=1 cargo test -p appytappy-setup bindings`.

/** `T` with every property and array readonly, all the way down. */
type DeepReadonly<T> = T extends object ? { readonly [K in keyof T]: DeepReadonly<T[K]> } : T;

/**
 * One action, as the file spells it.
 */
export type ActionDef = DeepReadonly<{ "kind": "media",
/**
 * What to do.
 */
command: MediaCommandDef,
/**
 * The executable whose media session to control, such as
 * `Player.exe`, or `None` for the session Windows considers current.
 */
app: string | null, } | { "kind": "volume",
/**
 * The level, from 0 to 100.
 */
percent: number,
/**
 * The executable whose volume to set, such as `Player.exe`, or
 * `None` for the whole system.
 */
app: string | null, } | { "kind": "keys",
/**
 * The keys, such as `Ctrl+Shift+M`; [`KeyChord::parse`] reads them.
 */
keys: string, } | { "kind": "open",
/**
 * A full path or an `http` or `https` link; [`OpenTarget::parse`]
 * reads it.
 */
target: string, } | { "kind": "sound",
/**
 * The file's full path; [`SoundFile::parse`] reads it.
 */
file: string, } | { "kind": "click",
/**
 * The ID of the saved application it clicks in.
 */
app: string,
/**
 * Which button, and how many times.
 */
button: ClickButtonDef,
/**
 * Whether the window that was in front gets the focus back after.
 */
returnFocus: boolean,
/**
 * Where it clicks.
 */
at: ClickAtDef, }>;

/**
 * [`AntiCheatClass`] as the file spells it.
 */
export type AntiCheatDef = DeepReadonly<"none" | "userMode" | "kernelLevel" | "unknown">;

/**
 * One application's authoring file.
 */
export type AppFile = DeepReadonly<{
/**
 * The format version, [`FORMAT`].
 */
format: number,
/**
 * The application's name as the user sees it, such as `Example App`.
 */
name: string,
/**
 * How to recognize the application's window.
 */
window: WindowDef,
/**
 * How often its window changes size, which says whether its values
 * scale with the window. [`SizeChangesDef::Often`] when the file leaves
 * it out.
 */
sizeChanges: SizeChangesDef,
/**
 * How the application protects itself from other programs.
 */
antiCheat: AntiCheatDef,
/**
 * The values read from the application's window.
 */
values: Array<ValueDef>,
/**
 * The characters its number values are read with. Always written,
 * even when empty, since the setup app reads the file's JSON as is.
 */
references: Array<ReferenceDef>,
/**
 * How the setup app shows the values' live readings on top of the
 * application, or `None` when it never has.
 */
overlay?: OverlayDef,
/**
 * The build of the application its values were last checked against,
 * or `None` before the setup app saved a value of it.
 */
checked?: CheckedDef, }>;

/**
 * A saved application with the open windows its match takes.
 */
export type AppFits = DeepReadonly<{
/**
 * The application.
 */
app: AppSummary,
/**
 * The open windows its match takes, the one a new session would read
 * first; none for a broken file.
 */
fits: Array<WindowFit>, }>;

/**
 * An application's name, how to find its window, and how often that window
 * changes size, as the editor sends them.
 */
export type AppSpec = DeepReadonly<{
/**
 * The name the user sees, such as `Example App`.
 */
name: string,
/**
 * How to recognize the application's window.
 */
window: WindowDef,
/**
 * How often its window changes size, which decides whether its values
 * scale to sizes they were not drawn at.
 */
sizeChanges: SizeChangesDef, }>;

/**
 * A saved application, as the Values page lists it.
 */
export type AppSummary = DeepReadonly<{
/**
 * The application's ID.
 */
id: string,
/**
 * The file, or `None` when it is broken.
 */
file: AppFile | null,
/**
 * Why the file is broken.
 */
error: string | null,
/**
 * The handles of the running windows that match, the best fit first.
 */
windows: Array<number>,
/**
 * The build the best fit's executable runs, or `None` while no window
 * matches or its build cannot be read.
 */
running: BuildDef | null, }>;

/**
 * The saved applications and the open windows, from one listing of the
 * desktop, as the picker of a value or reference shows them.
 */
export type AppWindows = DeepReadonly<{
/**
 * The open windows, which every application's fits are among.
 */
open: WindowList,
/**
 * Each saved application.
 */
apps: Array<AppFits>, }>;

/**
 * One automation: a condition on a value, the actions to run in order when
 * it becomes true, and whether to undo them when it stops being true.
 */
export type AutomationDef = DeepReadonly<{
/**
 * The automation's ID, unique within the file, such as `pause_music`.
 */
id: string,
/**
 * The automation's name as the user sees it, such as `Pause music`.
 */
name: string,
/**
 * Whether the runtime runs it.
 */
enabled: boolean,
/**
 * The condition that makes it run.
 */
when: WhenDef,
/**
 * The actions, run in order.
 */
actions: Array<ActionDef>,
/**
 * Whether to undo the actions when the condition stops being true.
 */
undo: boolean,
/**
 * How long after running it cannot run again, in milliseconds.
 */
cooldownMs: number, }>;

/**
 * A saved automation, as the Automations page lists it.
 */
export type AutomationSummary = DeepReadonly<{
/**
 * The automation.
 */
automation: AutomationDef,
/**
 * Why the runtime cannot run it as saved, such as a value that was
 * removed since.
 */
problem: string | null, }>;

/**
 * One build of an application, as its executable file tells it apart.
 *
 * Many applications keep the same file version across updates, so the
 * file's size and the time it was written count too: two builds are the
 * same only when all three match.
 */
export type BuildDef = DeepReadonly<{
/**
 * The executable's file version, such as `1.4.2.0`, or `None` when the
 * file has none.
 */
version?: string,
/**
 * The executable's size in bytes.
 */
size: number,
/**
 * When the executable was last written, in seconds since 1970 began in
 * UTC.
 */
modified: number, }>;

/**
 * How a value is read from its region.
 */
export type CheckDef = DeepReadonly<{ "kind": "number",
/**
 * The [`ReferenceDef::id`] of the reference to read with.
 */
reference: string,
/**
 * Whether only the whole part counts: every digit after the decimal
 * point is ignored, so `87.5%` reads as 87.
 */
whole: boolean, } | { "kind": "barFill",
/**
 * The bar's filled color.
 */
filled: ColorDef,
/**
 * The bar's empty color.
 */
empty: ColorDef, } | { "kind": "color",
/**
 * The color to look for.
 */
color: ColorDef,
/**
 * How far the average may stray from it.
 */
tolerance: number, } | { "kind": "state",
/**
 * The looks, in the order the setup app lists them. A tie goes to
 * the earlier state.
 */
states: Array<StateDef>,
/**
 * How far, on average, the region's red, green, and blue may stray
 * from a state's picture for it to count.
 */
tolerance: number, }>;

/**
 * A check as the UI describes it.
 */
export type CheckSpec = DeepReadonly<{ "kind": "number", region: RegionSpec, reference: ReferenceRef,
/**
 * Whether digits after the decimal point are ignored.
 */
whole: boolean, } | { "kind": "characters", region: RegionSpec,
/**
 * The screenshot the region was drawn on. Every labeled screenshot
 * and the recording must have its window size, since the region
 * means another spot on a window of another size.
 */
screenshot: number, reference: ReferenceRef | null, ink: ColorSpec, tolerance: number, labels: Array<Label>,
/**
 * Readings of the session's recording, which must be of `region`.
 */
recorded: Array<RecordedLabel>, } | { "kind": "barFill", region: RegionSpec, filled: ColorSpec, empty: ColorSpec, } | { "kind": "color", region: RegionSpec, color: ColorSpec, tolerance: number, } | { "kind": "state", region: RegionSpec,
/**
 * The screenshot the region was drawn on; every state's screenshot
 * must have its window size.
 */
drawnOn: number, states: Array<StateSpec>, tolerance: number, }>;

/**
 * The build of an application that its values were checked against, and
 * when.
 *
 * An update can move or restyle what a value reads, so the setup app warns
 * when the running build is another one, and the runtime notes it in its
 * log.
 */
export type CheckedDef = DeepReadonly<{
/**
 * The build.
 */
build: BuildDef,
/**
 * When the user checked them, in seconds since 1970 began in UTC.
 */
at: number, }>;

/**
 * Where a click lands, as the file spells it.
 */
export type ClickAtDef = DeepReadonly<{ "kind": "spot",
/**
 * The corner it is measured from.
 */
corner: CornerDef,
/**
 * How far across from the corner, in pixels.
 */
x: number,
/**
 * How far up or down from the corner, in pixels.
 */
y: number, } | { "kind": "picture",
/**
 * The picture's file name among the click pictures.
 */
image: string,
/**
 * The window's size when the picture was taken.
 */
windowSize: SizeDef,
/**
 * The area of the window the picture is looked for in.
 */
search: RegionDef,
/**
 * Where on the picture to press, from its top-left corner.
 */
press: PointDef,
/**
 * How far, on average, each compared pixel's red, green, and blue
 * may be from the picture's.
 */
tolerance: number, }>;

/**
 * [`ClickButton`] as the file spells it.
 */
export type ClickButtonDef = DeepReadonly<"left" | "right" | "double">;

/**
 * A click picture just kept, and the size of the window it was taken on.
 */
export type ClickPictureView = DeepReadonly<{
/**
 * Its file name among the click pictures.
 */
image: string,
/**
 * The window's width when it was taken.
 */
windowWidth: number,
/**
 * The window's height when it was taken.
 */
windowHeight: number, }>;

/**
 * A color.
 */
export type ColorDef = DeepReadonly<{
/**
 * The red channel.
 */
red: number,
/**
 * The green channel.
 */
green: number,
/**
 * The blue channel.
 */
blue: number, }>;

/**
 * A color the user picked on a screenshot.
 */
export type ColorSpec = DeepReadonly<{ red: number, green: number, blue: number, }>;

/**
 * A color as the UI describes one.
 */
export type ColorView = DeepReadonly<{ red: number, green: number, blue: number, }>;

/**
 * How a step's pick combines with the selection before it.
 */
export type Combine = DeepReadonly<"replace" | "add" | "remove">;

/**
 * [`Comparison`] as the file spells it.
 */
export type ComparisonDef = DeepReadonly<"below" | "atMost" | "equals" | "notEquals" | "atLeast" | "above">;

/**
 * [`Corner`] as the file spells it.
 */
export type CornerDef = DeepReadonly<"topLeft" | "topRight" | "bottomLeft" | "bottomRight">;

/**
 * [`WindowMatch::fallback`] as the file spells it.
 */
export type FallbackDef = DeepReadonly<"none" | "executable">;

/**
 * A value drawn again at one window size, which it reads there exactly
 * instead of scaling its first drawing.
 */
export type FixDef = DeepReadonly<{
/**
 * The window's size when it was drawn.
 */
windowSize: SizeDef,
/**
 * The region at that size.
 */
region: RegionDef,
/**
 * For a state check, each state's picture at this size, in the order
 * of the check's states; empty for any other check. Always written.
 */
images: Array<string>,
/**
 * For a number check, the [`ReferenceDef::id`] of a reference learned
 * at this size, or `None` to read with the check's own.
 */
reference?: string, }>;

/**
 * A saved value drawn again at another window size, as the UI describes
 * it.
 */
export type FixSpec = DeepReadonly<{
/**
 * The saved application.
 */
app: string,
/**
 * The value's ID.
 */
value: string,
/**
 * The check at this size: of the value's kind, with each of its
 * states in order, or for a number, the reference to read with.
 */
check: CheckSpec,
/**
 * The screenshot the region was drawn on, whose size the fix is for.
 */
screenshot: number, }>;

/**
 * Where a click would land in the window, from its top-left corner, and
 * for a picture how alike the place found is.
 */
export type FoundClick = DeepReadonly<{
/**
 * Across, in pixels.
 */
x: number,
/**
 * Down, in pixels.
 */
y: number,
/**
 * From 0 to 100 percent; `None` for a spot.
 */
likeness: number | null, }>;

/**
 * What one screenshot's region shows, typed by the user so a reference
 * learns its characters.
 */
export type Label = DeepReadonly<{ screenshot: number, text: string, }>;

/**
 * What a watched value reads on the newest frame, for the automation
 * editor.
 */
export type Live = DeepReadonly<{ "kind": "waiting" } | { "kind": "read",
/**
 * What it reads.
 */
reading: Reading,
/**
 * Whether the condition the editor asked about holds for it, or
 * `None` when it asked about none.
 */
holds: boolean | null, } | { "kind": "problem",
/**
 * What is wrong, for the user.
 */
message: string, } | { "kind": "lost",
/**
 * What happened, for the user.
 */
message: string, }>;

/**
 * A recording of looks as the UI shows it.
 */
export type LookRecordingView = DeepReadonly<{
/**
 * Its number in the session.
 */
id: number,
/**
 * Whether it still records.
 */
recording: boolean,
/**
 * How long it has recorded, or recorded in all once stopped.
 */
elapsedMs: number,
/**
 * Why the latest frame could not be recorded, or why it stopped by
 * itself.
 */
problem: string | null,
/**
 * The indexes of the screenshots it kept, in the order their looks
 * first showed.
 */
kept: Array<number>, }>;

/**
 * [`MediaCommand`] as the file spells it.
 */
export type MediaCommandDef = DeepReadonly<"play" | "pause" | "next" | "previous">;

/**
 * [`MustMatch`] as the file spells it.
 */
export type MustMatchDef = DeepReadonly<{
/**
 * The title has to match.
 */
title: boolean,
/**
 * The class has to match.
 */
class: boolean, }>;

/**
 * How the setup app shows an application's live readings on top of it. The
 * runtime ignores it.
 */
export type OverlayDef = DeepReadonly<{
/**
 * Whether it shows.
 */
enabled: boolean,
/**
 * Where it shows.
 */
mode: OverlayModeDef,
/**
 * Where the user last put the picture-in-picture panel, or `None`
 * before they moved it.
 */
panel?: PanelDef, }>;

/**
 * Where the live readings show.
 */
export type OverlayModeDef = DeepReadonly<"overlay" | "pictureInPicture">;

/**
 * An application's overlay, as its window shows it.
 */
export type OverlayView = DeepReadonly<{
/**
 * The application's name.
 */
name: string,
/**
 * Where the readings show.
 */
mode: OverlayModeDef,
/**
 * Why there are no readings, such as that the application is not
 * running, or `None` while there are.
 */
message: string | null,
/**
 * The size of the window the readings are from, or `None` before a
 * frame arrived.
 */
windowSize: SizeDef | null,
/**
 * Each value's name and reading, in the order they were saved.
 */
values: Array<ValueView>, }>;

/**
 * A picture-in-picture panel's place and size in screen pixels.
 */
export type PanelDef = DeepReadonly<{
/**
 * The left edge.
 */
x: number,
/**
 * The top edge.
 */
y: number,
/**
 * The width.
 */
width: number,
/**
 * The height.
 */
height: number, }>;

/**
 * The side or corner of the window a value's region keeps its distance
 * from as the window changes size: one of nine, top, middle or bottom by
 * left, center or right.
 */
export type PinDef = DeepReadonly<"topLeft" | "topCenter" | "topRight" | "middleLeft" | "center" | "middleRight" | "bottomLeft" | "bottomCenter" | "bottomRight">;

/**
 * A point in pixels.
 */
export type PointDef = DeepReadonly<{
/**
 * Across.
 */
x: number,
/**
 * Down.
 */
y: number, }>;

/**
 * How often the runtime reads a value: as often as `fastest_ms` allows while
 * it changes, and at least every `slowest_ms` while it holds still.
 */
export type ReadIntervalDef = DeepReadonly<{
/**
 * The shortest wait between two reads, in milliseconds.
 */
fastestMs: number,
/**
 * The longest wait between two reads, in milliseconds.
 */
slowestMs: number, }>;

/**
 * A value the check read, as the UI shows it.
 */
export type Reading = DeepReadonly<{ "kind": "number", "value": number } | { "kind": "flag", "value": boolean } | { "kind": "state", "value": string }>;

/**
 * What one recorded reading shows, as the user confirmed it in review, so
 * a reference learns its characters like a labeled screenshot's.
 */
export type RecordedLabel = DeepReadonly<{
/**
 * The reading's index in the recording.
 */
reading: number,
/**
 * What it shows.
 */
text: string, }>;

/**
 * One reading as the UI shows it.
 */
export type RecordedReading = DeepReadonly<{
/**
 * When it first showed, from the start of the recording.
 */
firstMs: number,
/**
 * How many frames showed it.
 */
seen: number,
/**
 * What the text reader made of it.
 */
text: RecordedText, }>;

/**
 * What the text reader made of a reading.
 */
export type RecordedText = DeepReadonly<{ "kind": "pending" } | { "kind": "read", "text": string } | { "kind": "failed", "text": string }>;

/**
 * What a recording records: the region of a window of one size, and what
 * counts as ink there.
 */
export type RecordingSettings = DeepReadonly<{
/**
 * The region of the window.
 */
region: RegionView,
/**
 * The characters' color.
 */
ink: ColorView,
/**
 * How far a pixel may be from the characters' color and still be ink.
 */
tolerance: number,
/**
 * The window's size when the region was drawn.
 */
windowSize: SizeView, }>;

/**
 * A recording as the UI shows it.
 */
export type RecordingView = DeepReadonly<{
/**
 * Its number in the session.
 */
id: number,
/**
 * Whether it still records.
 */
recording: boolean,
/**
 * How long it has recorded, or recorded in all once stopped.
 */
elapsedMs: number,
/**
 * Whether it holds [`MAX_READINGS`], so it keeps no more.
 */
full: boolean,
/**
 * Why the latest frame could not be recorded.
 */
problem: string | null,
/**
 * What it records, which going on with it needs again.
 */
settings: RecordingSettings,
/**
 * Its readings, in the order they first showed.
 */
readings: Array<RecordedReading>, }>;

/**
 * The characters of one font at one color, that number values read with.
 *
 * It learns them from pictures of text in that font. One reference serves
 * every value drawn in its font, whatever the size: the reader scales each
 * character to its line's height.
 */
export type ReferenceDef = DeepReadonly<{
/**
 * The reference's ID, unique within the file, such as `percent_font`.
 * Number checks refer to it by this.
 */
id: string,
/**
 * The reference's name as the user sees it, such as `Percent font`.
 */
name: string,
/**
 * The characters' color.
 */
ink: ColorDef,
/**
 * How far a pixel may stray from `ink` and still count.
 */
tolerance: number,
/**
 * The pictures it learns from.
 */
samples: Array<SampleDef>, }>;

/**
 * A saved reference of a saved application.
 */
export type ReferenceRef = DeepReadonly<{
/**
 * The application's ID.
 */
app: string,
/**
 * The reference's ID in the application's file.
 */
id: string, }>;

/**
 * A reference to save, as the UI describes it.
 */
export type ReferenceSaveSpec = DeepReadonly<{
/**
 * The saved application to add the reference to.
 */
app: string,
/**
 * The reference's name, such as `Percent font`.
 */
referenceName: string,
/**
 * The characters it is taught, as a [`CheckSpec::Characters`] that
 * names the reference when it is saved already.
 */
check: CheckSpec, }>;

/**
 * A rectangle in pixels from the window's top-left corner.
 */
export type RegionDef = DeepReadonly<{
/**
 * The left edge.
 */
x: number,
/**
 * The top edge.
 */
y: number,
/**
 * The width.
 */
width: number,
/**
 * The height.
 */
height: number, }>;

/**
 * A rectangle of the window, in pixels from its top-left corner.
 */
export type RegionSpec = DeepReadonly<{ x: number, y: number, width: number, height: number, }>;

/**
 * A region as the UI describes one.
 */
export type RegionView = DeepReadonly<{ x: number, y: number, width: number, height: number, }>;

/**
 * A picture of text in a reference's font and the text it shows.
 */
export type SampleDef = DeepReadonly<{
/**
 * The picture's file name.
 */
image: string,
/**
 * What the picture shows, such as `87%`.
 */
text: string, }>;

/**
 * A value to save, as the UI describes it.
 */
export type SaveSpec = DeepReadonly<{
/**
 * The saved application to add the value to.
 */
app: string,
/**
 * The value's name, such as `Progress`.
 */
valueName: string,
/**
 * The check, with the session's screenshots it names.
 */
check: CheckSpec,
/**
 * The screenshot the region was drawn on, whose size the value keeps.
 */
screenshot: number,
/**
 * The side of the window the region stays at as the window changes
 * size.
 */
pin: PinDef, }>;

/**
 * Where a reference was saved.
 */
export type SavedReference = DeepReadonly<{
/**
 * The application's ID.
 */
app: string,
/**
 * The reference's ID.
 */
reference: string, }>;

/**
 * Where a value was saved.
 */
export type SavedValue = DeepReadonly<{
/**
 * The application's ID.
 */
app: string,
/**
 * The value's ID.
 */
value: string, }>;

/**
 * A value read from a region of the window's frame.
 *
 * Its drawing is the window size it was drawn at, its region, and its
 * check. At any other size it reads its fix for that size, if it has one,
 * or else, while its application's window changes size
 * [often](SizeChangesDef::Often), its region scaled from this drawing and
 * kept at its pin's distance from that side of the window.
 */
export type ScreenDef = DeepReadonly<{
/**
 * The window's size when the region was drawn. A region's pixels mean
 * something only at this size, or scaled from it.
 */
windowSize: SizeDef,
/**
 * The region of the window's frame the check reads.
 */
region: RegionDef,
/**
 * The check.
 */
check: CheckDef,
/**
 * The side of the window the region stays at as the window changes
 * size, or `None` for the side [`PinDef::guess`] picks.
 */
pin?: PinDef,
/**
 * The value drawn again at other window sizes, each read exactly at its
 * own size. Always written, even when empty, since the setup app reads
 * the file's JSON as is.
 */
fixes: Array<FixDef>, }>;

/**
 * One thing the user did to select.
 */
export type SelectStep = DeepReadonly<{ "kind": "snap", region: RegionSpec, combine: Combine, } | { "kind": "click", x: number, y: number, combine: Combine, }>;

/**
 * What the user selected, as the steps they took.
 */
export type SelectionSpec = DeepReadonly<{
/**
 * How far a color may be from another and still count as alike.
 */
tolerance: number,
/**
 * The steps, in the order the user took them.
 */
steps: Array<SelectStep>, }>;

/**
 * A selection as the UI draws it.
 */
export type SelectionView = DeepReadonly<{
/**
 * The box around the selected pixels.
 */
region: RegionSpec,
/**
 * The runs of selected pixels in the box, each as the index of its
 * first pixel and of the pixel after its last, row by row from the top.
 */
spans: Array<[number, number]>, }>;

/**
 * Every setting.
 */
export type SettingsFile = DeepReadonly<{
/**
 * The format version, [`FORMAT`].
 */
format: number,
/**
 * How often the runtime reads a value.
 */
readInterval: ReadIntervalDef,
/**
 * The global shortcut that takes a screenshot in the setup app, as an
 * accelerator such as `F9` or `Control+Shift+S`.
 */
screenshotShortcut: string,
/**
 * The runtime trace, which records what the runtime reads and does.
 */
trace: TraceDef,
/**
 * Whether the runtime starts with automations enabled or disabled as
 * the tray last left them. Off, it starts with them enabled.
 */
rememberEnabled: boolean, }>;

/**
 * The settings as the Settings page shows them.
 */
export type SettingsView = DeepReadonly<{
/**
 * The saved settings, or the defaults when the file cannot be read.
 */
settings: SettingsFile,
/**
 * Why the saved settings cannot be read, in which case the page shows
 * the defaults and saving replaces the file.
 */
problem: string | null, }>;

/**
 * How often an application's window changes size, as the user answered.
 */
export type SizeChangesDef = DeepReadonly<"often" | "rarely">;

/**
 * A size in pixels.
 */
export type SizeDef = DeepReadonly<{
/**
 * The width.
 */
width: number,
/**
 * The height.
 */
height: number, }>;

/**
 * A window's size as the UI describes one.
 */
export type SizeView = DeepReadonly<{ width: number, height: number, }>;

/**
 * One look a state check knows, such as a corner showing `Paused`.
 */
export type StateDef = DeepReadonly<{
/**
 * The state's name, which the value reads when the region looks like
 * it. [`StateName::new`] says which names are allowed.
 */
name: string,
/**
 * The file name of the region's picture in this state. Its transparent
 * pixels are the background, which the check ignores.
 */
image: string, }>;

/**
 * One state of a state check, as the user set it up.
 */
export type StateSpec = DeepReadonly<{
/**
 * The name the value reads when the region looks like this.
 */
name: string,
/**
 * The screenshot whose region shows this state.
 */
screenshot: number,
/**
 * The thing to compare, selected on the screenshot; without one the
 * whole region is compared.
 */
selection?: SelectionSpec, }>;

/**
 * What a value is compared with: a number, yes or no, or a state's name.
 */
export type TargetDef = DeepReadonly<number | boolean | string>;

/**
 * What a trace can record.
 */
export type TraceCategory = DeepReadonly<"readings" | "changes" | "rules" | "actions" | "lifecycle">;

/**
 * The runtime trace: whether the runtime records what it reads and does to
 * a file, what it records, and how much disk the files may take.
 */
export type TraceDef = DeepReadonly<{
/**
 * Whether the runtime records.
 */
on: boolean,
/**
 * When recording ends, as Unix time in milliseconds, or `None` to
 * record until it is turned off. The setup app sets it when the trace
 * is turned on, so a forgotten trace ends without the runtime writing
 * this file.
 */
until: number | null,
/**
 * What the runtime records.
 */
record: Array<TraceCategory>,
/**
 * The size at which a trace file ends and the trace continues in a new
 * one, in megabytes.
 */
fileLimitMb: number,
/**
 * How many trace files to keep; the oldest beyond this are deleted.
 */
keepFiles: number, }>;

/**
 * A newer release the user can install.
 */
export type UpdateOffer = DeepReadonly<{
/**
 * Its version, such as `0.2.0`.
 */
version: string, }>;

/**
 * One value read from the application's window.
 */
export type ValueDef = DeepReadonly<{
/**
 * The value's signal name, unique within the file, such as
 * `progress`. Automations refer to the value by it.
 */
id: string,
/**
 * The value's name as the user sees it, such as `Progress`.
 */
name: string,
/**
 * Where on the window the value is read and how.
 */
screen: ScreenDef, }>;

/**
 * One value's line in the overlay.
 */
export type ValueView = DeepReadonly<{
/**
 * The value's ID.
 */
id: string,
/**
 * The value's name.
 */
name: string,
/**
 * Its reading, or `None` when there is none.
 */
reading: Reading | null,
/**
 * Why it has no reading, or `None` when it has one or no frame has
 * arrived yet.
 */
problem: string | null, }>;

/**
 * A comparison of one saved value with a target.
 */
export type WhenDef = DeepReadonly<{
/**
 * The ID of the value's application, such as `example`.
 */
app: string,
/**
 * The value's ID within that application, such as `progress`.
 */
value: string,
/**
 * How the value relates to the target.
 */
is: ComparisonDef,
/**
 * What the value is compared with.
 */
target: TargetDef, }>;

/**
 * A window the user can pick.
 */
export type WindowChoice = DeepReadonly<{
/**
 * The window's handle, which the capture takes.
 */
handle: number,
/**
 * The window's title.
 */
title: string,
/**
 * The window's class name, which a window match can use.
 */
class: string,
/**
 * The file name of the window's executable, such as `Example.exe`.
 */
executable: string,
/**
 * The window's width on screen, in pixels.
 */
width: number,
/**
 * The window's height on screen, in pixels.
 */
height: number, }>;

/**
 * How to recognize an application's window: see [`WindowMatch`].
 */
export type WindowDef = DeepReadonly<{
/**
 * The pattern for the window's title.
 */
title: string,
/**
 * The pattern for the window's class name.
 */
class: string,
/**
 * The pattern for the file name of the window's executable, which
 * always has to match.
 */
executable: string,
/**
 * The fields besides the executable a window has to match to qualify.
 */
mustMatch: MustMatchDef,
/**
 * What to take when no window qualifies.
 */
fallback: FallbackDef, }>;

/**
 * A running window that an application's window match takes.
 */
export type WindowFit = DeepReadonly<{
/**
 * The window's handle.
 */
handle: number,
/**
 * Whether it is taken only because no window qualifies.
 */
fallback: boolean,
/**
 * Whether it is the window a new session would read now.
 */
best: boolean,
/**
 * Whether it fits as well as the window a new session would read. A
 * session reads one of these, so a value can be drawn on any of them.
 * Only offered windows count: one the list leaves out ties with none.
 */
tied: boolean, }>;

/**
 * The windows to offer, with the anti-cheat products running now.
 */
export type WindowList = DeepReadonly<{
/**
 * The windows the user can pick, sorted by title.
 */
windows: Array<WindowChoice>,
/**
 * The anti-cheat products whose processes run now, sorted and without
 * repeats. A running product may protect any application, so the list is a
 * warning, not a classification.
 */
antiCheat: Array<string>, }>;

/**
 * The longest app name a media or volume action names and the longest path or link, the longest cooldown, the
 * most states, the settings' ranges, the highest tolerance, and the most pixels a click's search area holds, as the
 * saved files take them.
 */
export const LIMITS = {
  "fastestReadMs": 1,
  "maxActionAppBytes": 256,
  "maxCooldownMs": 86400000,
  "maxSearchPixels": 160000,
  "maxStates": 16,
  "maxTargetBytes": 2048,
  "maxTolerance": 255,
  "maxTraceFileMb": 1000,
  "maxTraceFiles": 100,
  "slowestReadMs": 10000
} as const;

/** What a state check reads when its region shows none of its states. */
export const NO_STATE = "none";

/** The keys a key press action can name: modifiers in the order they go down, named keys, and function keys from F1. */
export const KEY_NAMES = {
  "maxFunctionKey": 24,
  "modifiers": [
    "Ctrl",
    "Alt",
    "Shift",
    "Win"
  ],
  "named": [
    "Space",
    "Enter",
    "Tab",
    "Escape",
    "Backspace",
    "Insert",
    "Delete",
    "Home",
    "End",
    "PageUp",
    "PageDown",
    "Up",
    "Down",
    "Left",
    "Right"
  ]
} as const;

/**
 * The keys a screenshot shortcut can name: modifiers in the order they are written, named keys, the named keys
 * that need no modifier, and function keys from F1, which need none either.
 */
export const SHORTCUT_KEYS = {
  "alone": [
    "PrintScreen",
    "Pause"
  ],
  "maxFunctionKey": 24,
  "modifiers": [
    "Control",
    "Alt",
    "Shift",
    "Super"
  ],
  "named": [
    "Space",
    "Up",
    "Down",
    "Left",
    "Right",
    "Insert",
    "Delete",
    "Home",
    "End",
    "PageUp",
    "PageDown",
    "PrintScreen",
    "Pause"
  ]
} as const;

/** How an overlay window's label starts, before its application's ID, by its mode. */
export const OVERLAY_LABELS = {
  "overlay": "overlay-",
  "pictureInPicture": "panel-"
} as const satisfies Record<OverlayModeDef, string>;

/**
 * How a picture a command answers starts: the bytes of its little-endian width and height before the RGBA rows.
 * The sample is a picture as the Rust side encodes it, and what it holds.
 */
export const PICTURES = {
  "headerBytes": 8,
  "sample": {
    "encoded": [
      2,
      0,
      0,
      0,
      1,
      0,
      0,
      0,
      30,
      20,
      10,
      255,
      60,
      50,
      40,
      255
    ],
    "height": 1,
    "rgba": [
      30,
      20,
      10,
      255,
      60,
      50,
      40,
      255
    ],
    "width": 2
  }
} as const;

/** Whether each kind of action sends input, and whether undo can put it back, by kind and a media action's command. */
export const ACTIONS = {
  "injectsInput": {
    "click": true,
    "keys": true,
    "media": false,
    "open": false,
    "sound": false,
    "volume": false
  },
  "undoable": {
    "click": false,
    "keys": false,
    "media.next": false,
    "media.pause": true,
    "media.play": true,
    "media.previous": false,
    "open": false,
    "sound": false,
    "volume": true
  }
} as const;

/** Every trace category, in the order the page lists them. */
export const TRACE_CATEGORIES = [
  "readings",
  "changes",
  "rules",
  "actions",
  "lifecycle"
] as const satisfies readonly TraceCategory[];

/**
 * What the Rust side makes of sample paths, links, key presses, screenshot shortcuts and app names: the text it
 * keeps, or null when it refuses one; and where sample spots land in sample windows. The page's own checks must read
 * each the same.
 */
export const PARSE_CASES = {
  "appName": [
    {
      "read": "Player.exe",
      "text": "Player.exe"
    },
    {
      "read": " Player.exe ",
      "text": " Player.exe "
    },
    {
      "read": "\ufeffPlayer.exe",
      "text": "\ufeffPlayer.exe"
    },
    {
      "read": null,
      "text": ""
    },
    {
      "read": null,
      "text": "Player\n.exe"
    },
    {
      "read": null,
      "text": "\u0007"
    },
    {
      "read": null,
      "text": "Player\u0085.exe"
    },
    {
      "read": null,
      "text": "\u00e9\u00e9aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    },
    {
      "read": "\u00e9\u00e9aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "text": "\u00e9\u00e9aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    }
  ],
  "keys": [
    {
      "read": "Ctrl+Shift+M",
      "text": "Ctrl+Shift+M"
    },
    {
      "read": "Ctrl+Shift+M",
      "text": "shift + ctrl + m"
    },
    {
      "read": "Alt+Win+5",
      "text": "Win+Alt+5"
    },
    {
      "read": "Ctrl+Alt+Shift+Win+M",
      "text": "Ctrl+Shift+Alt+Win+M"
    },
    {
      "read": "F9",
      "text": "F9"
    },
    {
      "read": "F1",
      "text": "f1"
    },
    {
      "read": "F24",
      "text": "F24"
    },
    {
      "read": "Ctrl+PageDown",
      "text": "ctrl+pagedown"
    },
    {
      "read": "Space",
      "text": "Space"
    },
    {
      "read": "A",
      "text": "a"
    },
    {
      "read": "Ctrl+M",
      "text": "Ctrl+\u00a0M\u3000"
    },
    {
      "read": null,
      "text": ""
    },
    {
      "read": null,
      "text": "Ctrl"
    },
    {
      "read": null,
      "text": "Ctrl+"
    },
    {
      "read": null,
      "text": "Ctrl+Ctrl+M"
    },
    {
      "read": null,
      "text": "M+Ctrl"
    },
    {
      "read": null,
      "text": "M+N"
    },
    {
      "read": null,
      "text": "F0"
    },
    {
      "read": null,
      "text": "F25"
    },
    {
      "read": null,
      "text": "F01"
    },
    {
      "read": null,
      "text": "F+1"
    },
    {
      "read": null,
      "text": "F256"
    },
    {
      "read": null,
      "text": "Ctrl+Shift+Alt+Win+Meta+M"
    },
    {
      "read": null,
      "text": "\u00c4"
    },
    {
      "read": null,
      "text": "PrintScreen"
    },
    {
      "read": null,
      "text": "Bac\u212aspace"
    },
    {
      "read": null,
      "text": "\u017fhift+M"
    },
    {
      "read": null,
      "text": "\ufeffF9"
    },
    {
      "read": null,
      "text": "M+"
    },
    {
      "read": null,
      "text": "+M"
    },
    {
      "read": null,
      "text": "Ctrl++M"
    },
    {
      "read": "F",
      "text": "F"
    },
    {
      "read": null,
      "text": "F9x"
    },
    {
      "read": null,
      "text": "F1+F2"
    },
    {
      "read": "Up",
      "text": "Up"
    },
    {
      "read": null,
      "text": "ArrowUp"
    },
    {
      "read": null,
      "text": "Control+M"
    }
  ],
  "openTarget": [
    {
      "read": "C:\\Tools\\Timer.exe",
      "text": "C:\\Tools\\Timer.exe"
    },
    {
      "read": "d:/notes/today.txt",
      "text": "  d:/notes/today.txt "
    },
    {
      "read": "C:\\Timer.exe",
      "text": "\u00a0C:\\Timer.exe\u3000"
    },
    {
      "read": "https://example.com/a?b=c",
      "text": "https://example.com/a?b=c"
    },
    {
      "read": "HTTP://example.com",
      "text": "HTTP://example.com"
    },
    {
      "read": "http://x",
      "text": "http://x"
    },
    {
      "read": "C:\\",
      "text": "C:\\"
    },
    {
      "read": "https://a\ufeffb",
      "text": "https://a\ufeffb"
    },
    {
      "read": null,
      "text": ""
    },
    {
      "read": null,
      "text": "   "
    },
    {
      "read": null,
      "text": "\ufeffC:\\Timer.exe"
    },
    {
      "read": null,
      "text": "Timer.exe"
    },
    {
      "read": null,
      "text": "Tools\\Timer.exe"
    },
    {
      "read": null,
      "text": "C:Timer.exe"
    },
    {
      "read": null,
      "text": "1:/x"
    },
    {
      "read": null,
      "text": "\u212a:\\Timer.exe"
    },
    {
      "read": null,
      "text": "https://"
    },
    {
      "read": null,
      "text": "https://example.com/a b"
    },
    {
      "read": null,
      "text": "https://a\u3000b"
    },
    {
      "read": null,
      "text": "http\u017f://example.com"
    },
    {
      "read": null,
      "text": "ftp://example.com"
    },
    {
      "read": null,
      "text": "file:///C:/Tools/Timer.exe"
    },
    {
      "read": null,
      "text": "search-ms:query=x"
    },
    {
      "read": null,
      "text": "\\\\server\\share\\run.bat"
    },
    {
      "read": null,
      "text": "\\\\?\\UNC\\server\\share\\run.bat"
    },
    {
      "read": null,
      "text": "C:\\Tools\nTimer.exe"
    },
    {
      "read": null,
      "text": "C:\\Tools\u0085Timer.exe"
    },
    {
      "read": "C:\\\u00e9\u00e9aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.exe",
      "text": "C:\\\u00e9\u00e9aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.exe"
    },
    {
      "read": null,
      "text": "C:\\\u00e9\u00e9aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.exe"
    },
    {
      "read": null,
      "text": "https://\u00e9\u00e9aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    },
    {
      "read": "https://\u00e9\u00e9aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "text": "https://\u00e9\u00e9aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    }
  ],
  "screenshotShortcut": [
    {
      "read": "F9",
      "text": "F9"
    },
    {
      "read": "F1",
      "text": "F1"
    },
    {
      "read": "F24",
      "text": "F24"
    },
    {
      "read": "PrintScreen",
      "text": "PrintScreen"
    },
    {
      "read": "Pause",
      "text": "Pause"
    },
    {
      "read": "Control+Shift+S",
      "text": "Control+Shift+S"
    },
    {
      "read": "Control+Alt+Shift+Super+PrintScreen",
      "text": "Control+Alt+Shift+Super+PrintScreen"
    },
    {
      "read": "Alt+7",
      "text": "Alt+7"
    },
    {
      "read": "Super+Left",
      "text": "Super+Left"
    },
    {
      "read": "Control+Up",
      "text": "Control+Up"
    },
    {
      "read": "Shift+Down",
      "text": "Shift+Down"
    },
    {
      "read": "Alt+Right",
      "text": "Alt+Right"
    },
    {
      "read": "Control+Space",
      "text": "Control+Space"
    },
    {
      "read": "Control+Insert",
      "text": "Control+Insert"
    },
    {
      "read": "Control+Delete",
      "text": "Control+Delete"
    },
    {
      "read": "Control+Home",
      "text": "Control+Home"
    },
    {
      "read": "Control+End",
      "text": "Control+End"
    },
    {
      "read": "Control+PageUp",
      "text": "Control+PageUp"
    },
    {
      "read": "Shift+PageDown",
      "text": "Shift+PageDown"
    },
    {
      "read": "Control+F9",
      "text": "Control+F9"
    },
    {
      "read": "Super+Pause",
      "text": "Super+Pause"
    },
    {
      "read": null,
      "text": ""
    },
    {
      "read": null,
      "text": " F9 "
    },
    {
      "read": null,
      "text": "F0"
    },
    {
      "read": null,
      "text": "F25"
    },
    {
      "read": null,
      "text": "F09"
    },
    {
      "read": null,
      "text": "S"
    },
    {
      "read": null,
      "text": "7"
    },
    {
      "read": null,
      "text": "Space"
    },
    {
      "read": null,
      "text": "Up"
    },
    {
      "read": null,
      "text": "Shift+Control+S"
    },
    {
      "read": null,
      "text": "Super+Alt+S"
    },
    {
      "read": null,
      "text": "Control+Control+S"
    },
    {
      "read": null,
      "text": "Control+"
    },
    {
      "read": null,
      "text": "Control"
    },
    {
      "read": null,
      "text": "Control+s"
    },
    {
      "read": null,
      "text": "Ctrl+S"
    },
    {
      "read": null,
      "text": "Win+S"
    },
    {
      "read": null,
      "text": "Control+Enter"
    },
    {
      "read": null,
      "text": "Control+Bogus"
    },
    {
      "read": null,
      "text": "Bogus"
    }
  ],
  "soundFile": [
    {
      "read": "C:\\Sounds\\Ding.wav",
      "text": "C:\\Sounds\\Ding.wav"
    },
    {
      "read": "c:/sounds/ding.WAV",
      "text": " c:/sounds/ding.WAV "
    },
    {
      "read": "C:\\.wav",
      "text": "C:\\.wav"
    },
    {
      "read": null,
      "text": ""
    },
    {
      "read": null,
      "text": "   "
    },
    {
      "read": null,
      "text": ".wav"
    },
    {
      "read": null,
      "text": "Ding.wav"
    },
    {
      "read": null,
      "text": "C:\\Sounds\\Ding.mp3"
    },
    {
      "read": null,
      "text": "C:\\Sounds\\Ding.wave"
    },
    {
      "read": null,
      "text": "https://example.com/ding.wav"
    },
    {
      "read": null,
      "text": "\\\\server\\sounds\\ding.wav"
    },
    {
      "read": null,
      "text": "\u212a:\\Ding.wav"
    },
    {
      "read": null,
      "text": "\ufeffC:\\Ding.wav"
    },
    {
      "read": null,
      "text": "C:\\Sounds\u0007.wav"
    },
    {
      "read": null,
      "text": "C:\\\u00e9.wa"
    },
    {
      "read": "C:\\\u00e9\u00e9aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.wav",
      "text": "C:\\\u00e9\u00e9aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.wav"
    },
    {
      "read": null,
      "text": "C:\\\u00e9\u00e9aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.wav"
    }
  ],
  "spotPlacement": [
    {
      "at": {
        "corner": "topLeft",
        "x": 0,
        "y": 0
      },
      "place": {
        "x": 0,
        "y": 0
      },
      "window": {
        "height": 720,
        "width": 1280
      }
    },
    {
      "at": {
        "corner": "topLeft",
        "x": 9,
        "y": 15
      },
      "place": {
        "x": 9,
        "y": 15
      },
      "window": {
        "height": 720,
        "width": 1280
      }
    },
    {
      "at": {
        "corner": "topLeft",
        "x": 1279,
        "y": 719
      },
      "place": {
        "x": 1279,
        "y": 719
      },
      "window": {
        "height": 720,
        "width": 1280
      }
    },
    {
      "at": {
        "corner": "topLeft",
        "x": 1280,
        "y": 0
      },
      "place": null,
      "window": {
        "height": 720,
        "width": 1280
      }
    },
    {
      "at": {
        "corner": "topLeft",
        "x": 0,
        "y": 720
      },
      "place": null,
      "window": {
        "height": 720,
        "width": 1280
      }
    },
    {
      "at": {
        "corner": "topLeft",
        "x": 0,
        "y": 0
      },
      "place": {
        "x": 0,
        "y": 0
      },
      "window": {
        "height": 1,
        "width": 1
      }
    },
    {
      "at": {
        "corner": "topLeft",
        "x": 0,
        "y": 0
      },
      "place": null,
      "window": {
        "height": 0,
        "width": 0
      }
    },
    {
      "at": {
        "corner": "topRight",
        "x": 0,
        "y": 0
      },
      "place": {
        "x": 1279,
        "y": 0
      },
      "window": {
        "height": 720,
        "width": 1280
      }
    },
    {
      "at": {
        "corner": "topRight",
        "x": 9,
        "y": 15
      },
      "place": {
        "x": 1270,
        "y": 15
      },
      "window": {
        "height": 720,
        "width": 1280
      }
    },
    {
      "at": {
        "corner": "topRight",
        "x": 1279,
        "y": 719
      },
      "place": {
        "x": 0,
        "y": 719
      },
      "window": {
        "height": 720,
        "width": 1280
      }
    },
    {
      "at": {
        "corner": "topRight",
        "x": 1280,
        "y": 0
      },
      "place": null,
      "window": {
        "height": 720,
        "width": 1280
      }
    },
    {
      "at": {
        "corner": "topRight",
        "x": 0,
        "y": 720
      },
      "place": null,
      "window": {
        "height": 720,
        "width": 1280
      }
    },
    {
      "at": {
        "corner": "topRight",
        "x": 0,
        "y": 0
      },
      "place": {
        "x": 0,
        "y": 0
      },
      "window": {
        "height": 1,
        "width": 1
      }
    },
    {
      "at": {
        "corner": "topRight",
        "x": 0,
        "y": 0
      },
      "place": null,
      "window": {
        "height": 0,
        "width": 0
      }
    },
    {
      "at": {
        "corner": "bottomLeft",
        "x": 0,
        "y": 0
      },
      "place": {
        "x": 0,
        "y": 719
      },
      "window": {
        "height": 720,
        "width": 1280
      }
    },
    {
      "at": {
        "corner": "bottomLeft",
        "x": 9,
        "y": 15
      },
      "place": {
        "x": 9,
        "y": 704
      },
      "window": {
        "height": 720,
        "width": 1280
      }
    },
    {
      "at": {
        "corner": "bottomLeft",
        "x": 1279,
        "y": 719
      },
      "place": {
        "x": 1279,
        "y": 0
      },
      "window": {
        "height": 720,
        "width": 1280
      }
    },
    {
      "at": {
        "corner": "bottomLeft",
        "x": 1280,
        "y": 0
      },
      "place": null,
      "window": {
        "height": 720,
        "width": 1280
      }
    },
    {
      "at": {
        "corner": "bottomLeft",
        "x": 0,
        "y": 720
      },
      "place": null,
      "window": {
        "height": 720,
        "width": 1280
      }
    },
    {
      "at": {
        "corner": "bottomLeft",
        "x": 0,
        "y": 0
      },
      "place": {
        "x": 0,
        "y": 0
      },
      "window": {
        "height": 1,
        "width": 1
      }
    },
    {
      "at": {
        "corner": "bottomLeft",
        "x": 0,
        "y": 0
      },
      "place": null,
      "window": {
        "height": 0,
        "width": 0
      }
    },
    {
      "at": {
        "corner": "bottomRight",
        "x": 0,
        "y": 0
      },
      "place": {
        "x": 1279,
        "y": 719
      },
      "window": {
        "height": 720,
        "width": 1280
      }
    },
    {
      "at": {
        "corner": "bottomRight",
        "x": 9,
        "y": 15
      },
      "place": {
        "x": 1270,
        "y": 704
      },
      "window": {
        "height": 720,
        "width": 1280
      }
    },
    {
      "at": {
        "corner": "bottomRight",
        "x": 1279,
        "y": 719
      },
      "place": {
        "x": 0,
        "y": 0
      },
      "window": {
        "height": 720,
        "width": 1280
      }
    },
    {
      "at": {
        "corner": "bottomRight",
        "x": 1280,
        "y": 0
      },
      "place": null,
      "window": {
        "height": 720,
        "width": 1280
      }
    },
    {
      "at": {
        "corner": "bottomRight",
        "x": 0,
        "y": 720
      },
      "place": null,
      "window": {
        "height": 720,
        "width": 1280
      }
    },
    {
      "at": {
        "corner": "bottomRight",
        "x": 0,
        "y": 0
      },
      "place": {
        "x": 0,
        "y": 0
      },
      "window": {
        "height": 1,
        "width": 1
      }
    },
    {
      "at": {
        "corner": "bottomRight",
        "x": 0,
        "y": 0
      },
      "place": null,
      "window": {
        "height": 0,
        "width": 0
      }
    }
  ]
} as const;

/** The settings a new install starts with. */
export const DEFAULT_SETTINGS: SettingsFile = {
  "format": 1,
  "readInterval": {
    "fastestMs": 16,
    "slowestMs": 500
  },
  "screenshotShortcut": "F9",
  "trace": {
    "on": false,
    "until": null,
    "record": [
      "changes",
      "rules",
      "actions",
      "lifecycle"
    ],
    "fileLimitMb": 50,
    "keepFiles": 5
  },
  "rememberEnabled": false
};
