//! Where the setup app keeps what the user saves: the applications' folders
//! the automations' file, and the settings file.

use appytappy_config::app_file::AppFile;
use appytappy_config::store::{AppStore, AutomationStore, SettingsStore};

/// Where the authoring files live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Stores {
    /// The applications' folders.
    pub(crate) apps: AppStore,
    /// The automations' file.
    pub(crate) automations: AutomationStore,
    /// The settings file.
    pub(crate) settings: SettingsStore,
}

impl Stores {
    /// The saved application `app`, from its file. Fails with "That
    /// application is no longer saved." when no application has that ID, and
    /// with the read error when the stores or its file cannot be read.
    pub(crate) fn app_file(&self, app: &str) -> Result<AppFile, String> {
        self.apps
            .load()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|stored| stored.id == app)
            .ok_or_else(|| "That application is no longer saved.".to_owned())?
            .file
    }
}
