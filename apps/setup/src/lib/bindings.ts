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
command: MediaCommand,
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
 * How it reaches the window: with the cursor when left out.
 */
delivery: ClickDeliveryDef,
/**
 * Whether the window that was in front gets the focus back after a
 * click with the cursor. A click sent as messages moves no focus
 * and does not read this.
 */
returnFocus: boolean,
/**
 * Where it clicks.
 */
at: ClickAtDef, }>;

/**
 * How an app protects itself from other programs.
 *
 * Only [`AntiCheatClass::None`] permits input-injection actions: see
 * [`allows_input`]. An application's file spells it as the variant in
 * camel case, such as `userMode`, and the policy reads it as it is, since
 * every class is one the policy handles.
 */
export type AntiCheatClass = DeepReadonly<"none" | "userMode" | "kernelLevel" | "unknown">;

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
antiCheat: AntiCheatClass,
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
 * An application's name, how to find its window, how often that window
 * changes size, and its anti-cheat class, as the editor sends them.
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
sizeChanges: SizeChangesDef,
/**
 * The anti-cheat class picked in the editor, or `None` to keep the
 * saved one, which a new application starts at unknown. The editor
 * sends a class only when it was picked there, so a save leaves a class
 * set on the Applications page since the editor opened as it is.
 */
antiCheat: AntiCheatClass | null, }>;

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
 * One automation's numbers.
 */
export type AutomationMetrics = DeepReadonly<{ app: string, name: string,
/**
 * How many times its actions started.
 */
ran: number,
/**
 * How many times its cooldown held it back.
 */
held: number,
/**
 * How many times its actions failed.
 */
failed: number,
/**
 * From a frame arriving to its actions starting.
 */
frameToAction: Spread | null, }>;

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
 * What looking for a bar in a region found.
 */
export type BarDetection = DeepReadonly<{ "kind": "proposed", filled: ColorDef, empty: ColorDef,
/**
 * The screenshot whose layout gave the colors.
 */
foundOn: number,
/**
 * The screenshots that show the bar in these colors, full, empty,
 * or in between.
 */
shownOn: Array<number>,
/**
 * The screenshots looked at that show neither color.
 */
without: Array<number>,
/**
 * Whether two screenshots show both colors at different fills.
 */
moves: boolean,
/**
 * Whether the colors read a screenshot a few points off where the
 * bar ends on it.
 */
rough: boolean,
/**
 * Whether the colors were found over only the strips of the
 * region that show the bar from end to end, past a label, an icon,
 * or what lies beside the bar.
 */
around: boolean,
/**
 * How many older screenshots of the window size were not looked
 * at.
 */
leftOut: number, } | { "kind": "declined", message: string, }>;

/**
 * What two colors measure on the session's screenshots.
 */
export type BarMeasure = DeepReadonly<{
/**
 * What each screenshot looked at reads, in the order they were taken.
 */
readings: Array<BarReading>,
/**
 * How many older screenshots of the window size were not looked at.
 */
leftOut: number, }>;

/**
 * What a screenshot's region reads with two colors.
 */
export type BarReading = DeepReadonly<{
/**
 * The screenshot, by index.
 */
screenshot: number,
/**
 * How full the bar reads, from 0 to 100, or `None` when no pixel of
 * the region is near either color.
 */
fill: number | null, }>;

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
 * What goes in the bundle beyond the files it always holds.
 */
export type BundleSpec = DeepReadonly<{
/**
 * The pictures the values, references and click actions learned from,
 * and the screenshots the drafts keep.
 */
pictures: boolean,
/**
 * The name of the trace file to add, the one the dialog showed, or
 * `None` for no trace. Only a trace file in the trace folder is added.
 */
trace: string | null, }>;

/**
 * The capture of one application at one window size.
 */
export type CaptureOutline = DeepReadonly<{
/**
 * The rectangles the runtime copies out of each frame, in the frame's
 * pixels. Empty when no enabled automation watches a value that reads
 * at this size.
 */
copies: Array<RegionDef>,
/**
 * Each saved value, in the application's order.
 */
values: Array<OutlinedValue>, }>;

/**
 * A character's closest glyph and the closest of another character.
 */
export type CharacterView = DeepReadonly<{
/**
 * The closest glyph, if the reference has any.
 */
best: GlyphView | null,
/**
 * The closest glyph of another character, if the reference has one.
 */
runnerUp: GlyphView | null,
/**
 * Whether the value leaves the character out, so it reads as text and
 * not as a part of the number.
 */
leftOut: boolean, }>;

/**
 * The characters a reference is being taught, as the UI describes them:
 * the saved samples of `reference`, when it is saved, then `labels`, then
 * `recorded`, all in the color `ink` within `tolerance`.
 */
export type CharactersSpec = DeepReadonly<{
/**
 * Where the characters show, which a recording records and the live
 * window is read at.
 */
region: RegionDef,
/**
 * The screenshot the region was drawn on. Every screenshot with a
 * box and the recording must have its window size, since the region
 * means another spot on a window of another size.
 */
screenshot: number,
/**
 * The saved reference these characters add to, or `None` for a new
 * one.
 */
reference: ReferenceRef | null,
/**
 * The characters' color.
 */
ink: ColorDef,
/**
 * How far from `ink` a pixel of a character may be.
 */
tolerance: number,
/**
 * The labeled screenshots.
 */
labels: Array<Label>,
/**
 * Readings of the session's recording, which must be of `region`.
 */
recorded: Array<RecordedLabel>, }>;

/**
 * How many times fall in each bar of a chart.
 */
export type Chart = DeepReadonly<{
/**
 * How long each bar spans; bar `n` counts the times from `n` bars to
 * `n + 1`.
 */
barNs: number,
/**
 * How many times each bar counts.
 */
counts: number[],
/**
 * How many times come after the last bar, as a few far slower times
 * would leave the rest too thin to read.
 */
over: number,
/**
 * How many times in all.
 */
samples: number, }>;

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
whole: boolean,
/**
 * The characters the value reads, such as `0123456789%`, or `None`
 * for every character its reference knows.
 *
 * The value matches what it sees against these characters alone,
 * so a value that shows only digits cannot read a `0` as an `O`
 * its reference also knows. Whitespace does not count, as in a
 * label, and neither does a character the reference has yet to
 * learn. A fix that reads with its own reference reads these
 * characters of it.
 */
