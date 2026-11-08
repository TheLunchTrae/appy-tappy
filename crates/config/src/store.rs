//! Reading and writing the authoring files.
//!
//! Each application has a folder under [`crate::paths::Locations::apps`]
//! named by its ID, holding `app.json` and the pictures its values name.
//! The automations live in one file of their own, at
//! [`crate::paths::Locations::automations`]. Every write goes to a temporary
//! file first and then replaces the old file in one step, so a reader never
//! sees half a file.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::app_file::{AppFile, is_image_name};
use crate::automation_file::AutomationsFile;

/// The authoring file's name in an application's folder.
pub const APP_FILE: &str = "app.json";

/// How many times a write tries to replace the old file. Antivirus and
/// backup tools can hold a file open for a moment.
const RENAME_ATTEMPTS: u32 = 5;

/// How long a write waits between those tries.
const RENAME_WAIT: Duration = Duration::from_millis(50);

/// How long a temporary file from another run is left alone before a save
/// removes it: far longer than any write takes, so a write that another copy
/// of the program still has under way is never removed.
const LEFTOVER_AGE: Duration = Duration::from_mins(10);

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
    /// show it rather than hide it. An entry whose type cannot be read is
    /// left out. A store that does not exist yet holds no applications.
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
            // An error here fails the listing: on Windows the next call can
            // fail the same way forever, so skipping it would never end.
            let entry = entry?;
            let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if !entry.file_type().is_ok_and(|kind| kind.is_dir()) || !is_app_id(&id) {
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
    /// picture that is not there yet. Temporary files that an earlier run
    /// left there go too.
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
        write_atomic(&folder.join(APP_FILE), file.to_json().as_bytes())?;
        remove_leftovers(&folder);
        Ok(())
    }

    /// Removes the application `id`'s folder with everything in it. An
    /// application that is not there is already removed.
    ///
    /// # Errors
    ///
    /// Returns an error when `id` is not valid or the removal fails.
    pub fn remove(&self, id: &str) -> io::Result<()> {
        if !is_app_id(id) {
            return Err(invalid(format!("{id:?} is not a valid application ID")));
        }
        match fs::remove_dir_all(self.folder(id)) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            removed => removed,
        }
    }

    /// Removes the pictures in the application `id`'s folder that `file` no
    /// longer names, after a value or reference was removed or changed.
    ///
    /// # Errors
    ///
    /// Returns the error from listing the folder or removing a picture.
    pub fn prune(&self, id: &str, file: &AppFile) -> io::Result<()> {
        let named = file.images();
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

/// The file that holds every automation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutomationStore {
    path: PathBuf,
}

impl AutomationStore {
    /// The store at `path`, such as [`crate::paths::Locations::automations`].
    #[must_use]
    pub const fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// The automations. A file that does not exist yet holds none. An
    /// automation that breaks a rule is kept, and
    /// [`AutomationsFile::problem`] says why.
    ///
    /// # Errors
    ///
    /// Returns the error from reading the file, or an invalid-data error
    /// saying why the file as a whole is broken.
    pub fn load(&self) -> io::Result<AutomationsFile> {
        match fs::read_to_string(&self.path) {
            Ok(json) => AutomationsFile::parse(&json)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(AutomationsFile::default()),
            Err(error) => Err(error),
        }
    }

