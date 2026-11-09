//! The Settings page: the saved settings, which the runtime reads again once
//! a save worked, whether the runtime starts when the user signs in to
//! Windows, and the folder its trace files go in.

use appytappy_config::settings_file::SettingsFile;

use crate::settings::{self, SettingsView};

use super::Owner;
use super::host::Host;

impl<H: Host> Owner<H> {
    /// The saved settings, or the defaults and why they cannot be read.
    pub(crate) fn settings(&self) -> SettingsView {
        match self.stores() {
            Ok(stores) => settings::load(stores),
            Err(problem) => SettingsView {
                settings: SettingsFile::default(),
                problem: Some(problem),
            },
        }
    }

    /// Saves every setting, or says what to fix.
    pub(crate) fn save_settings(&mut self, file: &SettingsFile) -> Result<(), String> {
        self.update_runtime(|stores| settings::save(stores, file))
    }

    /// Whether the runtime starts when the user signs in to Windows. It
    /// runs here, like every command, so two changes reach the registry in
    /// the order the user made them.
    #[expect(
        clippy::unused_self,
        reason = "a command runs on the owner, which keeps the registry's reads and writes in order"
    )]
    #[mutants::skip] // Reads the registry, checked by hand on Windows.
    pub(crate) fn starts_with_windows(&self) -> Result<bool, String> {
        settings::starts_with_windows()
    }

    /// Makes the runtime start when the user signs in to Windows, or not.
    #[expect(
        clippy::unused_self,
        reason = "a command runs on the owner, which keeps the registry's reads and writes in order"
    )]
    #[mutants::skip] // Writes the registry, checked by hand on Windows.
    pub(crate) fn set_starts_with_windows(&self, on: bool) -> Result<(), String> {
        settings::set_starts_with_windows(on)
    }

    /// The folder the runtime writes its trace files to.
    pub(crate) fn trace_folder(&self) -> Result<String, String> {
        self.stores()
            .map(|stores| stores.traces.display().to_string())
    }

    /// Opens the trace folder in File Explorer.
    #[mutants::skip] // Opens a window, checked by hand on Windows.
    pub(crate) fn open_trace_folder(&self) -> Result<(), String> {
        settings::open_trace_folder(&self.stores()?.traces)
    }
}

#[cfg(test)]
mod tests {
    use appytappy_config::settings_file::{ReadIntervalDef, SettingsFile};

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
        assert_eq!(harness.ask(|owner| owner.settings()).settings, faster);

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
            harness.ask(|owner| owner.settings()).settings,
            faster,
            "the saved settings stay"
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
            harness_with(false, Err("nowhere".to_owned())).ask(|owner| owner.trace_folder()),
            Err("nowhere".to_owned())
        );
    }
}
