//! The Settings page: the saved settings, which the runtime reads again once
//! a save worked, whether the runtime starts when the user signs in to
//! Windows, and the folder its trace files go in.

use std::path::Path;

use appytappy_config::settings_file::SettingsFile;

use crate::settings::{self, SettingsView};

use super::Owner;
use super::host::Host;

impl<H: Host> Owner<H> {
    /// The saved settings, or the defaults and why they cannot be read.
    /// `setup.log` starts or stops as they say.
    pub(crate) fn settings(&mut self) -> SettingsView {
        let view = match self.stores() {
            Ok(stores) => settings::load(stores),
            Err(problem) => SettingsView {
                settings: SettingsFile::default(),
                problem: Some(problem),
            },
        };
        self.setup_log.set(view.settings.show_debug);
        view
    }

    /// Saves every setting, or says what to fix. Once they are saved,
    /// `setup.log` starts or stops as they say.
    pub(crate) fn save_settings(&mut self, file: &SettingsFile) -> Result<(), String> {
        self.update_runtime(|stores| settings::save(stores, file))?;
        self.setup_log.set(file.show_debug);
        Ok(())
    }

    /// The folder the runtime writes its trace files to.
    pub(crate) fn trace_folder(&self) -> Result<String, String> {
        self.traces().map(|traces| traces.display().to_string())
    }

    /// Opens the trace folder in File Explorer.
    #[mutants::skip] // Opens a window, checked by hand on Windows.
    pub(crate) fn open_trace_folder(&self) -> Result<(), String> {
        settings::open_trace_folder(self.traces()?)
    }

    /// The folder the runtime writes its trace files to, or why it is
    /// unknown, as the user reads it.
    fn traces(&self) -> Result<&Path, String> {
        self.stores
            .as_ref()
            .map(|stores| stores.traces.as_path())
            .map_err(|reason| {
                format!("AppyTappy cannot find where the runtime keeps its traces: {reason}.")
            })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use appytappy_config::settings_file::{ReadIntervalDef, SettingsFile};

    use crate::owner::Owner;
    use crate::owner::test_support::{harness_with, scratch_store};

    #[test]
    fn a_settings_save_that_worked_tells_the_runtime() {
        let (stores, root) = scratch_store("owner-settings");
        let mut harness = harness_with(false, Ok(stores));
        let faster = SettingsFile {
            read_interval: ReadIntervalDef {
                fastest_ms: 8,
                slowest_ms: 400,
            },
            ..SettingsFile::default()
        };
        assert_eq!(harness.ask(|owner| owner.save_settings(&faster)), Ok(()));
        assert_eq!(harness.reloads(), 1, "the runtime reads the settings again");
        assert_eq!(harness.ask(Owner::settings).settings, faster);

        let backwards = SettingsFile {
            read_interval: ReadIntervalDef {
                fastest_ms: 500,
                slowest_ms: 100,
            },
            ..SettingsFile::default()
        };
        assert!(
            harness
                .ask(|owner| owner.save_settings(&backwards))
                .is_err()
        );
        assert_eq!(harness.reloads(), 0, "a save that failed tells no one");
        assert_eq!(
            harness.ask(Owner::settings).settings,
            faster,
            "the saved settings stay"
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn debug_details_open_the_setup_log_and_hiding_them_closes_it() {
        let (stores, root) = scratch_store("owner-setup-log");
        let log = stores.logs.join("setup.log");
        let settings_file = stores.locations.settings();
        let mut harness = harness_with(false, Ok(stores));
        let shown = SettingsFile {
            show_debug: true,
            ..SettingsFile::default()
        };
        assert_eq!(harness.ask(|owner| owner.save_settings(&shown)), Ok(()));
        let failed = "No window.".to_owned();
        harness.ask(|owner| owner.record_command("pick_window", Duration::ZERO, Some(&failed)));
        let text = std::fs::read_to_string(&log).expect("the setup log");
        assert!(text.contains("pick_window failed"), "{text}");

        assert_eq!(
            harness.ask(|owner| owner.save_settings(&SettingsFile::default())),
            Ok(())
        );
        harness.ask(|owner| owner.record_command("save_value", Duration::ZERO, None));
        assert_eq!(
            std::fs::read_to_string(&log).expect("the setup log"),
            text,
            "hidden, it writes nothing"
        );

        // Loading settings that show debug details opens it again.
        std::fs::write(&settings_file, "{\"format\": 1, \"showDebug\": true}").expect("settings");
        assert!(harness.ask(Owner::settings).settings.show_debug);
        harness.ask(|owner| owner.record_command("save_value", Duration::ZERO, None));
        assert!(
            std::fs::read_to_string(&log)
                .expect("the setup log")
                .contains("save_value took"),
            "loaded, it writes again"
        );
        std::fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn the_trace_folder_is_where_the_runtime_writes_its_traces() {
        // Nothing is written, so there is nothing to clean up.
        let (stores, _root) = scratch_store("owner-traces");
        let traces = stores.traces.display().to_string();
        let mut harness = harness_with(false, Ok(stores));
        assert_eq!(harness.ask(|owner| owner.trace_folder()), Ok(traces));
        assert_eq!(
            harness_with(false, Err("APPDATA is not set".to_owned()))
                .ask(|owner| owner.trace_folder()),
            Err(
                "AppyTappy cannot find where the runtime keeps its traces: APPDATA is not set."
                    .to_owned()
            )
        );
    }
}
