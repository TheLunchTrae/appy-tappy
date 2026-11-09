//! The Settings page's settings: the settings file, and whether the runtime
//! starts when the user signs in to Windows.
//!
//! The settings file is the setup app's to write; the runtime reads it again
//! after each save, like the other saved files. Starting at sign-in lives in
//! the user's `Run` registry key, which the installer turns on and the
//! uninstaller removes, so the page reads and changes it there.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use appytappy_config::settings_file::SettingsFile;
use appytappy_platform::AUTOSTART_VALUE;
use serde::Serialize;

use crate::stores::Stores;

/// The runtime's program, which the installer puts beside the setup app's.
const RUNTIME_PROGRAM: &str = "appytappy-runtime.exe";

/// The settings as the Settings page shows them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct SettingsView {
    /// The saved settings, or the defaults when the file cannot be read.
    pub(crate) settings: SettingsFile,
    /// Why the saved settings cannot be read, in which case the page shows
    /// the defaults and saving replaces the file.
    pub(crate) problem: Option<String>,
}

/// The saved settings, or the defaults and why the file cannot be read.
pub(crate) fn load(stores: &Stores) -> SettingsView {
    match stores.settings.load() {
        Ok(settings) => SettingsView {
            settings,
            problem: None,
        },
        Err(error) => SettingsView {
            settings: SettingsFile::default(),
            problem: Some(format!(
                "AppyTappy cannot read your settings, so it shows the defaults: {error}."
            )),
        },
    }
}

/// Saves `settings`, saying what to fix when one is out of range.
pub(crate) fn save(stores: &Stores, settings: &SettingsFile) -> Result<(), String> {
    stores.settings.save(settings).map_err(|error| {
        if error.kind() == io::ErrorKind::InvalidInput {
            error.to_string()
        } else {
            format!("AppyTappy cannot save your settings: {error}.")
        }
    })
}

/// Whether Windows starts the runtime when the user signs in.
#[mutants::skip] // Reads the registry, checked by hand on Windows.
pub(crate) fn starts_with_windows() -> Result<bool, String> {
    appytappy_platform::autostart_enabled(AUTOSTART_VALUE)
        .map_err(|error| format!("AppyTappy cannot tell whether it starts with Windows: {error}."))
}

/// Makes Windows start the runtime when the user signs in, or stop doing
/// so.
#[mutants::skip] // Writes the registry, checked by hand on Windows.
pub(crate) fn set_starts_with_windows(on: bool) -> Result<(), String> {
    let changed = if on {
        env::current_exe().and_then(|setup| {
            let runtime = runtime_beside(&setup)?;
            appytappy_platform::enable_autostart(AUTOSTART_VALUE, &command(&runtime))
        })
    } else {
        appytappy_platform::disable_autostart(AUTOSTART_VALUE)
    };
    changed.map_err(|error| {
        format!("AppyTappy cannot change whether it starts with Windows: {error}.")
    })
}

/// Opens the trace folder `folder` in File Explorer. The folder is made
/// first, so it opens before the first trace too.
#[mutants::skip] // Opens a window, checked by hand on Windows.
pub(crate) fn open_trace_folder(folder: &Path) -> Result<(), String> {
    fs::create_dir_all(folder)
        .and_then(|()| open_folder(folder))
        .map_err(|error| format!("AppyTappy cannot open the trace folder: {error}."))
}

/// Shows `folder` in File Explorer.
#[cfg(windows)]
#[mutants::skip] // Opens a window, checked by hand on Windows.
fn open_folder(folder: &Path) -> io::Result<()> {
    std::process::Command::new("explorer.exe")
        .arg(folder)
        .spawn()
        .map(drop)
}

/// Off Windows there is no File Explorer to show a folder in.
#[cfg(not(windows))]
#[mutants::skip] // The setup app runs only on Windows.
fn open_folder(_folder: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "folders open only on Windows",
    ))
}

/// The runtime's program beside the setup app's program `setup`.
///
/// # Errors
///
/// Returns a not-found error when it is not there, as when the setup app
/// runs from a build folder rather than where it was installed.
fn runtime_beside(setup: &Path) -> io::Result<PathBuf> {
    let runtime = setup.with_file_name(RUNTIME_PROGRAM);
    if runtime.is_file() {
        Ok(runtime)
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{RUNTIME_PROGRAM} is not beside AppyTappy Setup"),
        ))
    }
}

/// The command line Windows runs at sign-in: the program, quoted because a
/// path with spaces would otherwise run its first word. The runtime writes
/// the same command with `--autostart on`.
fn command(runtime: &Path) -> OsString {
    let mut command = OsString::from("\"");
    command.push(runtime);
    command.push("\"");
    command
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process;

    use super::*;

    fn stores(name: &str) -> (Stores, PathBuf) {
        crate::stores::scratch(&format!("settings-{name}"))
    }

    #[test]
    fn settings_save_and_load_back() {
        let (stores, folder) = stores("round-trip");
        assert_eq!(
            load(&stores),
            SettingsView {
                settings: SettingsFile::default(),
                problem: None,
            }
        );
        let settings = SettingsFile {
            screenshot_shortcut: "Control+Shift+S".to_owned(),
            ..SettingsFile::default()
        };
        save(&stores, &settings).expect("saves");
        assert_eq!(load(&stores).settings, settings);
        fs::remove_dir_all(folder).expect("clean up");
    }

    #[test]
    fn a_setting_out_of_range_says_what_to_fix() {
        let (stores, folder) = stores("out-of-range");
        let mut settings = SettingsFile::default();
        settings.read_interval.fastest_ms = 900;
        assert_eq!(
            save(&stores, &settings),
            Err("The fastest read cannot wait longer than the slowest.".to_owned())
        );
        assert!(!folder.join("AppyTappy").join("settings.json").exists());
        if folder.exists() {
            fs::remove_dir_all(folder).expect("clean up");
        }
    }

    #[test]
    fn an_unreadable_file_shows_the_defaults_and_why() {
        let (stores, folder) = stores("broken");
        let roaming = folder.join("AppyTappy");
        fs::create_dir_all(&roaming).expect("make the folder");
        fs::write(roaming.join("settings.json"), "{").expect("write");
        let view = load(&stores);
        assert_eq!(view.settings, SettingsFile::default());
        let problem = view.problem.expect("a problem");
        assert!(
            problem.starts_with(
                "AppyTappy cannot read your settings, so it shows the defaults: the settings file \
                 is not valid"
            ),
            "{problem}"
        );
        fs::remove_dir_all(folder).expect("clean up");
    }

    #[test]
    fn the_runtime_starts_from_beside_the_setup_app() {
        let folder = env::temp_dir().join(format!("appytappy-beside-{}", process::id()));
        fs::create_dir_all(&folder).expect("make the folder");
        let setup = folder.join("appytappy-setup.exe");
        let missing = runtime_beside(&setup).expect_err("no runtime yet");
        assert_eq!(missing.kind(), io::ErrorKind::NotFound);
        fs::write(folder.join(RUNTIME_PROGRAM), b"").expect("write");
        assert_eq!(
            runtime_beside(&setup).expect("found"),
            folder.join(RUNTIME_PROGRAM)
        );
        fs::remove_dir_all(folder).expect("clean up");
    }

    #[test]
    fn the_sign_in_command_quotes_the_runtime() {
        assert_eq!(
            command(Path::new(
                "C:/Program Files/AppyTappy/appytappy-runtime.exe"
            )),
            OsString::from(r#""C:/Program Files/AppyTappy/appytappy-runtime.exe""#)
        );
    }
}