characters?: string, } | { "kind": "barFill",
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
 * A value's check as the UI describes it.
 */
export type CheckSpec = DeepReadonly<{ "kind": "number", region: RegionDef, reference: ReferenceRef,
/**
 * Whether digits after the decimal point are ignored.
 */
whole: boolean,
/**
 * The characters the value reads, or `None` for every character
 * the reference knows, as [`CheckDef::Number`] saves them.
 */
characters?: string, } | { "kind": "barFill", region: RegionDef, filled: ColorDef, empty: ColorDef, } | { "kind": "color", region: RegionDef, color: ColorDef, tolerance: number, } | { "kind": "state", region: RegionDef,
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
 * How a click reaches its window, as the file spells it: with the cursor
 * when the file names none. A cursor click's return of the focus is the
 * action's own field.
 */
export type ClickDeliveryDef = DeepReadonly<"cursor" | "message">;

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
 * The window a click sent on request went to.
 */
export type ClickSent = DeepReadonly<{
/**
 * The window's handle, as picking a window names it.
 */
window: number,
/**
 * The window's title, which tells an application's windows apart.
 */
title: string, }>;

/**
 * A click to send now, as the click target screen sets it up.
 */
export type ClickSpec = DeepReadonly<{
/**
 * The ID of the saved application whose value the automation watches,
 * or nothing before the automation watches one.
 */
watched: string | null,
/**
 * The ID of the saved application it clicks in.
 */
app: string,
/**
 * Which button, and how many times.
 */
button: ClickButtonDef,
/**
 * How it reaches the window.
 */
delivery: ClickDeliveryDef,
/**
 * Whether the window that was in front gets the focus back after a
 * click with the cursor.
 */
returnFocus: boolean,
/**
 * Where it clicks.
 */
at: ClickAtDef, }>;

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
 * How a step's pick combines with the selection before it.
 */
export type Combine = DeepReadonly<"replace" | "add" | "remove">;

/**
 * How a comparison relates a signal's value to the rule's value, spelled
 * in the file as the variant in camel case, such as `atMost`. It has
 * nothing to check, so the file reads it as it is.
 */
export type Comparison = DeepReadonly<"below" | "atMost" | "equals" | "notEquals" | "atLeast" | "above">;

/**
 * [`Corner`] as the file spells it.
 */
export type CornerDef = DeepReadonly<"topLeft" | "topRight" | "bottomLeft" | "bottomRight">;

/**
 * What the Debug page's overview shows.
 */
export type DebugOverview = DeepReadonly<{
/**
 * Whether the runtime is running, or `None` when that cannot be told.
 */
runtime: boolean | null,
/**
 * The newest trace file, or `None` without one.
 */
trace: TraceSummary | null,
/**
 * Why the newest trace file cannot be read, if it cannot.
 */
traceProblem: string | null, }>;

/**
 * A draft as the page saves it.
 */
export type DraftDef = DeepReadonly<{ kind: DraftKind,
/**
 * The ID of the saved application a value, fix, or reference belongs
 * to, or null.
 */
app: string | null,
/**
 * The ID of the saved item it changes: an automation, an application, a
 * value a fix is of, or a reference. Null for a new one. A saved item
 * has one draft at most.
 */
changes: string | null,
/**
 * What the lists call it.
 */
name: string,
/**
 * What is still to do before it can be saved, a sentence each.
 */
missing: Array<string>,
/**
 * The click pictures it names, which saving the automations keeps.
 */
pictures: Array<string>,
/**
 * What the flow had set up, which only the page reads.
 */
state: unknown, }>;

/**
 * What a draft is of, which says which flow finishes it.
 */
export type DraftKind = DeepReadonly<"automation" | "application" | "value" | "fix" | "reference">;

/**
 * A screenshot a draft keeps: the index it had in its capture, and whether
 * a recording of looks kept it there rather than the user taking it.
 */
export type DraftShot = DeepReadonly<{ index: number, recorded: boolean, }>;

/**
 * A saved draft, as the lists show it.
 */
export type DraftSummary = DeepReadonly<{ id: string,
/**
 * The draft, or null when its file cannot be read.
 */
draft: DraftDef | null,
/**
 * Why its file cannot be read, or null.
 */
problem: string | null,
/**
 * How many screenshots it keeps.
 */
screenshots: number, }>;

/**
 * Where an edited value is read and how.
 */
export type EditDrawing = DeepReadonly<{ "kind": "kept",
/**
 * The check as the file spells it. A state check names the
 * pictures the value has, in their order, each under the name the
 * state has now.
 */
check: CheckDef, } | { "kind": "drawn",
/**
 * The check, with the session's screenshots it names. A state
 * check takes every state's picture again.
 */
check: CheckSpec,
/**
 * The screenshot the region was drawn on.
 */
screenshot: number, }>;

/**
 * What saving an edit would do, which the editor shows before the save.
 */
export type EditPreview = DeepReadonly<{
/**
 * Why the edited value is not read at the window's size now, or `None`
 * when the live reading reads it there.
 */
unread: string | null,
/**
 * The window sizes of the fixes the edit removes, since they no longer
 * suit how the value reads. The save needs the user's word for it.
 */
droppedFixes: Array<SizeDef>,
/**
 * Why the save is refused: the automations the edit would break.
 */
refused: string | null, }>;

/**
 * A saved value changed in place, as the UI describes it. The value keeps
 * its ID and the window size it was drawn at.
 */
export type EditSpec = DeepReadonly<{
/**
 * The saved application.
 */
app: string,
/**
 * The value's ID.
 */
value: string,
/**
 * The value's name, the one it has or a new one.
 */
valueName: string,
/**
 * The side of the window the region stays at as the window changes
 * size.
 */
pin: PinDef,
/**
 * Whether the region is a reading area, larger than what it reads.
 * Only a check that reads anywhere in its region may have one.
 */
readingArea: boolean,
/**
 * Where the value is read and how.
 */
drawing: EditDrawing, }>;

/**
 * One more example of the text a recording of the whole window looks for:
 * a box the user drew around some of it.
 */
export type ExampleSpec = DeepReadonly<{
/**
 * The box.
 */
region: RegionDef,
/**
 * The screenshot it is drawn on.
 */
screenshot: number, }>;

/**
 * [`WindowMatch::fallback`] as the file spells it.
 */
export type FallbackDef = DeepReadonly<"none" | "executable">;

/**
 * One find as the UI shows it.
 */
export type FindView = DeepReadonly<{
/**
 * Where it is on the screenshot.
 */
region: RegionDef,
/**
 * What the text reader made of it.
 */
text: RecordedText,
/**
 * The characters' color its ink was last cut with for the text reader,
 * which reads the same ink as the same text.
 */
ink: ColorDef,
/**
 * The tolerance its ink was last cut with.
 */
tolerance: number,
/**
 * Where each of its characters is in its region, left to right, as
 * that color and tolerance split them, with a name for its shape that
 * a recorded reading's character of the same shape shares.
 */
characters: Array<ShownCharacter>, }>;

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
 * A search as the UI shows it.
 */
export type FoundView = DeepReadonly<{
/**
 * Its number in the session.
 */
id: number,
/**
 * The screenshot it searched.
 */
screenshot: number,
/**
 * Each find, top to bottom and left to right.
 */
finds: Array<FindView>,
/**
 * Finds past the most a search keeps.
 */
more: number,
/**
 * Runs in the characters' color of another size, left out.
 */
otherSizes: number, }>;

/**
 * How close a character is to one glyph.
 */
export type GlyphView = DeepReadonly<{
/**
 * The glyph's character.
 */
character: string,
/**
 * How many grid cells one of the two sets where the other sets
 * neither that cell nor one beside it.
 */
apart: number,
/**
 * How many grid cells only one of the two sets.
 */
differing: number,
/**
 * How many grid cells either of the two sets.
 */
set: number,
/**
 * Whether the character reads as this glyph: at most 1 in 32 of the
 * set cells are apart.
 */
matches: boolean, }>;

/**
 * What the user chose for an import.
 */
export type ImportSpec = DeepReadonly<{
/**
 * The application's name.
 */
name: string,
/**
 * The anti-cheat class the user confirmed, which is saved whatever the
 * file holds.
 */
antiCheat: AntiCheatClass,
/**
 * The name of the file the bytes came from, kept as where the
 * application was imported from.
 */
from: string, }>;

/**
 * What one screenshot's region shows, typed by the user so a reference
 * learns its characters.
 */
export type Label = DeepReadonly<{ screenshot: number,
/**
 * Where on the screenshot the text shows, when it is not the spec's
 * region: the box the user drew around it, or kept from found text.
 */
region?: RegionDef, text: string, }>;

/**
 * The pixels of one kind in the rectangle asked about.
 */
export type Layer = DeepReadonly<{
/**
 * How many pixels are of this kind.
 */
count: number,
/**
 * The runs of them in the rectangle, each as the index of its first
 * pixel and of the pixel after its last, row by row from the top, as a
 * selection's are. Missing when there are too many runs to draw.
 */
spans: Array<[number, number]> | null, }>;

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
 * One of the two logs.
 */
export type LogName = DeepReadonly<"runtime" | "setup">;

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
 * What makes a pixel match, as the UI describes it.
 */
export type MatchRule = DeepReadonly<{ "kind": "ink", ink: ColorDef, tolerance: number, } | { "kind": "barFill", filled: ColorDef, empty: ColorDef, } | { "kind": "color", color: ColorDef, tolerance: number, }>;

/**
 * What a rule takes of a rectangle of a screenshot.
 */
export type MatchView = DeepReadonly<{ "kind": "ink", ink: Layer, } | { "kind": "barFill", filled: Layer, empty: Layer, } | { "kind": "color",
/**
 * The pixels that are near the color on their own.
 */
near: Layer,
/**
 * The rectangle's average color.
 */
average: ColorDef,
/**
 * The largest difference between the average's red, green, and
 * blue and the color's, which the tolerance bounds.
 */
distance: number,
/**
 * Whether the check reads yes on this rectangle.
 */
matches: boolean, }>;

/**
 * What a media action does, spelled in the file as the variant in camel
 * case, such as `play`. It has nothing to check, so the file reads it as
 * it is.
 */
export type MediaCommand = DeepReadonly<"play" | "pause" | "next" | "previous">;

/**
 * Which fields besides the executable a window has to match to qualify,
 * as an application's file spells it too.
 */
export type MustMatch = DeepReadonly<{
/**
 * The title has to match.
 */
title: boolean,
/**
 * The class has to match.
 */
class: boolean, }>;

/**
 * How a value or a reference was made: its provenance.
 *
 * Setup guidance only. The setup app shows it, so whoever reads a
 * definition can tell where each value and reference came from. Nothing
 * that finds the window or reads a value looks at it, and no rule of the
 * file covers it, whatever file name it holds.
 */
export type OriginDef = DeepReadonly<{ "kind": "byHand" } | { "kind": "recording" } | { "kind": "imported",
/**
 * The name of the file it was imported from, such as
 * `appytappy-example.zip`. Empty in a shared file, which says
 * that its maker imported it and not from which file of theirs.
 */
file: string, }>;

/**
 * One saved value in a [`CaptureOutline`].
 */
export type OutlinedValue = DeepReadonly<{
/**
 * The value's ID.
 */
id: string,
/**
 * The value's name.
 */
name: string,
/**
 * Whether an enabled automation watches it, so the runtime copies it.
 */
watched: boolean,
/**
 * Where it reads in the frame, or `None` where it does not read.
 */
region: RegionDef | null,
/**
 * Why it does not read at this size, if it does not.
 */
problem: string | null, }>;

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
 * How far a [`TraceView`] has read its file.
 */
export type ReadProgress = DeepReadonly<{
/**
 * The file's name.
 */
file: string,
/**
 * The file's size.
 */
bytes: number,
/**
 * How much of it is read. Less than `bytes` while it is read.
 */
read: number,
/**
 * Why its rows cannot be read, such as a trace from another version.
 */
problem: string | null,
/**
 * Whether some rows could not be read, as when a quoted field went
 * wrong, so the numbers leave them out.
 */
skipped: boolean, }>;

/**
 * A value the check read, as the UI shows it.
 */
export type Reading = DeepReadonly<{ "kind": "number", "value": number } | { "kind": "flag", "value": boolean } | { "kind": "state", "value": string }>;

/**
 * A color's red, green, and blue.
 */
export type ReadingColor = DeepReadonly<{
/**
 * Red.
 */
red: number,
/**
 * Green.
 */
green: number,
/**
 * Blue.
 */
blue: number, }>;

/**
 * A live reading's debug details.
 */
export type ReadingDetails = DeepReadonly<{
/**
 * How long the check took to read the region, in microseconds.
 */
tookUs: number,
/**
 * How long ago the frame it read arrived, in milliseconds.
 */
frameAgeMs: number,
/**
 * The check's scores, or `None` when the region has none.
 */
workings: WorkingsView | null,
/**
 * Why the region has no scores, such as no ink in it.
 */
problem: string | null, }>;

/**
 * What a recording of readings records, as the UI describes it.
 */
export type RecordSpec = DeepReadonly<{
/**
 * The saved application whose window is recorded, whose answer on how
 * often its window changes size says whether the recording follows
 * the window to another size.
 */
app: string,
/**
 * The region of the window to record.
 */
region: RegionDef,
/**
 * The screenshot the region was drawn on, whose window size the
 * recording keeps.
 */
screenshot: number,
/**
 * The side of the window the region stays at as the window changes
 * size, or `None` for the side the region is nearest.
 */
pin: PinDef | null,
/**
 * The characters' color.
 */
ink: ColorDef,
/**
 * How far from `ink` a pixel of a character may be.
 */
tolerance: number,
/**
 * Where to look for characters: in the region alone, or wherever the
 * window shows text like the region's.
 */
scope: RecordingScope,
/**
 * The reference's boxes: more examples of its text, which a recording
 * of the whole window looks for the sizes of too.
 */
examples: Array<ExampleSpec>, }>;

/**
 * What one recorded reading shows, as the user confirmed it in review, so
 * a reference learns its characters like a box's.
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
 * How many frames showed exactly this reading.
 */
seen: number,
/**
 * The size of text a recording of the whole window took it for when
 * it first showed. `None` for a reading of the region itself.
 */
size: RecordedSize | null,
/**
 * What the text reader made of it.
 */
text: RecordedText,
/**
 * Each of its characters' shapes, left to right, as an index into the
 * recording's shapes.
 */
shapes: Array<number>,
/**
 * How wide its picture is, in pixels.
 */
width: number,
/**
 * Where each of its characters is in its picture, left to right, with
 * a name for its shape that a find's character of the same shape
 * shares.
 */
characters: Array<ShownCharacter>, }>;

/**
 * How often one shape showed, as the UI shows it. These are counts of
 * what the recording saw, not a measure of how well the shape reads: a
 * shape that shows often is still whatever the user's label says it is.
 */
export type RecordedShape = DeepReadonly<{
/**
 * How many frames showed it.
 */
frames: number,
/**
 * How many different readings showed it.
 */
readings: number, }>;

/**
 * The size of text a reading was taken for, as the UI shows it.
 */
export type RecordedSize = DeepReadonly<{
/**
 * How tall its characters are in its picture, in pixels: about as
 * tall as the look searched for at the window's size then.
 */
height: number,
/**
 * The one of the recording's sizes it is: how tall the characters of
 * that example were on the screenshot it was drawn on. It differs
 * from `height` when the window had another size.
 */
example: number, }>;

/**
 * What the text reader made of a reading.
 */
export type RecordedText = DeepReadonly<{ "kind": "pending" } | { "kind": "read", "text": string } | { "kind": "failed", "text": string }>;

/**
 * Where a recording looks for characters, as the UI names it.
 */
export type RecordingScope = DeepReadonly<"place" | "window">;

/**
 * What a recording records: the region of a window of one size, and what
 * counts as ink there.
 */
export type RecordingSettings = DeepReadonly<{
/**
 * The region of the window.
 */
region: RegionDef,
/**
 * The characters' color.
 */
ink: ColorDef,
/**
 * How far a pixel may be from the characters' color and still be ink.
 */
tolerance: number,
/**
 * The window's size when the region was drawn.
 */
windowSize: SizeDef,
/**
 * Where it looks for characters.
 */
scope: RecordingScope, }>;

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
 * Why it keeps and counts no more, once it is full.
 */
full: string | null,
/**
 * Why the latest frame could not be recorded.
 */
problem: string | null,
/**
 * What it records, which going on with it needs again.
 */
settings: RecordingSettings,
/**
 * The window size it is placed at: the size the region was drawn at,
 * or the one frames settled at since. The region and the sizes of
 * text are scaled from the one to the other where they differ.
 */
at: SizeDef,
/**
 * How tall the characters are that a recording of the whole window
 * looks for, smallest first, in pixels of the window the region was
 * drawn on: one size for each example of text that differs from the
 * others. Empty for a recording of the region alone.
 */
sizes: Array<number>,
/**
 * Where a recording of the whole window found text like its region's
 * the last time it looked, on a window of the size it is placed at,
 * while it records; empty otherwise.
 */
spots: Array<RegionDef>,
/**
 * The readings it kept, in the order they first showed.
 */
readings: Array<RecordedReading>,
/**
 * Every different shape its frames showed, in the order they first
 * showed. Each shows in at least one kept reading.
 */
shapes: Array<RecordedShape>, }>;

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
 * How the reference was made. [`OriginDef::ByHand`] when the file
 * leaves it out.
 */
