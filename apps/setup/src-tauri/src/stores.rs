//! Where the setup app keeps what the user saves: the applications' folders
//! and the automations' file.

use appytappy_config::app_file::AppFile;
use appytappy_config::store::{AppStore, AutomationStore};

/// Where the authoring files live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Stores {
    /// The applications' folders.
    pub(crate) apps: AppStore,
    /// The automations' file.
    pub(crate) automations: AutomationStore,
}

impl Stores {
    /// The saved application `app`, from its file.
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
