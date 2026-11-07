//! The AppyTappy setup app, where the user points at an application's window, takes
//! screenshots, and sets up the values automations read.
//!
//! It is a separate program from the runtime: it opens on demand and exits
//! fully when closed. See `docs/developer/architecture.md` for how the two meet.

#![forbid(unsafe_code)]
#![windows_subsystem = "windows"]

mod automations;
mod check_spec;
mod commands;
mod overlay;
mod overlay_window;
mod owner;
mod pixels;
mod window_list;

use std::process::{self, ExitCode};
use std::sync::mpsc;
use std::thread;

use appytappy_config::paths::Locations;
use appytappy_config::store::{AppStore, AutomationStore};
use appytappy_sources::HostSystem;

use crate::automations::Stores;
use crate::commands::Bridge;
use crate::owner::{Message, Owner};

fn main() -> ExitCode {
    // The host system sets the process's DPI awareness, which Windows needs
    // before any window exists, so it comes before the app's window.
    let system = HostSystem::default();
    let (sender, messages) = mpsc::channel();
    let stores = Locations::from_env()
        .map(|locations| Stores {
            apps: AppStore::new(locations.apps()),
            automations: AutomationStore::new(locations.automations()),
        })
        .map_err(|error| format!("AppyTappy cannot find where to keep your values: {error}."));
    let owner = Owner::new(system, stores, process::id(), sender.clone());
    let Ok(owner_thread) = thread::Builder::new()
        .name("owner".to_owned())
        .spawn(move || owner::run(owner, &messages))
    else {
        return ExitCode::FAILURE;
    };
    let code = tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(Bridge(sender.clone()))
        .on_window_event(overlay_window::on_event)
        .invoke_handler(tauri::generate_handler![
            commands::list_windows,
            commands::pick_window,
            commands::preview,
            commands::stop_capture,
            commands::take_screenshot,
            commands::screenshot,
            commands::set_check,
            commands::read_value,
            commands::watch_value,
            commands::watched_reading,
            commands::stop_watching,
            commands::match_windows,
            commands::list_apps,
            commands::save_value,
            commands::delete_value,
            commands::set_anti_cheat,
            commands::list_automations,
            commands::save_automation,
            commands::set_automation_enabled,
            commands::delete_automation,
            commands::sync_overlays,
            commands::set_overlay,
            commands::overlay_state,
            commands::overlay_card,
        ])
        .build(tauri::generate_context!())
        .map_or(1, |app| app.run_return(|_app, _event| {}));
    // The owner stops its capture before it ends, so the join waits for it.
    let stopped = sender.send(Message::Shutdown).is_ok()
        && owner_thread.join().is_ok_and(|closed| closed.is_ok());
    if code == 0 && stopped {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
