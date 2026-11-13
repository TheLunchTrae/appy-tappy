//! Drafts: the automations, applications, values, fixes, and references the
//! user saved before they were finished, to finish later.
//!
//! The setup app keeps each draft in its own folder under the drafts folder:
//! `draft.json`, and for a value, fix, or reference the screenshots its
//! capture held, each under the index it had there: `shot-{index}.png` for
//! one the user took, `recorded-{index}.png` for one a recording of looks
//! kept. The
//! runtime never reads the folder, so a draft never runs, and the
//! authoring files never hold an unfinished item. The page keeps what each
//! flow had set up in the draft's `state`, which this side stores as it is.

use std::collections::HashSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// The file in a draft's folder that holds the draft.
const DRAFT_FILE: &str = "draft.json";

/// The end of the name of the folder a draft is written to before it takes
/// the place of the old one; it is not a draft ID, so the list skips it.
const SAVING: &str = ".saving";

/// The end of the name the old draft is moved to while the new one takes its
/// place; it is not a draft ID either.
const OLD: &str = ".old";

/// The longest draft ID.
const MAX_ID_LEN: usize = 32;

/// What a draft is of, which says which flow finishes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) enum DraftKind {
    Automation,
    Application,
    Value,
    /// A saved value drawn again at another window size.
    Fix,
    Reference,
}

/// A draft as the page saves it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DraftDef {
    pub(crate) kind: DraftKind,
    /// The ID of the saved application a value, fix, or reference belongs
    /// to, or null.
    pub(crate) app: Option<String>,
    /// The ID of the saved item it changes: an automation, an application, a
    /// value a fix is of, or a reference. Null for a new one. A saved item
    /// has one draft at most.
    pub(crate) changes: Option<String>,
    /// What the lists call it.
    pub(crate) name: String,
    /// What is still to do before it can be saved, a sentence each.
    pub(crate) missing: Vec<String>,
    /// The click pictures it names, which saving the automations keeps.
    pub(crate) pictures: Vec<String>,
    /// What the flow had set up, which only the page reads.
    #[cfg_attr(test, ts(type = "unknown"))]
    pub(crate) state: serde_json::Value,
}

/// A screenshot a draft keeps: the index it had in its capture, and whether
/// a recording of looks kept it there rather than the user taking it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub(crate) struct DraftShot {
    pub(crate) index: usize,
    pub(crate) recorded: bool,
}

/// A saved draft, as the lists show it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct DraftSummary {
    pub(crate) id: String,
    /// The draft, or null when its file cannot be read.
    pub(crate) draft: Option<DraftDef>,
    /// Why its file cannot be read, or null.
    pub(crate) problem: Option<String>,
    /// How many screenshots it keeps.
    pub(crate) screenshots: u32,
}

/// Whether `id` can name a draft: lowercase letters, digits and dashes, so
/// it is a plain folder name.
fn is_draft_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_LEN
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// The error for an ID that cannot name a draft.
fn bad_id() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "that is not a draft's ID")
}

/// The name of the file a draft keeps `shot` in.
fn shot_name(shot: DraftShot) -> String {
    let kind = if shot.recorded { "recorded" } else { "shot" };
    format!("{kind}-{}.png", shot.index)
}

/// The screenshot a draft keeps as `name`, or `None` for any other file.
/// Only a name [`shot_name`] gives counts.
fn draft_shot(name: &str) -> Option<DraftShot> {
    let (recorded, rest) = match name.strip_prefix("recorded-") {
        Some(rest) => (true, rest),
        None => (false, name.strip_prefix("shot-")?),
    };
    let index = rest.strip_suffix(".png")?.parse().ok()?;
    let shot = DraftShot { index, recorded };
    (shot_name(shot) == name).then_some(shot)
}

/// The folder of drafts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DraftStore {
    folder: PathBuf,
}

impl DraftStore {
    pub(crate) const fn new(folder: PathBuf) -> Self {
        Self { folder }
    }

    /// The folder of the draft `id`.
    fn draft_folder(&self, id: &str) -> io::Result<PathBuf> {
        if is_draft_id(id) {
            Ok(self.folder.join(id))
        } else {
            Err(bad_id())
        }
    }

