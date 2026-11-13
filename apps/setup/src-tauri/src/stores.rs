//! Where the setup app keeps what the user saves: the applications' folders,
//! the automations' file, the settings file and the drafts, and where the
//! runtime writes its logs and traces.

use std::fs;
use std::io::{self, Read as _};
use std::path::PathBuf;

use appytappy_config::app_file::AppFile;
use appytappy_config::paths::Locations;
use appytappy_config::store::{AppStore, AutomationStore, SettingsStore};

use crate::drafts::DraftStore;

/// Where the authoring files live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Stores {
    /// The applications' folders.
    pub(crate) apps: AppStore,
    /// The automations' file.
    pub(crate) automations: AutomationStore,
    /// The settings file.
    pub(crate) settings: SettingsStore,
    /// The drafts, which only the setup app reads.
    pub(crate) drafts: DraftStore,
    /// The folder the runtime writes its trace files to.
    pub(crate) traces: PathBuf,
    /// The folder of the runtime's log, where the setup app writes its own
    /// while it shows debug details.
    pub(crate) logs: PathBuf,
    /// Every file's place, for the debug bundle, which copies the files as
    /// they are on disk.
    pub(crate) locations: Locations,
}

impl From<Locations> for Stores {
    fn from(locations: Locations) -> Self {
        Self {
            apps: AppStore::new(locations.apps()),
            automations: AutomationStore::new(locations.automations()),
            settings: SettingsStore::new(locations.settings()),
            drafts: DraftStore::new(locations.drafts()),
            traces: locations.traces(),
            logs: locations.logs(),
            locations,
        }
    }
}

impl Stores {
    /// The file of the saved application `app`, read without the other
    /// applications, or `None` when no application has that ID. Fails with
    /// the read error when its folder or its file cannot be read.
    pub(crate) fn file(&self, app: &str) -> Result<Option<AppFile>, String> {
        self.apps
            .load_one(app)
            .map_err(|error| error.to_string())?
            .map(|stored| stored.file)
            .transpose()
    }

    /// The saved application `app`. Fails with "That application is no
    /// longer saved." when no application has that ID, and with the read
    /// error when its folder or its file cannot be read.
    pub(crate) fn app(&self, app: &str) -> Result<SavedApp, String> {
        Ok(SavedApp {
            file: self
                .file(app)?
                .ok_or_else(|| "That application is no longer saved.".to_owned())?,
            folder: self.apps.folder(app),
        })
    }
}

/// A saved application: its file, and the folder its pictures are in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SavedApp {
    /// The application's file.
    pub(crate) file: AppFile,
    /// Its folder.
    pub(crate) folder: PathBuf,
}

impl SavedApp {
    /// The picture `name` in the application's folder, as the runtime reads
    /// it for a value's check.
    ///
    /// # Errors
    ///
    /// Returns the error from reading it.
    pub(crate) fn picture(&self, name: &str) -> io::Result<Vec<u8>> {
        fs::read(self.folder.join(name))
    }

    /// The picture `name`, read only up to one byte past `most`, so a
    /// picture larger than `most` is known to be without reading it all.
    ///
    /// # Errors
    ///
    /// Returns the error from reading it.
    pub(crate) fn picture_within(&self, name: &str, most: u64) -> io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        fs::File::open(self.folder.join(name))?
            .take(most.saturating_add(1))
            .read_to_end(&mut bytes)?;
        Ok(bytes)
    }
}

/// Fresh, empty stores for one test, all in a folder named for `name`, which
/// the test removes when it ends.
#[cfg(test)]
pub(crate) fn scratch(name: &str) -> (Stores, PathBuf) {
    let folder = std::env::temp_dir().join(format!("appytappy-{name}-{}", std::process::id()));
    if folder.exists() {
        fs::remove_dir_all(&folder).expect("remove leftovers");
    }
    let locations =
        Locations::from_app_data(Some(folder.clone().into()), Some(folder.clone().into()))
            .expect("the temporary folder is absolute");
    (Stores::from(locations), folder)
}

#[cfg(test)]
mod tests {
    use appytappy_config::app_file::{AntiCheatDef, FORMAT, SizeChangesDef, WindowDef};
    use appytappy_config::window_match::WindowFacts;

    use super::*;

    #[test]
    fn an_application_loads_with_its_pictures_or_says_why_it_cannot() {
        let (stores, root) = scratch("stores-app");
        assert_eq!(stores.file("example"), Ok(None));
        assert_eq!(
            stores.app("example"),
            Err("That application is no longer saved.".to_owned())
        );
        let file = AppFile {
            format: FORMAT,
            name: "Example App".to_owned(),
            window: WindowDef::exact(WindowFacts {
                title: "Example App",
                class: "ExampleWindow",
                executable: "Example.exe",
            }),
            size_changes: SizeChangesDef::Often,
            anti_cheat: AntiCheatDef::Unknown,
            overlay: None,
            checked: None,
            values: Vec::new(),
            references: Vec::new(),
        };
        stores
            .apps
            .save("example", &file, &[("shown.png".to_owned(), vec![1, 2])])
            .expect("saves");
        let saved = stores.app("example").expect("loads");
        assert_eq!(saved.file, file);
        assert_eq!(stores.file("example"), Ok(Some(file)));
        assert_eq!(saved.picture("shown.png").expect("reads"), [1, 2]);
        assert_eq!(saved.picture_within("shown.png", 2).expect("reads"), [1, 2]);
        assert_eq!(
            saved.picture_within("shown.png", 0).expect("reads"),
            [1],
            "a picture past the most is read only one byte past it"
        );
        fs::write(saved.folder.join("app.json"), "{").expect("breaks the file");
        for read in [
            stores.app("example").map(drop),
            stores.file("example").map(drop),
        ] {
            assert!(
                read.is_err_and(|error| error.starts_with("the file is not valid")),
                "a broken file says why"
            );
        }
        fs::remove_dir_all(root).expect("clean up");
    }
}
