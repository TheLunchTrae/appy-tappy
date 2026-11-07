//! Reading and writing the applications' authoring files.
//!
//! Each application has a folder under [`crate::paths::Locations::apps`]
//! named by its ID, holding `app.json` and the pictures its values name.
//! Every write goes to a temporary file first and then replaces the old file
//! in one step, so a reader never sees half a file.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::app_file::{AppFile, is_image_name};

/// The authoring file's name in an application's folder.
pub const APP_FILE: &str = "app.json";

/// How many times a write tries to replace the old file. Antivirus and
/// backup tools can hold a file open for a moment.
const RENAME_ATTEMPTS: u32 = 5;

/// How long a write waits between those tries.
const RENAME_WAIT: Duration = Duration::from_millis(50);

/// The longest application ID, in bytes.
const MAX_ID_LEN: usize = 48;

/// One application's folder, as [`AppStore::load`] found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredApp {
    /// The folder's name, which identifies the application.
    pub id: String,
    /// The file, or why it could not be read.
    pub file: Result<AppFile, String>,
}

/// The folder of application folders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppStore {
    root: PathBuf,
}

impl AppStore {
    /// The store at `root`, such as [`crate::paths::Locations::apps`].
    #[must_use]
    pub const fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Every application folder, sorted by ID. A folder whose file is
    /// missing or broken is listed with the reason, so the setup app can
    /// show it rather than hide it. A store that does not exist yet holds no
    /// applications.
    ///
    /// # Errors
    ///
    /// Returns the error from listing the store's folder.
    pub fn load(&self) -> io::Result<Vec<StoredApp>> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let mut apps = Vec::new();
        for entry in entries {
            let entry = entry?;
            let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if !entry.file_type()?.is_dir() || !is_app_id(&id) {
                continue;
            }
            let file = fs::read_to_string(entry.path().join(APP_FILE))
                .map_err(|error| error.to_string())
                .and_then(|json| AppFile::parse(&json).map_err(|error| error.to_string()));
            apps.push(StoredApp { id, file });
        }
        apps.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(apps)
    }

    /// The folder of the application `id`.
    #[must_use]
    pub fn folder(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    /// Writes `pictures` and then `file` into the application `id`'s folder,
    /// creating it. The pictures come first, so the file never names a
    /// picture that is not there yet.
    ///
    /// # Errors
    ///
    /// Returns an error when `id` or a picture's name is not valid, the file
    /// does not pass [`AppFile::check`], or a write fails.
    pub fn save(&self, id: &str, file: &AppFile, pictures: &[(String, Vec<u8>)]) -> io::Result<()> {
        if !is_app_id(id) {
            return Err(invalid(format!("{id:?} is not a valid application ID")));
        }
        file.check().map_err(|error| invalid(error.to_string()))?;
        let folder = self.folder(id);
        fs::create_dir_all(&folder)?;
        for (name, bytes) in pictures {
            if !is_image_name(name) {
                return Err(invalid(format!("{name:?} is not a valid picture name")));
            }
            write_atomic(&folder.join(name), bytes)?;
        }
        write_atomic(&folder.join(APP_FILE), file.to_json().as_bytes())
    }

    /// Removes the pictures in the application `id`'s folder that `file` no
    /// longer names, after a value was removed or changed.
    ///
    /// # Errors
    ///
    /// Returns the error from listing the folder or removing a picture.
    pub fn prune(&self, id: &str, file: &AppFile) -> io::Result<()> {
        let named: Vec<&str> = file
            .values
            .iter()
            .flat_map(|value| value.screen.check.images())
            .collect();
        for entry in fs::read_dir(self.folder(id))? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if is_image_name(name) && !named.contains(&name) {
                fs::remove_file(entry.path())?;
            }
        }
        Ok(())
    }
}

/// Whether `id` can name an application's folder.
///
/// That is lowercase ASCII letters, digits, hyphens, and underscores,
/// starting with a letter or digit, so it is the same folder on every file
/// system and never leaves the store.
#[must_use]
pub fn is_app_id(id: &str) -> bool {
    id.len() <= MAX_ID_LEN
        && id
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_lowercase() || first.is_ascii_digit())
        && id.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '-'
                || character == '_'
        })
}