    /// Every draft, oldest first, after putting back any a stopped save left
    /// moved aside; a folder without a draft file, or one a save is still
    /// writing, is left out. No folder yet holds none.
    ///
    /// # Errors
    ///
    /// Returns the error from listing the folder.
    pub(crate) fn list(&self) -> io::Result<Vec<DraftSummary>> {
        self.tidy();
        let mut drafts = Vec::new();
        for (id, folder) in self.entries()? {
            if !is_draft_id(&id) || !folder.join(DRAFT_FILE).is_file() {
                continue;
            }
            let (draft, problem) = match read_draft(&folder) {
                Ok(draft) => (Some(draft), None),
                Err(error) => (None, Some(error)),
            };
            drafts.push(DraftSummary {
                screenshots: u32::try_from(shots(&folder).len()).unwrap_or(u32::MAX),
                id,
                draft,
                problem,
            });
        }
        // IDs are made from the time, so they sort in the order the drafts
        // were first saved.
        drafts.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(drafts)
    }

    /// Saves `draft` over the draft `id`, or as a new one, with `screenshots`,
    /// each a PNG with what its capture says of it; a draft without
    /// screenshots keeps none. Another draft of the same saved automation, application,
    /// or reference goes, so the item has one; a value can have several fix
    /// drafts, so fixes are left alone. Answers the draft's ID.
    ///
    /// The draft is written to a folder of its own, each file flushed to the
    /// disk, and only then swapped in for the old one, so a save that fails
    /// or stops partway leaves the old draft, or puts it back at the next
    /// list.
    ///
    /// # Errors
    ///
    /// Returns the error from writing a file, or from reading the folder
    /// when `id` is new.
    pub(crate) fn save(
        &self,
        id: Option<&str>,
        draft: &DraftDef,
        screenshots: &[(DraftShot, Vec<u8>)],
    ) -> io::Result<String> {
        let existing = self.list()?;
        let id = if let Some(id) = id {
            self.draft_folder(id)?;
            id.to_owned()
        } else {
            let taken: HashSet<&str> = existing.iter().map(|summary| summary.id.as_str()).collect();
            new_id(&taken)
        };
        let folder = self.draft_folder(&id)?;
        // Each save has folders of its own, so one a stopped save left, and
        // could not remove yet, never mixes with this one.
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        let staging = self.folder.join(format!("{id}.{stamp}{SAVING}"));
        fs::create_dir_all(&staging)?;
        for (shot, bytes) in screenshots {
            write_synced(&staging.join(shot_name(*shot)), bytes)?;
        }
        let json = serde_json::to_string_pretty(draft).map_err(io::Error::other)?;
        write_synced(&staging.join(DRAFT_FILE), json.as_bytes())?;
        self.swap_in(
            &staging,
            &folder,
            &self.folder.join(format!("{id}.{stamp}{OLD}")),
        )?;
        if draft.changes.is_some() && draft.kind != DraftKind::Fix {
            for other in existing.iter().filter(|other| {
                other.id != id
                    && other.draft.as_ref().is_some_and(|other| {
                        other.kind == draft.kind
                            && other.app == draft.app
                            && other.changes == draft.changes
                    })
            }) {
                // The new draft is in place, so the save worked. An older
                // draft of the same item that stays is listed before it, and
                // the page takes the newest.
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the draft is saved; an older one that stays shows on its page"
                )]
                let _ = self.delete(&other.id);
            }
        }
        Ok(id)
    }

    /// Puts the draft written to `staging` in place of the one at `folder`,
    /// which is first moved to `aside`. Tidying then removes the old one, or
    /// puts it back when the new one did not take its place, so one of the
    /// two is always there.
    fn swap_in(&self, staging: &Path, folder: &Path, aside: &Path) -> io::Result<()> {
        if folder.exists() {
            fs::rename(folder, aside)?;
        }
        let moved = fs::rename(staging, folder);
        self.tidy();
        moved
    }

    /// Puts back an old draft that a stopped save moved aside without putting
    /// the new one in its place, and removes what stopped saves left. A save
    /// stopped there when its own `.saving` folder is still beside the old
    /// one, and that folder stays until the old one is back; an old one
    /// without it was replaced, so it goes. A folder that cannot be moved or
    /// removed now is tried again next time.
    fn tidy(&self) {
        let Ok(entries) = self.entries() else {
            return;
        };
        for (name, path) in &entries {
            let Some(save) = name.strip_suffix(OLD) else {
                continue;
            };
            let id = save.split_once('.').map_or(save, |(id, _)| id);
            let folder = self.folder.join(id);
            let stopped = self.folder.join(format!("{save}{SAVING}")).exists();
            #[expect(
                clippy::let_underscore_must_use,
                reason = "tidying is tried again at the next list"
            )]
            let _ = if !stopped {
                fs::remove_dir_all(path)
            } else if !folder.exists() {
                fs::rename(path, folder)
            } else {
                // Something is in the draft's place; the copy waits.
                Ok(())
            };
        }
        for (name, path) in &entries {
            // A save's folder stays while its old draft is still aside, so a
            // put back that failed is tried again rather than taken as done.
            let waits = name
                .strip_suffix(SAVING)
                .is_some_and(|save| self.folder.join(format!("{save}{OLD}")).exists());
            if name.ends_with(SAVING) && !waits {
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "tidying is tried again at the next list"
                )]
                let _ = fs::remove_dir_all(path);
            }
        }
    }

    /// What the drafts folder holds, by name, leaving out names that are not
    /// text. No folder yet holds nothing.
    fn entries(&self) -> io::Result<Vec<(String, PathBuf)>> {
        let entries = match fs::read_dir(&self.folder) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            entries => entries?,
        };
        let mut named = Vec::new();
        for entry in entries {
            let entry = entry?;
            if let Ok(name) = entry.file_name().into_string() {
                named.push((name, entry.path()));
            }
        }
        Ok(named)
    }

    /// The screenshots of the draft `id`, each a PNG with what its capture
    /// said of it, lowest index first.
    ///
    /// # Errors
    ///
    /// Returns the error from reading one.
    pub(crate) fn screenshots(&self, id: &str) -> io::Result<Vec<(DraftShot, Vec<u8>)>> {
        let folder = self.draft_folder(id)?;
        shots(&folder)
            .into_iter()
            .map(|shot| Ok((shot, fs::read(folder.join(shot_name(shot)))?)))
            .collect()
    }

    /// Removes the draft `id` with its screenshots. One that is already gone
    /// is fine.
    ///
    /// # Errors
    ///
    /// Returns the error from removing its folder.
    pub(crate) fn delete(&self, id: &str) -> io::Result<()> {
        let folder = self.draft_folder(id)?;
        // What a stopped save left goes first: without its `.saving` folder,
        // a copy moved aside is never put back. A copy that cannot be removed
        // now is removed at the next list.
        let prefix = format!("{id}.");
        let entries = self.entries()?;
        for (name, path) in &entries {
            if name.starts_with(&prefix) && name.ends_with(SAVING) {
                fs::remove_dir_all(path)?;
            }
        }
        for (name, path) in &entries {
            if name.starts_with(&prefix) && name.ends_with(OLD) {
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "without its save's folder, the next list removes it"
                )]
                let _ = fs::remove_dir_all(path);
            }
        }
        match fs::remove_dir_all(folder) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            removed => removed,
        }
    }

    /// The click pictures the drafts name, which saving the automations
    /// keeps.
    ///
    /// # Errors
    ///
    /// Returns the error from listing the folder, or an error while a draft
    /// cannot be read: nothing says which pictures that one names, so the
    /// caller must keep them all until it reads again or is deleted.
    pub(crate) fn pictures(&self) -> io::Result<Vec<String>> {
        let mut named = Vec::new();
        for summary in self.list()? {
            let draft = summary.draft.ok_or_else(|| {
                io::Error::other(format!("the draft {} cannot be read", summary.id))
            })?;
            named.extend(draft.pictures);
        }
        Ok(named)
    }

    /// Deletes each draft that `gone` takes, such as those of an application
    /// that was removed, which nothing can finish. A draft that cannot be
    /// read is left, since nothing says what it is of.
    ///
    /// # Errors
    ///
    /// Returns the error from listing the folder or from the first delete
    /// that fails.
    pub(crate) fn delete_where(&self, gone: impl Fn(&DraftDef) -> bool) -> io::Result<()> {
        for summary in self.list()? {
            if summary.draft.as_ref().is_some_and(&gone) {
                self.delete(&summary.id)?;
            }
        }
        Ok(())
    }
}