origin: OriginDef,
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
samples: Array<SampleDef>,
/**
 * The characters the user says it needs to know, such as
 * `0123456789.%`, or `None` when the user chose none.
 *
 * Setup guidance only. The setup app shows which of them the samples
 * have yet to teach, and writes them each once, in order, when it saves
 * the reference. Nothing that learns or reads looks at it, and no rule
 * of the file covers it: it adds no glyph, labels no shape, leaves how
 * a number is read as it is, and never stops a file from loading,
 * whatever text it holds.
 */
expects?: string, }>;

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
 * The characters it is taught, which name the reference when it is
 * saved already.
 */
characters: CharactersSpec,
/**
 * The characters the user says it needs to know, or empty when the
 * user chose none. The save lists each once, in order, refuses more
 * than a reference saves, and replaces what the saved reference
 * expected.
 */
expects: string, }>;

/**
 * A reference being taught to try on the live window before it is saved,
 * as the UI describes it.
 */
export type ReferenceTrial = DeepReadonly<{
/**
 * The saved application the reference is for, whose window's size
 * changes say where its region reads.
 */
app: string,
/**
 * The characters it is taught.
 */
characters: CharactersSpec,
/**
 * The characters the number it is made for reads of it, or `None`
 * for every character it is taught, as a saved number check has them.
 */
reads: string | null,
/**
 * Whether that number ignores digits after the decimal point.
 */
whole: boolean,
/**
 * The side of the window that number's region stays at, or `None`
 * for the side the region is nearest.
 */
pin: PinDef | null, }>;

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
 * Which rows the rows table shows.
 */