/// An application ID made from an executable's name that no folder in
/// `taken` uses yet, such as `example` from `Example.exe`, or
/// `example-2` when that is taken.
#[must_use]
pub fn new_app_id(executable: &str, taken: &[&str]) -> String {
    let stem = executable
        .rsplit_once('.')
        .map_or(executable, |(stem, _extension)| stem);
    let mut base: String = stem
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || *character == '-'
        })
        .collect();
    let base_limit = MAX_ID_LEN.saturating_sub(4);
    base.truncate(base_limit);
    let base = base.trim_start_matches('-');
    let base = if base.is_empty() { "app" } else { base };
    let free = |candidate: &str| is_app_id(candidate) && !taken.contains(&candidate);
    if free(base) {
        return base.to_owned();
    }
    // `taken` is finite, so one of its length plus one numbers is free.
    (2_usize..=taken.len().saturating_add(2))
        .map(|number| format!("{base}-{number}"))
        .find(|candidate| free(candidate))
        .unwrap_or_else(|| base.to_owned())
}

/// Writes `bytes` to `path` so that a reader sees the old file or the whole
/// new one: it writes a temporary file beside it, flushes it to disk, and
/// renames it over `path`, trying the rename again a few times.
///
/// # Errors
///
/// Returns the error from writing the temporary file, or from the last
/// rename, after removing the temporary file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(format!(".{}-{stamp}.tmp", process::id()));
    let temporary = PathBuf::from(temporary);
    let written = fs::File::create(&temporary).and_then(|mut file| {
        file.write_all(bytes)?;
        file.sync_all()
    });
    let renamed = written.and_then(|()| rename_with_retries(&temporary, path));
    if renamed.is_err() {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the write's own error is the one to report; the leftover is only cleanup"
        )]
        let _ = fs::remove_file(&temporary);
    }
    renamed
}

/// Renames `from` over `to`, trying [`RENAME_ATTEMPTS`] times.
fn rename_with_retries(from: &Path, to: &Path) -> io::Result<()> {
    let mut attempt = 1;
    loop {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(error) if attempt >= RENAME_ATTEMPTS => return Err(error),
            Err(_busy) => {
                thread::sleep(RENAME_WAIT);
                attempt = attempt.saturating_add(1);
            },
        }
    }
}