    /// Replaces the automations with `file`, creating the folder it lives
    /// in. An automation that has a problem is written as it is, so an edit
    /// of another one keeps it; the caller checks the automations it
    /// changes. Temporary files that an earlier run left beside it go too.
    ///
    /// # Errors
    ///
    /// Returns an error when the file does not pass
    /// [`AutomationsFile::check`] or a write fails.
    pub fn save(&self, file: &AutomationsFile) -> io::Result<()> {
        file.check().map_err(|error| invalid(error.to_string()))?;
        let folder = self.path.parent();
        if let Some(folder) = folder {
            fs::create_dir_all(folder)?;
        }
        write_atomic(&self.path, file.to_json().as_bytes())?;
        if let Some(folder) = folder {
            remove_leftovers(folder);
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
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
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

/// Whether `name` is a temporary file that [`write_atomic`] made in another
/// run than the one with process ID `own`, such as one that a crash left
/// between the write and the rename. This run's own may be mid-write.
fn is_leftover(name: &str, own: u32) -> bool {
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
    name.strip_suffix(".tmp")
        .and_then(|rest| rest.rsplit_once('.'))
        .and_then(|(_file, mark)| mark.split_once('-'))
        .is_some_and(|(pid, stamp)| {
            digits(pid) && digits(stamp) && pid.parse::<u32>().ok() != Some(own)
        })
}

/// Removes the leftover temporary files in `folder` that have not changed
/// for [`LEFTOVER_AGE`], as far as it can: one that stays is tried again by
/// the next save.
fn remove_leftovers(folder: &Path) {
    let own = process::id();
    let now = SystemTime::now();
    for entry in fs::read_dir(folder).into_iter().flatten().flatten() {
        let old = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .is_ok_and(|modified| {
                now.duration_since(modified)
                    .is_ok_and(|age| age >= LEFTOVER_AGE)
            });
        if old
            && entry
                .file_name()
                .to_str()
                .is_some_and(|name| is_leftover(name, own))
        {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the save itself worked; a leftover that stays is only clutter"
            )]
            let _ = fs::remove_file(entry.path());
        }
    }
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
        AntiCheatDef, CheckDef, FORMAT, RegionDef, ScreenDef, SizeDef, StateDef, ValueDef,
        WindowDef,
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
            overlay: None,
            values: vec![ValueDef {
                id: "icon_visible".to_owned(),
                name: "Icon visible".to_owned(),
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
                    check: CheckDef::State {
                        states: vec![StateDef {
                            name: "Shown".to_owned(),
                            image: reference.to_owned(),
                        }],
                        tolerance: 0,
                    },
                },
            }],
            references: Vec::new(),
        }
    }

    #[test]
    fn saved_apps_load_back_sorted_with_broken_ones_explained() {
        let root = scratch("load");
        let store = AppStore::new(root.join("apps"));
        assert_eq!(store.load().expect("missing store"), []);
        let not_a_folder = root.join("file");
        fs::write(&not_a_folder, b"").expect("write");
        AppStore::new(not_a_folder)
            .load()
            .expect_err("a file is not a store");

        let saved = file("icon_visible.png");
        store
            .save(
                "zeta",
                &saved,
                &[("icon_visible.png".to_owned(), vec![1, 2, 3])],
            )
            .expect("saves");
        store
            .save("alpha", &saved, &[("icon_visible.png".to_owned(), vec![4])])
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
            fs::read(store.folder("zeta").join("icon_visible.png")).expect("picture"),
            [1, 2, 3]
        );
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn remove_takes_the_whole_folder_and_only_a_valid_id() {
        let root = scratch("remove");
        let store = AppStore::new(root.clone());
        store
            .save(
                "alpha",
                &file("icon_visible.png"),
                &[("icon_visible.png".to_owned(), vec![1])],
            )
            .expect("save");
        fs::create_dir_all(store.folder("beta")).expect("broken folder");
        store.remove("alpha").expect("remove");
        assert!(!store.folder("alpha").exists());
        store.remove("alpha").expect("removing again is fine");
        let refused = store.remove("../beta").expect_err("bad id");
        assert_eq!(refused.kind(), io::ErrorKind::InvalidInput);
        assert!(store.folder("beta").exists());
        // Any other failure, such as a file where the folder should be, is
        // reported.
        fs::write(store.folder("gamma"), b"not a folder").expect("stray file");
        store.remove("gamma").expect_err("not a folder");
        assert!(store.folder("gamma").exists());
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn save_refuses_bad_ids_pictures_and_files() {
        let root = scratch("refuse");
        let store = AppStore::new(root.clone());
        let good = file("icon_visible.png");
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
    fn automations_load_back_and_a_broken_file_says_why() {
        use crate::automation_file::{
            ActionDef, AutomationDef, ComparisonDef, MediaCommandDef, TargetDef, WhenDef,
        };

        let root = scratch("automations");
        let store = AutomationStore::new(root.join("roaming").join("automations.json"));
        assert_eq!(
            store.load().expect("missing file"),
            AutomationsFile::default()
        );
        let file = AutomationsFile {
            format: crate::automation_file::FORMAT,
            automations: vec![AutomationDef {
                id: "pause_music".to_owned(),
                name: "Pause music".to_owned(),
                enabled: true,
                when: WhenDef {
                    app: "example".to_owned(),
                    value: "icon_visible".to_owned(),
                    is: ComparisonDef::Equals,
                    target: TargetDef::Flag(true),
                },
                actions: vec![ActionDef::Media {
                    command: MediaCommandDef::Pause,
                    app: None,
                }],
                undo: true,
                cooldown_ms: 0,
            }],
        };
        store.save(&file).expect("saves, creating the folder");
        assert_eq!(store.load().expect("loads"), file);

        let mut broken = file.clone();
        broken.automations[0].actions.clear();
        store
            .save(&broken)
            .expect("an automation with a problem is kept");
        let loaded = store.load().expect("one automation's problem is its own");
        assert_eq!(
            loaded.problem(&loaded.automations[0]),
            Some("add an action".to_owned())
        );
        let mut other = file;
        other.format = 2;
        let refused = store.save(&other).expect_err("another format");
        assert_eq!(refused.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(store.load().expect("unchanged"), broken);

        fs::write(root.join("roaming").join("automations.json"), b"{").expect("write");
        let error = store.load().expect_err("broken");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(
            error
                .to_string()
                .starts_with("the automations file is not valid")
        );

        let folder = AutomationStore::new(root.clone());
        assert!(folder.load().is_err());
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

    /// Makes the file at `path` look untouched for longer than
    /// [`LEFTOVER_AGE`].
    fn age(path: &Path) {
        let then = SystemTime::now()
            .checked_sub(LEFTOVER_AGE.saturating_add(Duration::from_secs(1)))
            .expect("the clock is past the epoch by more than that");
        fs::File::options()
            .write(true)
            .open(path)
            .and_then(|file| file.set_modified(then))
            .expect("dates the file");
    }

    #[test]
    fn a_save_removes_temporaries_that_another_run_left_behind() {
        let other = process::id().wrapping_add(1);
        let own = process::id();
        assert!(is_leftover(&format!("app.json.{other}-1700.tmp"), own));
        assert!(is_leftover(&format!("sample-1.png.{other}-1.tmp"), own));
        assert!(!is_leftover(&format!("app.json.{own}-1700.tmp"), own));
        for kept in ["notes.tmp", "a.b-c.tmp", "a.1-.tmp", "a.-1.tmp", "app.json"] {
            assert!(!is_leftover(kept, own), "{kept}");
        }

        let root = scratch("leftovers");
        let apps = AppStore::new(root.join("apps"));
        let folder = apps.folder("app");
        fs::create_dir_all(&folder).expect("create");
        let left = format!("app.json.{other}-1700.tmp");
        let writing = format!("sample-1.png.{own}-1700.tmp");
        let fresh = format!("sample-2.png.{other}-1700.tmp");
        for name in [&left, &writing, &fresh] {
            fs::write(folder.join(name), b"x").expect("writes");
        }
        for name in [&left, &writing] {
            age(&folder.join(name));
        }
        apps.save(
            "app",
            &file("kept.png"),
            &[("kept.png".to_owned(), vec![1])],
        )
        .expect("saves");
        assert!(!folder.join(&left).exists(), "another run's old one goes");
        assert!(folder.join(&writing).exists(), "this run's stays");
        assert!(
            folder.join(&fresh).exists(),
            "another copy's write under way stays"
        );

        let automations = AutomationStore::new(root.join("automations.json"));
        let beside = root.join(format!("automations.json.{other}-1700.tmp"));
        fs::write(&beside, b"x").expect("writes");
        age(&beside);
        automations
            .save(&AutomationsFile::default())
            .expect("saves");
        assert!(!beside.exists());
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_rename_that_keeps_failing_gives_up_with_its_error() {
        let root = scratch("rename");
        let started = std::time::Instant::now();
        let error = rename_with_retries(&root.join("absent"), &root.join("target"))
            .expect_err("nothing to rename");
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        // It waited between every attempt before giving up.
        assert!(started.elapsed() >= RENAME_WAIT * (RENAME_ATTEMPTS - 1));
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn app_ids_are_plain_folder_names() {
        for id in ["example", "app64", "my-app_2", "9lives"] {
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
        assert_eq!(new_app_id("My-Example-App64.exe", &[]), "my-example-app64");
        assert_eq!(new_app_id("app64", &[]), "app64");
        assert_eq!(new_app_id("--.exe", &[]), "app");
        assert_eq!(new_app_id("プレイ.exe", &[]), "app");
        let long = new_app_id(&format!("{}.exe", "a".repeat(80)), &[]);
        assert_eq!(long.len(), 44);
        assert!(is_app_id(&long));
    }
}