export type RowFilter = DeepReadonly<{ range: TraceRange,
/**
 * The categories of row to show. A trace's own rows count as
 * lifecycle rows.
 */
categories: Array<TraceCategory>,
/**
 * The application, or `None` for every one.
 */
app: string | null,
/**
 * Only rows that took longer than this, in nanoseconds.
 */
tookOverNs: number | null,
/**
 * Text the row's thread, event, application, item, value or detail
 * holds, in any case; empty for any row.
 */
search: string, }>;

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
pin: PinDef,
/**
 * Whether the region was drawn as a reading area, larger than what it
 * reads. Only a check that reads anywhere in its region may have one.
 */
readingArea: boolean, }>;

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
 * Whether the user drew the region as a reading area: larger than what
 * it reads, to cover every place content that moves inside the window
 * can show. Left out of the file when it is not.
 *
 * Setup guidance only. The setup app shows the region as a reading
 * area when the value is listed or drawn again. Nothing that places
 * or reads the region looks at it, so the value reads the same with
 * and without it. Only a check that
 * [reads anywhere in its region](CheckDef::reads_anywhere) may have
 * one.
 */
readingArea?: boolean,
/**
 * The value drawn again at other window sizes, each read exactly at its
 * own size. Always written, even when empty, since the setup app reads
 * the file's JSON as is.
 */