/// An invalid-input error with `message`.
fn invalid(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

#[cfg(test)]
mod tests {
    use std::env;

    use super::*;
    use crate::app_file::{
        AntiCheatDef, CheckDef, FORMAT, RegionDef, ScreenDef, SizeDef, ValueDef, WindowDef,
    };
    use crate::window_match::WindowFacts;

    /// A fresh, empty folder for one test.
    fn scratch(name: &str) -> PathBuf {
        let folder = env::temp_dir().join(format!("appytappy-store-{name}-{}", process::id()));
        // A leftover from an earlier run would change what the test sees.
        if folder.exists() {
            fs::remove_dir_all(&folder).expect("remove leftovers");
        }
        fs::create_dir_all(&folder).expect("create");
        folder
    }

    fn file(reference: &str) -> AppFile {
        AppFile {
            format: FORMAT,
            name: "Example App".to_owned(),
            window: WindowDef::exact(WindowFacts {
                title: "Example App",
                class: "ExampleWindow",
                executable: "Example.exe",
            }),
            anti_cheat: AntiCheatDef::Unknown,
            values: vec![ValueDef {
                id: "in_menu".to_owned(),
                name: "In menu".to_owned(),
                screen: ScreenDef {
                    window_size: SizeDef {
                        width: 100,
                        height: 50,
                    },
                    region: RegionDef {
                        x: 0,
                        y: 0,
                        width: 10,
                        height: 10,
                    },
                    check: CheckDef::Reference {
                        image: reference.to_owned(),
                        tolerance: 0,
                    },
                },
            }],
        }
    }

    #[test]
    fn saved_apps_load_back_sorted_with_broken_ones_explained() {
        let root = scratch("load");
        let store = AppStore::new(root.join("apps"));
        assert_eq!(store.load().expect("missing store"), []);

        let saved = file("in_menu.png");
        store
            .save("zeta", &saved, &[("in_menu.png".to_owned(), vec![1, 2, 3])])
            .expect("saves");
        store
            .save("alpha", &saved, &[("in_menu.png".to_owned(), vec![4])])
            .expect("saves");
        fs::create_dir_all(store.folder("broken")).expect("create");
        fs::write(store.folder("broken").join(APP_FILE), "{").expect("write");
        fs::create_dir_all(store.folder("Not An Id")).expect("create");
        fs::write(store.root.join("loose.txt"), "x").expect("write");

        let apps = store.load().expect("loads");
        let ids: Vec<&str> = apps.iter().map(|app| app.id.as_str()).collect();
        assert_eq!(ids, ["alpha", "broken", "zeta"]);
        assert_eq!(apps[0].file, Ok(saved));
        assert!(
            apps[1]
                .file
                .as_ref()
                .is_err_and(|error| error.contains("not valid"))
        );
        assert_eq!(
            fs::read(store.folder("zeta").join("in_menu.png")).expect("picture"),
            [1, 2, 3]
        );
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn save_refuses_bad_ids_pictures_and_files() {
        let root = scratch("refuse");
        let store = AppStore::new(root.clone());
        let good = file("in_menu.png");
        let bad_id = store.save("../up", &good, &[]).expect_err("bad id");
        assert_eq!(bad_id.kind(), io::ErrorKind::InvalidInput);
        let bad_picture = store
            .save("app", &good, &[("../x.png".to_owned(), Vec::new())])
            .expect_err("bad picture");
        assert!(bad_picture.to_string().contains("not a valid picture name"));
        let mut broken = good;
        broken.name = String::new();
        let bad_file = store.save("app", &broken, &[]).expect_err("bad file");
        assert!(bad_file.to_string().contains("no name"));
        assert!(!store.folder("app").join(APP_FILE).exists());
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn prune_removes_only_pictures_the_file_no_longer_names() {
        let root = scratch("prune");
        let store = AppStore::new(root.clone());
        let pictures = [
            ("old.png".to_owned(), vec![1]),
            ("kept.png".to_owned(), vec![2]),
        ];
        store
            .save("app", &file("kept.png"), &pictures)
            .expect("saves");
        fs::write(store.folder("app").join("notes.txt"), "mine").expect("write");
        store.prune("app", &file("kept.png")).expect("prunes");
        let mut left: Vec<String> = fs::read_dir(store.folder("app"))
            .expect("lists")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .into_string()
                    .expect("utf-8")
            })
            .collect();
        left.sort();
        assert_eq!(left, ["app.json", "kept.png", "notes.txt"]);
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn writes_replace_the_whole_file_and_leave_no_temporary_behind() {
        let root = scratch("atomic");
        let path = root.join("file.json");
        write_atomic(&path, b"first").expect("writes");
        write_atomic(&path, b"second").expect("replaces");
        assert_eq!(fs::read(&path).expect("reads"), b"second");
        let missing = root.join("no-such-folder").join("file.json");
        assert!(write_atomic(&missing, b"x").is_err());
        let names: Vec<_> = fs::read_dir(&root)
            .expect("lists")
            .map(|entry| entry.expect("entry").file_name())
            .collect();
        assert_eq!(names, ["file.json"]);
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_rename_that_keeps_failing_gives_up_with_its_error() {
        let root = scratch("rename");
        let error = rename_with_retries(&root.join("absent"), &root.join("target"))
            .expect_err("nothing to rename");
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn app_ids_are_plain_folder_names() {
        for id in ["example", "obs64", "my-app_2", "9lives"] {
            assert!(is_app_id(id), "{id}");
        }
        let longest = "a".repeat(48);
        assert!(is_app_id(&longest));
        let long = "a".repeat(49);
        for id in [
            "",
            "-a",
            "_a",
            "A",
            "a b",
            "a/b",
            "..",
            "a.b",
            long.as_str(),
        ] {
            assert!(!is_app_id(id), "{id:?}");
        }
    }

    #[test]
    fn new_ids_come_from_the_executable_and_avoid_taken_ones() {
        assert_eq!(new_app_id("Example.exe", &[]), "example");
        assert_eq!(new_app_id("Example.exe", &["example"]), "example-2");
        assert_eq!(
            new_app_id("Example.exe", &["example", "example-2"]),
            "example-3"
        );
        assert_eq!(
            new_app_id("VALORANT-Win64-Shipping.exe", &[]),
            "valorant-win64-shipping"
        );
        assert_eq!(new_app_id("obs64", &[]), "obs64");
        assert_eq!(new_app_id("--.exe", &[]), "app");
        assert_eq!(new_app_id("プレイ.exe", &[]), "app");
        let long = new_app_id(&format!("{}.exe", "a".repeat(80)), &[]);
        assert_eq!(long.len(), 44);
        assert!(is_app_id(&long));
    }
}
