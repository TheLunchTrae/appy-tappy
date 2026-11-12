//! Every command the page calls, once: its function, the arguments the page
//! sends it by name, and its answer.
//!
//! `main.rs` registers each one with Tauri from this table, so a command
//! left out of it is never called and fails the dead-code lint. The
//! bindings test writes `apps/setup/src/lib/api.ts` from it, one function
//! for each command that sends its arguments under the keys Tauri reads,
//! and checks the table against each function: the compiler checks the
//! argument and answer types, and the test the argument names. The doc
//! comments are the page's; they become the page functions' docs.
//!
//! An argument or answer is its Rust type, except for three the page reads
//! differently from their ts-rs type: `Pixels`, the raw bytes of a picture
//! that the page decodes, `U64AsNumber`, a `u64` the page takes as a number
//! because it stays far below 2^53, and `OptionalU64AsNumber`, one that can
//! be missing.

/// Hands the table to `$then`, a macro that takes its entries.
macro_rules! with_commands {
    ($then:ident) => {
        $then! {
            commands::list_windows() -> WindowList;
            commands::pick_window(window: U64AsNumber) -> ();
            /// Stops capturing and forgets the session's screenshots and check.
            commands::stop_capture() -> ();
            /// The newest frame at most `maxWidth` wide, or null before one arrives.
            commands::preview(max_width: u32) -> Pixels;
            /// Keeps the newest frame as a screenshot and returns its index.
            commands::take_screenshot() -> usize;
            /// A screenshot at most `maxWidth` wide, or null for an unknown index.
            commands::screenshot(index: usize, max_width: u32) -> Pixels;
            /// What `selection`'s steps select on screenshot `screenshot`, or null when they leave nothing selected.
            commands::select(screenshot: usize, selection: SelectionSpec) -> Option<SelectionView>;
            commands::set_check(spec: CheckSpec) -> ();
            /// What the check reads on the newest frame, or null without a check or a frame.
            commands::read_value() -> Option<Reading>;
            /// Records `region` of the picked window, drawn on `screenshot`. With `more`, goes on with the stopped
            /// recording, which needs the same region, ink and window size; otherwise starts a new one.
            commands::start_recording(
                region: RegionSpec,
                ink: ColorSpec,
                tolerance: u8,
                screenshot: usize,
                more: bool
            ) -> ();
            /// Stops the recording, keeping its readings for review.
            commands::stop_recording() -> ();
            /// Forgets the recording and its readings.
            commands::discard_recording() -> ();
            /// The recording and its readings so far, or null without one.
            commands::recording_state() -> Option<RecordingView>;
            /// A recorded reading at most `maxWidth` wide, or null for an unknown index.
            commands::recorded_reading(index: usize, max_width: u32) -> Pixels;
            /// Why the reference `spec` teaches cannot learn each of its recorded readings, in order, or null for one it
            /// can.
            commands::review_problems(spec: CheckSpec) -> Vec<Option<String>>;
            /// Captures a saved value's region on its application's window, replacing any capture, and returns the
            /// watch's number.
            commands::watch_value(application: String, value: String) -> U64AsNumber;
            /// What the watch's value reads now, and whether `when` holds for it. Fails once the watch was stopped or
            /// replaced.
            commands::watched_reading(watch: U64AsNumber, when: Option<WhenDef>) -> Live;
            /// Stops the watch, unless another capture replaced it already.
            commands::stop_watching(watch: U64AsNumber) -> ();
            /// The running windows `window` takes, the one a new session would read first.
            commands::match_windows(window: WindowDef) -> Vec<WindowFit>;
            /// The window a new session would read for `window`, or null while none fits.
            commands::read_window(window: WindowDef) -> OptionalU64AsNumber;
            /// Keeps `region` of screenshot `screenshot` as a new click picture, comparing only what `selection` picks
            /// when it is given. One that no saved automation names goes the next time automations are saved, or with
            /// {@link forgetUnsavedClickPictures}.
            commands::add_click_picture(
                screenshot: usize,
                region: RegionSpec,
                selection: Option<SelectionSpec>
            ) -> ClickPictureView;
            /// Removes the click pictures no saved automation names, for when no automation is being made or changed.
            commands::forget_unsaved_click_pictures() -> ();
            /// The click picture `image`, at most `maxWidth` wide.
            commands::click_picture(image: String, max_width: u32) -> Pixels;
            /// Where a click at `at` lands on the picked window's newest frame; rejects with why it lands nowhere.
            commands::find_click(at: ClickAtDef) -> FoundClick;
            commands::list_apps() -> Vec<AppSummary>;
            /// The saved applications and the open windows, with the windows each application's match takes, from
            /// one listing of the desktop, so the three agree. Rejects when the windows cannot be listed.
            commands::list_app_windows() -> AppWindows;
            /// Saves `spec` over the application `existing`, or as a new one; returns its ID.
            commands::save_app(existing: Option<String>, spec: AppSpec) -> String;
            /// Removes an application with its values, unless an automation uses one of them, and closes its overlay.
            /// The removal worked when this resolves; the answer is why an overlay window could not open or close, or
            /// null.
            commands::remove_app(application: String) -> Option<String>;
            commands::save_value(spec: SaveSpec) -> SavedValue;
            /// Reads the live frame with a saved value as `spec` edits it, as it would be read at the window's size
            /// now, and answers what saving the edit would do. Nothing is saved. Rejects with what to fix first.
            commands::try_edit(spec: EditSpec) -> EditPreview;
            /// Saves a saved value as `spec` edits it, under the ID it has. `dropFixes` is the window sizes of the fixes
            /// the user agreed to lose: an edit that leaves other fixes unusable than exactly those is refused, as is
            /// one that would break an automation.
            commands::edit_value(spec: EditSpec, drop_fixes: Vec<SizeDef>) -> SavedValue;
            /// The size of the picked window on its newest frame, which an edit is tried at, or null before one
            /// arrives.
            commands::window_size() -> Option<SizeDef>;
            /// Where the saved value starts on a window of `width` by `height` in the fix flow, or null when it would
            /// not fit.
            commands::place_value(
                application: String,
                value: String,
                width: u32,
                height: u32
            ) -> Option<RegionSpec>;
            /// Saves a fix of a saved value at the size of the screenshot `spec` names.
            commands::save_fix(spec: FixSpec) -> SavedValue;
            /// Removes the saved value's fix at `width` by `height`.
            commands::remove_fix(application: String, value: String, width: u32, height: u32) -> ();
            /// What the application's values read now, for the Values page; the first call starts reading its window.
            commands::live_values(application: String) -> OverlayView;
            /// Stops reading values for the Values page.
            commands::stop_live_values() -> ();
            commands::delete_value(application: String, value: String) -> ();
            /// Saves a reference, over the one `spec.check.reference` names when it names one.
            commands::save_reference(spec: ReferenceSaveSpec) -> SavedReferenceId;
            /// Removes a saved reference and its pictures; refused while a value reads with it.
            commands::delete_reference(application: String, reference: String) -> ();
            /// Sets how the application `application` protects itself from other programs.
            commands::set_anti_cheat(application: String, anti_cheat: AntiCheatDef) -> ();
            /// Records the build of `application` that runs now as the one its values were checked against.
            commands::mark_checked(application: String) -> ();
            commands::list_automations() -> Vec<AutomationSummary>;
            /// Saves `automation` over the automation `existing`, or as a new one; returns its ID.
            commands::save_automation(existing: Option<String>, automation: AutomationDef) -> String;
            commands::set_automation_enabled(id: String, enabled: bool) -> ();
            commands::delete_automation(id: String) -> ();
            /// Opens a window for each overlay that is on, and closes the rest.
            commands::sync_overlays() -> ();
            /// Turns an application's overlay on or off and sets where it shows. The setting is saved when this
            /// resolves; the answer is why an overlay window could not open or close, or null.
            commands::set_overlay(application: String, enabled: bool, mode: OverlayModeDef) -> Option<String>;
            /// What an application's overlay shows now.
            commands::overlay_state(application: String) -> OverlayView;
            /// Tells the setup app the size of an overlay's card in CSS pixels, which its window over the application
            /// takes.
            commands::overlay_card(application: String, width: f64, height: f64) -> ();
            /// The saved settings, or the defaults and why the file cannot be read.
            commands::load_settings() -> SettingsView;
            /// Saves every setting; rejects with what to fix.
            commands::save_settings(settings: SettingsFile) -> ();
            /// Whether the runtime starts when the user signs in to Windows.
            commands::starts_with_windows() -> bool;
            commands::set_starts_with_windows(on: bool) -> ();
            /// The folder the runtime writes its trace files to.
            commands::trace_folder() -> String;
            /// Opens the trace folder in File Explorer.
            commands::open_trace_folder() -> ();
            /// The newer release on GitHub, or null when this one is the newest or this build does not update itself.
            updates::check_for_update() -> Option<UpdateOffer>;
            /// Downloads the newest release and checks its signature; AppyTappy Setup keeps it until
            /// {@link installUpdate}. The release it downloaded can be newer than the one offered, if one came out
            /// since. It rejects when the check or the download fails.
            updates::download_update() -> UpdateOffer;
            /// Runs the downloaded installer for `version`, which closes AppyTappy Setup and the background part,
            /// updates both, and starts them again. It rejects when the update fails before the installer starts, and
            /// never settles otherwise.
            updates::install_update(version: String) -> ();
        }
    };
}

pub(crate) use with_commands;