fixes: Array<FixDef>, }>;

/**
 * One thing the user did to select.
 */
export type SelectStep = DeepReadonly<{ "kind": "snap", region: RegionDef, combine: Combine, } | { "kind": "click", x: number, y: number, combine: Combine, }>;

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
region: RegionDef,
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
rememberEnabled: boolean,
/**
 * Whether the setup app shows its debug details: the Debug page, the
 * working behind each live reading, the capture outlines and the window
 * match scores. On, it also writes `setup.log`. The runtime ignores it.
 */
showDebug: boolean, }>;

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
 * A shared file as the import dialog shows it, before anything is saved.
 */
export type SharedView = DeepReadonly<{
/**
 * The application's file as the shared file holds it, with the
 * anti-cheat class its maker set.
 */
file: AppFile,
/**
 * The name to offer: the file's, or that with a number when a saved
 * application has it already.
 */
name: string, }>;

/**
 * One character of a picture.
 */
export type ShownCharacter = DeepReadonly<{
/**
 * A name for its shape, which two characters share exactly when their
 * shapes are the same, whichever recording or search they are from.
 */
shape: string,
/**
 * The first column of the picture it covers.
 */
left: number,
/**
 * The column after the last it covers.
 */
right: number, }>;

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
 * A middle and a slow time, in nanoseconds.
 */
export type Spread = DeepReadonly<{
/**
 * The median.
 */
medianNs: number,
/**
 * The 95th percentile: 1 in 20 took longer.
 */
p95Ns: number, }>;

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
 * How far the region is from one state's picture.
 */
export type StateView = DeepReadonly<{
/**
 * The state's name.
 */
name: string,
/**
 * The summed difference of the compared channels.
 */
total: number,
/**
 * How many channels were compared.
 */
channels: number,
/**
 * The largest average difference that shows the state.
 */
tolerance: number,
/**
 * Whether the region is within the tolerance. The read is the closest
 * state that is.
 */
within: boolean,
/**
 * Whether the read took this state.
 */
read: boolean, }>;

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
 * A trace file, as the Debug page lists it.
 */
export type TraceFile = DeepReadonly<{
/**
 * Its name.
 */
name: string,
/**
 * Its size.
 */
bytes: number, }>;

/**
 * The metrics of one trace file over one range. Counts go by whole
 * seconds, so a range's counts can take in up to a second before it, while
 * its times and rows start where it does.
 */
