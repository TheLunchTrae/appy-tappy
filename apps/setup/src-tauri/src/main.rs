//! AppyTappy Setup, the app where the user points at an application's window, takes
//! screenshots, and sets up the values automations read.
//!
//! It is a separate program from the runtime: it opens on demand and exits
//! fully when closed. See `wiki/Architecture.md` for how the two meet.

#![forbid(unsafe_code)]
#![windows_subsystem = "windows"]

mod applications;
mod automations;
#[cfg(test)]
mod bindings;
mod check_spec;
mod commands;
mod overlay;
mod owner;
mod pixels;
mod recording;
mod selection;
mod settings;
mod stores;
mod updates;
mod window_list;

use std::env;
use std::process::{self, ExitCode};
use std::sync::mpsc;
use std::thread;

use appytappy_config::paths::Locations;
use appytappy_config::store::{AppStore, AutomationStore, SettingsStore};
use appytappy_sources::HostSystem;

use crate::commands::Bridge;
use crate::owner::{Message, Owner};
use crate::recording::ocr;
use crate::stores::Stores;

fn main() -> ExitCode {
    // The host system sets the process's DPI awareness, which Windows needs
    // before any window exists, so it comes before the app's window.
    let system = HostSystem::default();
    let (sender, messages) = mpsc::channel();
    let stores = Locations::from_env()
        .map(|locations| Stores {
            apps: AppStore::new(locations.apps()),
            automations: AutomationStore::new(locations.automations()),
            settings: SettingsStore::new(locations.settings()),
        })
        .map_err(|error| format!("AppyTappy cannot find where to keep your values: {error}."));
    // The text reader reads recorded readings on a thread of its own, and
    // ends once the owner drops its sender.
    let (jobs, inbox) = mpsc::channel();
    let model = env::current_exe().map_or_else(
        |_unknown| ocr::model_path(ocr::MODEL_FILE.as_ref()),
        |program| ocr::model_path(&program),
    );
    let reader_messages = sender.clone();
    let Ok(reader_thread) = thread::Builder::new()
        .name("text reader".to_owned())
        .spawn(move || ocr::run(&inbox, &reader_messages, &model))
    else {
        return ExitCode::FAILURE;
    };
    let owner = Owner::new(system, stores, process::id(), sender.clone(), jobs);
    let Ok(owner_thread) = thread::Builder::new()
        .name("owner".to_owned())
        .spawn(move || owner::run(owner, &messages))
    else {
        return ExitCode::FAILURE;
    };
    let code = tauri::Builder::default()
        // A second copy that starts hands over to this one and exits, before
        // it opens a window. The plugin asks to be registered first.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            overlay::window::bring_forward(app);
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(Bridge(sender.clone()))
        .setup(|app| Ok(updates::set_up(app)?))
        .on_window_event(overlay::window::on_event)
        .invoke_handler(tauri::generate_handler![
            commands::list_windows,
            commands::pick_window,
            commands::preview,
            commands::stop_capture,
            commands::take_screenshot,
            commands::screenshot,
            commands::select,
            commands::set_check,
            commands::read_value,
            commands::start_recording,
            commands::stop_recording,
            commands::discard_recording,
            commands::recording_state,
            commands::recorded_reading,
            commands::review_problems,
            commands::watch_value,
            commands::watched_reading,
            commands::stop_watching,
            commands::match_windows,
            commands::add_click_picture,
            commands::click_picture,
            commands::find_click,
            commands::list_apps,
            commands::save_app,
            commands::remove_app,
            commands::save_value,
            commands::place_value,
            commands::save_fix,
            commands::remove_fix,
            commands::live_values,
            commands::stop_live_values,
            commands::delete_value,
            commands::save_reference,
            commands::delete_reference,
            commands::set_anti_cheat,
            commands::mark_checked,
            commands::list_automations,
            commands::save_automation,
            commands::set_automation_enabled,
            commands::delete_automation,
            commands::sync_overlays,
            commands::set_overlay,
            commands::overlay_state,
            commands::overlay_card,
            commands::settings,
            commands::save_settings,
            commands::starts_with_windows,
            commands::set_starts_with_windows,
            commands::trace_folder,
            commands::open_trace_folder,
            updates::check_for_update,
            updates::download_update,
            updates::install_update,
        ])
        .build(tauri::generate_context!())
        .map_or(1, |app| app.run_return(|_app, _event| {}));
    // The owner stops its capture before it ends, so the join waits for it.
    // The owner's sender to the text reader goes with it, so the reader
    // ends after the reading it is on.
    let stopped = sender.send(Message::Shutdown).is_ok()
        && owner_thread.join().is_ok_and(|closed| closed.is_ok());
    let read = reader_thread.join().is_ok();
    if code == 0 && stopped && read {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
