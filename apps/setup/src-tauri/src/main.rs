//! The AppyTappy setup app, where the user points at a game's window, takes
//! screenshots, and sets up the values automations read.
//!
//! It is a separate program from the runtime: it opens on demand and exits
//! fully when closed. See `docs/architecture.md` for how the two meet.

#![forbid(unsafe_code)]
#![windows_subsystem = "windows"]

mod check_spec;
mod commands;
mod owner;
mod pixels;
mod window_list;

use std::process::{self, ExitCode};
use std::sync::mpsc;
use std::thread;

use appytappy_config::paths::Locations;
use appytappy_config::store::AppStore;
use appytappy_sources::HostSystem;

use crate::commands::Bridge;
use crate::owner::{Message, Owner};

fn main() -> ExitCode {
    // The host system sets the process's DPI awareness, which Windows needs
    // before any window exists, so it comes before the app's window.
    let system = HostSystem::default();
    let (sender, messages) = mpsc::channel();
    let store = Locations::from_env()
        .map(|locations| AppStore::new(locations.apps()))
        .map_err(|error| format!("AppyTappy cannot find where to keep your values: {error}."));
    let owner = Owner::new(system, store, process::id(), sender.clone());
    let Ok(owner_thread) = thread::Builder::new()
        .name("owner".to_owned())
        .spawn(move || owner::run(owner, &messages))
    else {
        return ExitCode::FAILURE;
    };
    let code = tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(Bridge(sender.clone()))
        .invoke_handler(tauri::generate_handler![
            commands::list_windows,
            commands::pick_window,
            commands::preview,
            commands::take_screenshot,
            commands::screenshot,
            commands::set_check,
            commands::read_value,
            commands::match_windows,
            commands::list_apps,
            commands::save_value,
            commands::delete_value,
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