export type TraceMetrics = DeepReadonly<{
/**
 * How much of the file is read.
 */
progress: ReadProgress,
/**
 * The categories the file holds rows of. A number of another category
 * was not recorded.
 */
recorded: Array<TraceCategory>,
/**
 * How long the range covers up to the newest row: from where it starts,
 * or from the first row when that is later.
 */
spanNs: number,
/**
 * From a frame arriving to the action it led to starting.
 */
frameToAction: Spread | null,
/**
 * How those times spread.
 */
chart: Chart | null,
/**
 * How long the runtime took to handle each event.
 */
ruleHandling: Spread | null,
/**
 * How long each automation's actions took to run.
 */
actionRun: Spread | null,
/**
 * How many values were read.
 */
reads: number,
/**
 * How many values changed.
 */
changes: number,
/**
 * How many automations' actions started.
 */
actions: number,
/**
 * How many automations' actions failed.
 */
failed: number,
/**
 * How many records the trace dropped.
 */
dropped: number,
/**
 * Each value read, by application and then name.
 */
values: Array<ValueMetrics>,
/**
 * Each automation that fired or ran, by application and then name.
 */
automations: Array<AutomationMetrics>,
/**
 * Whether a time comes from only the newest [`SAMPLES`] of its kind,
 * as the range holds more.
 */
sampled: boolean, }>;

/**
 * Which of a trace's rows the metrics and the rows count.
 */
export type TraceRange = DeepReadonly<"lastMinute" | "lastTenMinutes" | "whole">;

/**
 * One row of a trace file, as the page shows it.
 */
export type TraceRow = DeepReadonly<{
/**
 * When it happened, in nanoseconds since the trace started.
 */
timeNs: number, thread: string, event: string, app: string, item: string, value: string,
/**
 * How long it took, in nanoseconds, where the event says.
 */
tookNs: number | null,
/**
 * When the frame arrived whose reading led to this row.
 */
cause: number | null, detail: string, }>;

/**
 * The rows a [`RowFilter`] picks.
 */
export type TraceRows = DeepReadonly<{
/**
 * How much of the file is read.
 */
progress: ReadProgress,
/**
 * The newest rows it picks, at most [`SHOWN`], oldest first.
 */
rows: Array<TraceRow>,
/**
 * How many rows it picks.
 */
matching: number,
/**
 * How many rows are held to pick from, the newest [`ROWS`].
 */
held: number,
/**
 * How many rows were read.
 */
total: number,
/**
 * The applications the held rows name, in order.
 */
apps: Array<string>, }>;

/**
 * What the overview shows of the newest trace file.
 */
export type TraceSummary = DeepReadonly<{
/**
 * The file's name.
 */
file: string,
/**
 * Its size.
 */
bytes: number,
/**
 * Why its rows cannot be read, such as a trace from another version.
 */
problem: string | null,
/**
 * From a frame arriving to the action it led to starting, or `None`
 * when no action started from a frame, as with `actions` or `changes`
 * not recorded.
 */
frameToAction: Spread | null,
/**
 * How long the runtime took to handle each event, or `None` when no
 * event was recorded, as with `rules` not recorded.
 */
ruleHandling: Spread | null,
/**
 * How long the runtime took to run each automation's actions, or
 * `None` when none ran, as with `actions` not recorded.
 */
actionRun: Spread | null,
/**
 * How many records the trace dropped, because its buffer was full, in
 * the part of the file read.
 */
dropped: number,
/**
 * Whether the part read starts after the file's first row, because
 * the file, or its growth since the last read, was larger than a read
 * takes. The numbers then cover only the newer rows.
 */
partial: boolean,
/**
 * The newest rows, oldest first.
 */
newest: Array<TraceRow>, }>;

/**
 * A value's check to try on the live window before it is saved, as the UI
 * describes it.
 */
export type TrialSpec = DeepReadonly<{
/**
 * The saved application the value is for, whose references it reads
 * with and whose window's size changes say where it reads.
 */
app: string,
/**
 * The check, with the session's screenshots it names.
 */
check: CheckSpec,
/**
 * The screenshot the region was drawn on, whose size the value would
 * keep.
 */
screenshot: number,
/**
 * The side of the window the region stays at as the window changes
 * size, or `None` before one is picked, for the side it is nearest.
 */
pin: PinDef | null, }>;

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
 * How the value was made. [`OriginDef::ByHand`] when the file leaves
 * it out.
 */
origin: OriginDef,
/**
 * Where on the window the value is read and how.
 */
screen: ScreenDef, }>;

/**
 * One value's numbers.
 */
export type ValueMetrics = DeepReadonly<{ app: string, name: string, reads: number, changes: number,
/**
 * When it last changed, in the whole trace.
 */
lastChangeNs: number | null,
/**
 * How long its check took.
 */
check: Spread | null, }>;

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
is: Comparison,
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
mustMatch: MustMatch,
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
 * How one open window compares with an application's window match, for
 * the debug details: which fields match, and whether the match takes it.
 */
export type WindowScore = DeepReadonly<{
/**
 * The window's handle.
 */
handle: number,
/**
 * Whether its title matches the title's pattern.
 */
title: boolean,
/**
 * Whether its class matches the class's pattern.
 */
class: boolean,
/**
 * Whether its executable matches the executable's pattern.
 */
executable: boolean,
/**
 * How the match takes it, or `None` when it does not.
 */
fit: WindowFit | null, }>;

/**
 * [`Workings`] as the page reads it.
 */
export type WorkingsView = DeepReadonly<{ "kind": "number",
/**
 * The characters.
 */
characters: Array<CharacterView>, } | { "kind": "barFill",
/**
 * The pixels nearer the filled color.
 */
filled: number,
/**
 * The pixels near either color.
 */
counted: number, } | { "kind": "color",
/**
 * The region's average color.
 */
average: ReadingColor,
/**
 * The color the check looks for.
 */
color: ReadingColor,
/**
 * How far each of red, green, and blue may be.
 */
tolerance: number, } | { "kind": "state",
/**
 * The states, in the check's order.
 */
states: Array<StateView>, }>;

/**
 * The longest app name a media or volume action names and the longest path or link, the longest cooldown, the
 * most states and the longest state name in characters, the most characters a reference expects, the settings'
 * ranges, the highest tolerance, and the most pixels a click's search area holds, as the saved files take them.
 */