/// Writes `bytes` to `path` and flushes them to the disk, so a draft that is
/// in place holds what was written.
fn write_synced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// The draft in `folder`, or why it cannot be read, as a sentence.
fn read_draft(folder: &Path) -> Result<DraftDef, String> {
    let text = fs::read_to_string(folder.join(DRAFT_FILE))
        .map_err(|error| format!("Its file cannot be read: {error}."))?;
    serde_json::from_str(&text).map_err(|error| format!("Its file is not valid: {error}."))
}

/// The screenshots `folder` holds, lowest index first. A capture leaves the
/// index of a screenshot the user removed empty, so the indexes can have
/// gaps. A folder that cannot be listed holds none.
fn shots(folder: &Path) -> Vec<DraftShot> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut shots: Vec<DraftShot> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_str().and_then(draft_shot))
        .collect();
    shots.sort_unstable_by_key(|shot| shot.index);
    shots
}

/// A new draft ID that `taken` does not hold, made from the time so the IDs
/// sort in the order drafts are made.
fn new_id(taken: &HashSet<&str>) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis());
    // One more stamp than `taken` holds is sure to find one it doesn't.
    let span = u128::try_from(taken.len()).unwrap_or(u128::MAX);
    (now..=now.saturating_add(span))
        .map(|stamp| format!("{stamp:016x}"))
        .find(|id| !taken.contains(id.as_str()))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn scratch(name: &str) -> (DraftStore, PathBuf) {
        let folder =
            std::env::temp_dir().join(format!("appytappy-drafts-{name}-{}", std::process::id()));
        if folder.exists() {
            fs::remove_dir_all(&folder).expect("remove leftovers");
        }
        (DraftStore::new(folder.join("drafts")), folder)
    }

    /// Screenshot `index`, which the user took.
    const fn taken(index: usize) -> DraftShot {
        DraftShot {
            index,
            recorded: false,
        }
    }

    /// Screenshot `index`, which a recording of looks kept.
    const fn recorded(index: usize) -> DraftShot {
        DraftShot {
            index,
            recorded: true,
        }
    }

    fn draft(kind: DraftKind, changes: Option<&str>) -> DraftDef {
        DraftDef {
            kind,
            app: Some("example".to_owned()),
            changes: changes.map(str::to_owned),
            name: "Boss health".to_owned(),
            missing: vec!["Pick the filled color".to_owned()],
            pictures: vec!["click-1.png".to_owned()],
            state: json!({ "format": 1, "valueName": "Boss health" }),
        }
    }

    #[test]
    fn a_draft_saves_with_its_screenshots_and_goes() {
        let (store, root) = scratch("save");
        assert_eq!(store.list().expect("lists"), [], "no folder holds none");
        let saved = draft(DraftKind::Value, None);
        let id = store
            .save(
                None,
                &saved,
                &[
                    (taken(0), vec![1]),
                    (recorded(2), vec![2, 2]),
                    (taken(10), vec![3]),
                ],
            )
            .expect("saves");
        // Only files named as a screenshot is saved count as one.
        for other in [
            "shot-01.png",
            "shot-1.png.bak",
            "shot-x.png",
            "recorded-02.png",
            "recorded-shot-3.png",
            "notes.png",
        ] {
            fs::write(store.folder.join(&id).join(other), [7]).expect("writes");
        }
        assert!(is_draft_id(&id), "{id}");
        assert_eq!(
            store.list().expect("lists"),
            [DraftSummary {
                id: id.clone(),
                draft: Some(saved.clone()),
                problem: None,
                screenshots: 3,
            }]
        );
        assert_eq!(
            store.screenshots(&id).expect("reads"),
            [
                (taken(0), vec![1]),
                (recorded(2), vec![2, 2]),
                (taken(10), vec![3])
            ],
            "each keeps its index and mark, gaps included, lowest first"
        );
        assert_eq!(store.pictures().expect("lists"), ["click-1.png"]);

        // Saving it again with fewer screenshots drops the rest.
        store
            .save(Some(&id), &saved, &[(taken(0), vec![3])])
            .expect("saves");
        assert_eq!(
            store.screenshots(&id).expect("reads"),
            [(taken(0), vec![3])]
        );
        assert_eq!(store.list().expect("lists").len(), 1, "saved over");

        store.delete(&id).expect("deletes");
        store.delete(&id).expect("one already gone is fine");
        assert_eq!(store.list().expect("lists"), []);
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_saved_item_keeps_one_draft() {
        let (store, root) = scratch("changes");
        let first = store
            .save(None, &draft(DraftKind::Automation, Some("pause")), &[])
            .expect("saves");
        let other = store
            .save(None, &draft(DraftKind::Automation, Some("other")), &[])
            .expect("saves");
        let new_one = store
            .save(None, &draft(DraftKind::Automation, None), &[])
            .expect("saves");
        let second_new = store
            .save(None, &draft(DraftKind::Automation, None), &[])
            .expect("saves");
        let reference = store
            .save(None, &draft(DraftKind::Reference, Some("pause")), &[])
            .expect("saves");
        let again = store
            .save(None, &draft(DraftKind::Automation, Some("pause")), &[])
            .expect("saves");
        let ids: Vec<String> = store
            .list()
            .expect("lists")
            .into_iter()
            .map(|summary| summary.id)
            .collect();
        assert!(
            !ids.contains(&first),
            "the newer draft of pause replaced it"
        );
        for kept in [&other, &new_one, &second_new, &reference, &again] {
            assert!(ids.contains(kept), "{kept}");
        }
        assert_eq!(ids.len(), 5);
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_value_keeps_each_of_its_fix_drafts() {
        let (store, root) = scratch("fixes");
        let small = store
            .save(
                None,
                &draft(DraftKind::Fix, Some("health")),
                &[(taken(0), vec![1])],
            )
            .expect("saves");
        let large = store
            .save(
                None,
                &draft(DraftKind::Fix, Some("health")),
                &[(taken(0), vec![2])],
            )
            .expect("saves");
        assert_eq!(store.list().expect("lists").len(), 2);
        assert_eq!(
            store.screenshots(&small).expect("reads"),
            [(taken(0), vec![1])]
        );
        assert_eq!(
            store.screenshots(&large).expect("reads"),
            [(taken(0), vec![2])]
        );
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_save_that_stopped_leaves_the_old_draft() {
        let (store, root) = scratch("stopped");
        let saved = draft(DraftKind::Value, None);
        let id = store
            .save(None, &saved, &[(taken(0), vec![1])])
            .expect("saves");
        // A save that stopped partway leaves its own folder, never the draft's.
        let staging = store.folder.join(format!("{id}.1{SAVING}"));
        fs::create_dir_all(&staging).expect("folder");
        fs::write(staging.join(DRAFT_FILE), "{").expect("writes");
        fs::write(staging.join(shot_name(taken(0))), [9]).expect("writes");
        let listed = store.list().expect("lists");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].draft.as_ref(), Some(&saved));
        assert_eq!(
            store.screenshots(&id).expect("reads"),
            [(taken(0), vec![1])]
        );
        assert!(!staging.exists(), "listing clears it away");

        // A save that stopped after moving the old draft aside, before the
        // new one took its place, gets the old one back at the next list.
        let folder = store.draft_folder(&id).expect("an ID");
        let aside = store.folder.join(format!("{id}.2{OLD}"));
        let stopped = store.folder.join(format!("{id}.2{SAVING}"));
        fs::rename(&folder, &aside).expect("moves aside");
        fs::create_dir_all(&stopped).expect("folder");
        let listed = store.list().expect("lists");
        assert_eq!(listed.len(), 1);
        assert_eq!(
            store.screenshots(&id).expect("reads"),
            [(taken(0), vec![1])]
        );
        assert!(!aside.exists());
        assert!(!stopped.exists());

        // While the old draft cannot be put back, here because a folder is
        // in its place, the save's folder stays, so it is tried again.
        fs::rename(&folder, &aside).expect("moves aside");
        fs::create_dir_all(&stopped).expect("folder");
        fs::write(&folder, b"in the way").expect("writes");
        store.list().expect("lists");
        assert!(
            stopped.exists(),
            "the save's folder waits for the old draft"
        );
        assert!(aside.exists());
        fs::remove_file(&folder).expect("clears the way");
        store.list().expect("lists");
        assert_eq!(
            store.screenshots(&id).expect("reads"),
            [(taken(0), vec![1])]
        );
        assert!(!stopped.exists());
        assert!(!aside.exists());

        // One left behind once the new draft is in place goes.
        fs::create_dir_all(&aside).expect("folder");
        store
            .save(Some(&id), &saved, &[(taken(0), vec![3])])
            .expect("saves");
        assert!(!aside.exists());
        assert_eq!(
            store.screenshots(&id).expect("reads"),
            [(taken(0), vec![3])]
        );

        // One left behind by a save that finished never brings back a draft
        // deleted later, and deleting removes it anyway.
        fs::create_dir_all(&aside).expect("folder");
        fs::rename(&folder, store.folder.join("Kept aside")).expect("moves");
        assert_eq!(
            store.list().expect("lists"),
            [],
            "without its save, it is not put back"
        );
        assert!(!aside.exists());
        fs::rename(store.folder.join("Kept aside"), &folder).expect("moves back");
        fs::create_dir_all(&aside).expect("folder");
        fs::create_dir_all(&stopped).expect("folder");
        let other = store.folder.join(format!("other.9{OLD}"));
        let other_save = store.folder.join(format!("other.9{SAVING}"));
        fs::create_dir_all(&other).expect("folder");
        fs::create_dir_all(&other_save).expect("folder");
        store.delete(&id).expect("deletes");
        assert!(!aside.exists(), "deleting takes it");
        assert!(!stopped.exists(), "and the save's folder");
        assert!(other.exists(), "and leaves another draft's");
        assert!(other_save.exists());
        assert_eq!(store.list().expect("lists"), []);
        store
            .save(Some(&id), &saved, &[(taken(0), vec![3])])
            .expect("saves");

        // A stray file with a save's ending, which cannot be removed as a
        // folder, never stops a save.
        fs::write(store.folder.join(format!("{id}.3{SAVING}")), b"stray").expect("writes");
        store
            .save(Some(&id), &saved, &[(taken(0), vec![4])])
            .expect("saves");
        assert_eq!(
            store.screenshots(&id).expect("reads"),
            [(taken(0), vec![4])]
        );
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_broken_draft_is_listed_and_a_half_saved_one_is_not() {
        let (store, root) = scratch("broken");
        let id = store
            .save(None, &draft(DraftKind::Fix, Some("health")), &[])
            .expect("saves");
        let folder = store.draft_folder(&id).expect("an ID");
        fs::write(folder.join(DRAFT_FILE), "{").expect("breaks it");
        fs::create_dir_all(store.folder.join("half-saved")).expect("folder");
        fs::write(
            store.folder.join("half-saved").join(shot_name(taken(0))),
            [1],
        )
        .expect("writes");
        fs::create_dir_all(store.folder.join("Not An ID")).expect("folder");
        fs::write(store.folder.join("Not An ID").join(DRAFT_FILE), "{}").expect("writes");
        let listed = store.list().expect("lists");
        let [broken] = listed.as_slice() else {
            panic!("one draft: {listed:?}");
        };
        assert_eq!(broken.id, id);
        assert_eq!(broken.draft, None);
        assert!(
            broken
                .problem
                .as_ref()
                .is_some_and(|problem| problem.starts_with("Its file is not valid")),
            "{broken:?}"
        );
        // Nothing says which pictures a draft that cannot be read names.
        let named = store
            .save(None, &draft(DraftKind::Automation, None), &[])
            .expect("saves");
        assert!(store.pictures().is_err());
        store.delete(&id).expect("deletes the broken one");
        assert_eq!(store.pictures().expect("lists"), ["click-1.png"]);
        store.delete(&named).expect("deletes");
        assert_eq!(store.pictures().expect("lists"), Vec::<String>::new());
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn drafts_of_something_gone_are_deleted_and_the_rest_stay() {
        let (store, root) = scratch("gone");
        let value = store
            .save(None, &draft(DraftKind::Value, None), &[(taken(0), vec![1])])
            .expect("saves");
        store
            .save(None, &draft(DraftKind::Fix, Some("health")), &[])
            .expect("saves");
        let broken = store
            .save(None, &draft(DraftKind::Fix, Some("health")), &[])
            .expect("saves");
        fs::write(
            store.draft_folder(&broken).expect("an ID").join(DRAFT_FILE),
            "{",
        )
        .expect("breaks it");
        store
            .delete_where(|draft| draft.kind == DraftKind::Fix)
            .expect("deletes");
        let left: Vec<String> = store
            .list()
            .expect("lists")
            .into_iter()
            .map(|summary| summary.id)
            .collect();
        assert_eq!(
            left,
            [value.clone(), broken],
            "one that cannot be read stays"
        );
        // A draft that cannot be removed says so.
        fs::write(store.folder.join(format!("{value}.1{SAVING}")), b"stray").expect("writes");
        assert!(store.delete_where(|_draft| true).is_err());
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_save_works_though_an_older_draft_of_the_item_cannot_be_removed() {
        let (store, root) = scratch("stuck");
        let changes = draft(DraftKind::Automation, Some("pause"));
        let older = store.save(None, &changes, &[]).expect("saves");
        // A file with a save's ending and the draft's name cannot be
        // removed as a folder, so deleting the older draft fails.
        fs::write(store.folder.join(format!("{older}.1{SAVING}")), b"stray").expect("writes");
        assert!(store.delete(&older).is_err());
        let newer = store
            .save(None, &changes, &[])
            .expect("the new one is saved");
        assert_ne!(newer, older);
        assert!(store.draft_folder(&newer).expect("an ID").is_dir());
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn a_folder_that_cannot_be_read_or_removed_says_why() {
        let (store, root) = scratch("unreadable");
        let id = store
            .save(None, &draft(DraftKind::Value, None), &[])
            .expect("saves");
        // A file where a draft's folder should be cannot be removed as one.
        fs::remove_dir_all(store.draft_folder(&id).expect("an ID")).expect("removes");
        fs::write(store.draft_folder(&id).expect("an ID"), b"not a folder").expect("writes");
        assert!(store.delete(&id).is_err());
        // A file where the drafts folder should be cannot be listed.
        fs::remove_dir_all(&store.folder).expect("removes");
        fs::write(&store.folder, b"not a folder").expect("writes");
        assert!(store.list().is_err());
        assert!(
            store
                .save(None, &draft(DraftKind::Value, None), &[])
                .is_err()
        );
        fs::remove_dir_all(root).expect("clean up");
    }

    #[test]
    fn an_id_that_is_not_a_folder_name_is_refused() {
        let (store, root) = scratch("ids");
        for id in [
            "",
            "../outside",
            "Upper",
            "a/b",
            &"a".repeat(MAX_ID_LEN + 1),
        ] {
            assert!(!is_draft_id(id), "{id:?}");
            assert_eq!(
                store.delete(id).expect_err("not an ID").kind(),
                io::ErrorKind::InvalidInput
            );
            assert!(
                store
                    .save(Some(id), &draft(DraftKind::Value, None), &[])
                    .is_err()
            );
        }
        assert!(is_draft_id(&"a".repeat(MAX_ID_LEN)));
        assert!(!root.join("outside").exists());
        assert!(!root.exists(), "nothing was written");
    }

    #[test]
    fn new_ids_skip_taken_ones_and_sort_by_time() {
        let first = new_id(&HashSet::new());
        let taken = HashSet::from([first.as_str()]);
        let second = new_id(&taken);
        assert_ne!(first, second);
        assert!(is_draft_id(&first) && is_draft_id(&second));
    }
}
