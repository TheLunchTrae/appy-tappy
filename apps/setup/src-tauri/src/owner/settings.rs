//! The Settings page: the saved settings, which the runtime reads again once
//! a save worked, and whether the runtime starts when the user signs in to
//! Windows.

use appytappy_config::settings_file::SettingsFile;

use crate::settings::{self, SettingsView};

use super::host::Host;
use super::{Changed, Owner};

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
        let saved = self
            .stores()
            .and_then(|stores| settings::save(stores, file));
        self.after(Changed::Runtime, saved)
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
}
