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
    pub(crate) fn starts_with_windows(&self) -> Result<bool, String> {
        settings::starts_with_windows()
    }

    /// Makes the runtime start when the user signs in to Windows, or not.
    #[expect(
        clippy::unused_self,
        reason = "a command runs on the owner, which keeps the registry's reads and writes in order"
    )]
    pub(crate) fn set_starts_with_windows(&self, on: bool) -> Result<(), String> {
        settings::set_starts_with_windows(on)
    }
}