export const LIMITS = {
  "fastestReadMs": 1,
  "maxActionAppBytes": 256,
  "maxCooldownMs": 86400000,
  "maxExpectedCharacters": 128,
  "maxSearchPixels": 160000,
  "maxSharedBytes": 16777216,
  "maxStateNameChars": 32,
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

/** The first line of a trace file, which names its fields. */
export const TRACE_HEADER = "time_ns,thread,event,app,item,value,took_ns,cause,detail";

/** How many of the newest times of each kind the Debug page's metrics keep. */
export const TRACE_SAMPLES = 20000;

/**
 * What the Rust side makes of sample paths, links, key presses, screenshot shortcuts, app names, labels and expected
 * characters: the text it keeps, or null when it refuses one; and where sample spots land in sample windows. The
 * page's own checks must read each the same.
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
  "expectedCharacters": [
    {
      "read": "578%.",
      "text": "87.5%"
    },
    {
      "read": "123",
      "text": " 1 2\t3\n"
    },
    {
      "read": "01%",
      "text": "%10 0%"
    },
    {
      "read": "Aa",
      "text": "Aa"
    },
    {
      "read": "",
      "text": ""
    },
    {
      "read": "",
      "text": "   "
    },
    {
      "read": "12",
      "text": "1\u00852"
    },
    {
      "read": "12\ufeff",
      "text": "\ufeff12"
    },
    {
      "read": "123",
      "text": "1\u30002\u00a03"
    },
    {
      "read": "x\u200b",
      "text": "\u2028x\u2029\u200b"
    },
    {
      "read": "e\u0301",
      "text": "e\u0301"
    },
    {
      "read": "9z\uff11\ud835\udfd9\ud83d\ude00",
      "text": "\ud835\udfd9z\uff119\ud83d\ude00"
    },
    {
      "read": "12",
      "text": "1\u00072\u009f"
    },
    {
      "read": "\u0100\u0101\u0102\u0103\u0104\u0105\u0106\u0107\u0108\u0109\u010a\u010b\u010c\u010d\u010e\u010f\u0110\u0111\u0112\u0113\u0114\u0115\u0116\u0117\u0118\u0119\u011a\u011b\u011c\u011d\u011e\u011f\u0120\u0121\u0122\u0123\u0124\u0125\u0126\u0127\u0128\u0129\u012a\u012b\u012c\u012d\u012e\u012f\u0130\u0131\u0132\u0133\u0134\u0135\u0136\u0137\u0138\u0139\u013a\u013b\u013c\u013d\u013e\u013f\u0140\u0141\u0142\u0143\u0144\u0145\u0146\u0147\u0148\u0149\u014a\u014b\u014c\u014d\u014e\u014f\u0150\u0151\u0152\u0153\u0154\u0155\u0156\u0157\u0158\u0159\u015a\u015b\u015c\u015d\u015e\u015f\u0160\u0161\u0162\u0163\u0164\u0165\u0166\u0167\u0168\u0169\u016a\u016b\u016c\u016d\u016e\u016f\u0170\u0171\u0172\u0173\u0174\u0175\u0176\u0177\u0178\u0179\u017a\u017b\u017c\u017d\u017e\u017f",
      "text": "\u0100\u0101\u0102\u0103\u0104\u0105\u0106\u0107\u0108\u0109\u010a\u010b\u010c\u010d\u010e\u010f\u0110\u0111\u0112\u0113\u0114\u0115\u0116\u0117\u0118\u0119\u011a\u011b\u011c\u011d\u011e\u011f\u0120\u0121\u0122\u0123\u0124\u0125\u0126\u0127\u0128\u0129\u012a\u012b\u012c\u012d\u012e\u012f\u0130\u0131\u0132\u0133\u0134\u0135\u0136\u0137\u0138\u0139\u013a\u013b\u013c\u013d\u013e\u013f\u0140\u0141\u0142\u0143\u0144\u0145\u0146\u0147\u0148\u0149\u014a\u014b\u014c\u014d\u014e\u014f\u0150\u0151\u0152\u0153\u0154\u0155\u0156\u0157\u0158\u0159\u015a\u015b\u015c\u015d\u015e\u015f\u0160\u0161\u0162\u0163\u0164\u0165\u0166\u0167\u0168\u0169\u016a\u016b\u016c\u016d\u016e\u016f\u0170\u0171\u0172\u0173\u0174\u0175\u0176\u0177\u0178\u0179\u017a\u017b\u017c\u017d\u017e\u017f"
    },
    {
      "read": null,
      "text": "\u0100\u0101\u0102\u0103\u0104\u0105\u0106\u0107\u0108\u0109\u010a\u010b\u010c\u010d\u010e\u010f\u0110\u0111\u0112\u0113\u0114\u0115\u0116\u0117\u0118\u0119\u011a\u011b\u011c\u011d\u011e\u011f\u0120\u0121\u0122\u0123\u0124\u0125\u0126\u0127\u0128\u0129\u012a\u012b\u012c\u012d\u012e\u012f\u0130\u0131\u0132\u0133\u0134\u0135\u0136\u0137\u0138\u0139\u013a\u013b\u013c\u013d\u013e\u013f\u0140\u0141\u0142\u0143\u0144\u0145\u0146\u0147\u0148\u0149\u014a\u014b\u014c\u014d\u014e\u014f\u0150\u0151\u0152\u0153\u0154\u0155\u0156\u0157\u0158\u0159\u015a\u015b\u015c\u015d\u015e\u015f\u0160\u0161\u0162\u0163\u0164\u0165\u0166\u0167\u0168\u0169\u016a\u016b\u016c\u016d\u016e\u016f\u0170\u0171\u0172\u0173\u0174\u0175\u0176\u0177\u0178\u0179\u017a\u017b\u017c\u017d\u017e\u017f\u0180"
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
  "labelCharacters": [
    {
      "read": "87.5%",
      "text": "87.5%"
    },
    {
      "read": "123",
      "text": " 1 2\t3\n"
    },
    {
      "read": "%100%",
      "text": "%10 0%"
    },
    {
      "read": "Aa",
      "text": "Aa"
    },
    {
      "read": "",
      "text": ""
    },
    {
      "read": "",
      "text": "   "
    },
    {
      "read": "12",
      "text": "1\u00852"
    },
    {
      "read": "\ufeff12",
      "text": "\ufeff12"
    },
    {
      "read": "123",
      "text": "1\u30002\u00a03"
    },
    {
      "read": "x\u200b",
      "text": "\u2028x\u2029\u200b"
    },
    {
      "read": "e\u0301",
      "text": "e\u0301"
    },
    {
      "read": "\ud835\udfd9z\uff119\ud83d\ude00",
      "text": "\ud835\udfd9z\uff119\ud83d\ude00"
    },
    {
      "read": "1\u00072\u009f",
      "text": "1\u00072\u009f"
    },
    {
      "read": "\u0100\u0101\u0102\u0103\u0104\u0105\u0106\u0107\u0108\u0109\u010a\u010b\u010c\u010d\u010e\u010f\u0110\u0111\u0112\u0113\u0114\u0115\u0116\u0117\u0118\u0119\u011a\u011b\u011c\u011d\u011e\u011f\u0120\u0121\u0122\u0123\u0124\u0125\u0126\u0127\u0128\u0129\u012a\u012b\u012c\u012d\u012e\u012f\u0130\u0131\u0132\u0133\u0134\u0135\u0136\u0137\u0138\u0139\u013a\u013b\u013c\u013d\u013e\u013f\u0140\u0141\u0142\u0143\u0144\u0145\u0146\u0147\u0148\u0149\u014a\u014b\u014c\u014d\u014e\u014f\u0150\u0151\u0152\u0153\u0154\u0155\u0156\u0157\u0158\u0159\u015a\u015b\u015c\u015d\u015e\u015f\u0160\u0161\u0162\u0163\u0164\u0165\u0166\u0167\u0168\u0169\u016a\u016b\u016c\u016d\u016e\u016f\u0170\u0171\u0172\u0173\u0174\u0175\u0176\u0177\u0178\u0179\u017a\u017b\u017c\u017d\u017e\u017f",
      "text": "\u0100\u0101\u0102\u0103\u0104\u0105\u0106\u0107\u0108\u0109\u010a\u010b\u010c\u010d\u010e\u010f\u0110\u0111\u0112\u0113\u0114\u0115\u0116\u0117\u0118\u0119\u011a\u011b\u011c\u011d\u011e\u011f\u0120\u0121\u0122\u0123\u0124\u0125\u0126\u0127\u0128\u0129\u012a\u012b\u012c\u012d\u012e\u012f\u0130\u0131\u0132\u0133\u0134\u0135\u0136\u0137\u0138\u0139\u013a\u013b\u013c\u013d\u013e\u013f\u0140\u0141\u0142\u0143\u0144\u0145\u0146\u0147\u0148\u0149\u014a\u014b\u014c\u014d\u014e\u014f\u0150\u0151\u0152\u0153\u0154\u0155\u0156\u0157\u0158\u0159\u015a\u015b\u015c\u015d\u015e\u015f\u0160\u0161\u0162\u0163\u0164\u0165\u0166\u0167\u0168\u0169\u016a\u016b\u016c\u016d\u016e\u016f\u0170\u0171\u0172\u0173\u0174\u0175\u0176\u0177\u0178\u0179\u017a\u017b\u017c\u017d\u017e\u017f"
    },
    {
      "read": "\u0100\u0101\u0102\u0103\u0104\u0105\u0106\u0107\u0108\u0109\u010a\u010b\u010c\u010d\u010e\u010f\u0110\u0111\u0112\u0113\u0114\u0115\u0116\u0117\u0118\u0119\u011a\u011b\u011c\u011d\u011e\u011f\u0120\u0121\u0122\u0123\u0124\u0125\u0126\u0127\u0128\u0129\u012a\u012b\u012c\u012d\u012e\u012f\u0130\u0131\u0132\u0133\u0134\u0135\u0136\u0137\u0138\u0139\u013a\u013b\u013c\u013d\u013e\u013f\u0140\u0141\u0142\u0143\u0144\u0145\u0146\u0147\u0148\u0149\u014a\u014b\u014c\u014d\u014e\u014f\u0150\u0151\u0152\u0153\u0154\u0155\u0156\u0157\u0158\u0159\u015a\u015b\u015c\u015d\u015e\u015f\u0160\u0161\u0162\u0163\u0164\u0165\u0166\u0167\u0168\u0169\u016a\u016b\u016c\u016d\u016e\u016f\u0170\u0171\u0172\u0173\u0174\u0175\u0176\u0177\u0178\u0179\u017a\u017b\u017c\u017d\u017e\u017f\u0180",
      "text": "\u0100\u0101\u0102\u0103\u0104\u0105\u0106\u0107\u0108\u0109\u010a\u010b\u010c\u010d\u010e\u010f\u0110\u0111\u0112\u0113\u0114\u0115\u0116\u0117\u0118\u0119\u011a\u011b\u011c\u011d\u011e\u011f\u0120\u0121\u0122\u0123\u0124\u0125\u0126\u0127\u0128\u0129\u012a\u012b\u012c\u012d\u012e\u012f\u0130\u0131\u0132\u0133\u0134\u0135\u0136\u0137\u0138\u0139\u013a\u013b\u013c\u013d\u013e\u013f\u0140\u0141\u0142\u0143\u0144\u0145\u0146\u0147\u0148\u0149\u014a\u014b\u014c\u014d\u014e\u014f\u0150\u0151\u0152\u0153\u0154\u0155\u0156\u0157\u0158\u0159\u015a\u015b\u015c\u015d\u015e\u015f\u0160\u0161\u0162\u0163\u0164\u0165\u0166\u0167\u0168\u0169\u016a\u016b\u016c\u016d\u016e\u016f\u0170\u0171\u0172\u0173\u0174\u0175\u0176\u0177\u0178\u0179\u017a\u017b\u017c\u017d\u017e\u017f\u0180"
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
  "rememberEnabled": false,
  "showDebug": false
};
