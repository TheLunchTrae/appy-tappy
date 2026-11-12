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
mod debug;
mod drafts;
mod match_preview;
mod overlay;
mod owner;
mod pixels;
mod recording;
mod screenshots;
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
use appytappy_sources::HostSystem;

use crate::commands::Bridge;
use crate::owner::{Desktop, Message, Owner};
use crate::recording::ocr;
use crate::stores::Stores;

/// Tauri's handler for every command in the table.
macro_rules! handler {
    ($($(#[doc = $doc:literal])* $module:ident::$name:ident($($arg:ident: $ty:ty),* $(,)?) -> $answer:ty;)*) => {
        tauri::generate_handler![$($module::$name),*]
    };
}

fn main() -> ExitCode {
    // The host system sets the process's DPI awareness, which Windows needs
    // before any window exists, so it comes before the app's window.
    let system = HostSystem::default();
    let (sender, messages) = mpsc::channel();
    let stores = Locations::from_env()
        .map(Stores::from)
        .map_err(|error| error.to_string());
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
    let built = tauri::Builder::default()
        // A second copy that starts hands over to this one and exits, before
        // it opens a window. The plugin asks to be registered first.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            overlay::window::bring_forward(app);
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(Bridge(sender.clone()))
        .setup(|app| Ok(updates::set_up(app)?))
        .on_window_event(overlay::window::on_event)
        .invoke_handler(commands::table::with_commands!(handler))
        .build(tauri::generate_context!());
    let Ok(app) = built else {
        return ExitCode::FAILURE;
    };
    // The owner opens and closes the overlays' windows, so it starts once
    // the app is built. A command waits in the channel until it does, and
    // none comes before the app runs.
    let host = Desktop {
        system,
        app: app.handle().clone(),
    };
    let owner = Owner::new(host, stores, process::id(), sender.clone(), jobs);
    let Ok(owner_thread) = thread::Builder::new()
        .name("owner".to_owned())
        .spawn(move || owner::run(owner, &messages))
    else {
        return ExitCode::FAILURE;
    };
    let code = app.run_return(|_app, _event| {});
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
