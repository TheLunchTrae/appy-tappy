//! The debug bundle: one zip file with what it takes to see what went
//! wrong, which the user saves where they choose and shares as they like.
//! Nothing is sent anywhere.
//!
//! It holds the files as they are on disk, so a file that cannot be read
//! shows why: the settings, the automations, each application's file, each
//! draft's file, both logs and their old files, and a note of the setup
//! app's version. On request it also holds the pictures, which can show the
//! applications' windows, a draft's whole screenshots among them, and the
//! newest trace file.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::Path;

use appytappy_config::app_file::AppFile;
use appytappy_config::automation_file::AutomationsFile;
use appytappy_config::names::{is_app_id, is_image_name};
use appytappy_platform::LocalTime;
use serde::Deserialize;

use crate::debug::trace_file;
use crate::drafts::{DraftDef, draft_shot, is_draft_id};
use crate::stores::Stores;
use crate::zip_pack::ZipPack;

/// What goes in the bundle beyond the files it always holds.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct BundleSpec {
    /// The pictures the values, references and click actions learned from,
    /// and the screenshots the drafts keep.
    pub(crate) pictures: bool,
    /// The name of the trace file to add, the one the dialog showed, or
    /// `None` for no trace. Only a trace file in the trace folder is added.
    pub(crate) trace: Option<String>,
}

/// The four log files, as the runtime and the setup app write them.
const LOGS: [&str; 4] = [
    "runtime.log",
    "runtime.old.log",
    "setup.log",
    "setup.old.log",
];

/// A bundle to make away from the owner thread, since packing a large trace
/// takes a while and the page's other commands wait for that thread.
#[derive(Debug)]
pub(crate) struct BundleJob {
    stores: Stores,
    spec: BundleSpec,
}

impl BundleJob {
    /// A bundle of the files `stores` names, holding what `spec` asks for.
    pub(crate) const fn new(stores: Stores, spec: BundleSpec) -> Self {
        Self { stores, spec }
    }

    /// Makes the bundle, as the bytes of a zip file.
    ///
    /// # Errors
    ///
    /// Returns why it could not be made, as the user reads it.
    pub(crate) fn make(self) -> Result<Vec<u8>, String> {
        build(&self.stores, self.spec)
            .map_err(|error| format!("AppyTappy cannot make the debug bundle: {error}."))
    }
}

/// The bundle, as the bytes of a zip file.
///
/// # Errors
///
/// Returns the error from reading a file that is there, or from packing.
pub(crate) fn build(stores: &Stores, spec: BundleSpec) -> io::Result<Vec<u8>> {
    let mut bundle = ZipPack::new();
    bundle.add(
        "about.txt",
        format!(
            "AppyTappy Setup {}\nMade {}\n",
            env!("CARGO_PKG_VERSION"),
            LocalTime::now()
                .map_or_else(|| "at an unknown time".to_owned(), |time| time.to_string())
        )
        .as_bytes(),
    )?;
    let locations = &stores.locations;
    bundle.file_if_there("settings.json", &locations.settings())?;
    let mut clicks = HashSet::new();
    if let Some(bytes) = config(&mut bundle, "automations.json", &locations.automations())?
        && spec.pictures
        && let Some(file) = str::from_utf8(&bytes)
            .ok()
            .and_then(|json| AutomationsFile::parse(json).ok())
    {
        clicks.extend(file.images().into_iter().map(str::to_owned));
    }
    for (name, app) in folders(&locations.apps(), is_app_id)? {
        let prefix = format!("apps/{name}");
        if let Some(bytes) = config(
            &mut bundle,
            &format!("{prefix}/app.json"),
            &app.join("app.json"),
        )? && spec.pictures
            && let Some(file) = str::from_utf8(&bytes)
                .ok()
                .and_then(|json| AppFile::parse(json).ok())
        {
            let images: HashSet<_> = file.images().into_iter().collect();
            bundle.folder(&prefix, &app, |name| images.contains(name))?;
        }
    }
    for (name, draft) in folders(&locations.drafts(), is_draft_id)? {
        let prefix = format!("drafts/{name}");
        if let Some(bytes) = config(
            &mut bundle,
            &format!("{prefix}/draft.json"),
            &draft.join("draft.json"),
        )? && spec.pictures
            && let Ok(saved) = serde_json::from_slice::<DraftDef>(&bytes)
        {
            clicks.extend(saved.pictures);
            bundle.folder(&prefix, &draft, |name| draft_shot(name).is_some())?;
        }
    }
    if spec.pictures {
        bundle.folder(
            "automation pictures",
            &stores.automations.pictures(),
            |name| is_image_name(name) && clicks.contains(name),
        )?;
    }
    for log in LOGS {
        bundle.file_if_there(&format!("logs/{log}"), &stores.logs.join(log))?;
    }
    if let Some(name) = spec
        .trace
        .as_deref()
        .filter(|name| trace_file::is_trace_name(name))
    {
        bundle.file_if_there(&format!("traces/{name}"), &stores.traces.join(name))?;
    }
    bundle.finish()
}

/// Packs the raw file even when its contents cannot be parsed. Picture selection
/// reads these same bytes, so a concurrent save cannot change the list mid-export.
fn config(bundle: &mut ZipPack, name: &str, path: &Path) -> io::Result<Option<Vec<u8>>> {
    let bytes = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => fs::read(path),
        Ok(_) => return Ok(None),
        Err(error) => Err(error),
    };
    match bytes {
        Ok(bytes) => {
            bundle.add(name, &bytes)?;
            Ok(Some(bytes))
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// The saved item folders in `parent`, or none when it is not there.
fn folders(
    parent: &Path,
    valid: fn(&str) -> bool,
) -> io::Result<Vec<(String, std::path::PathBuf)>> {
    let entries = match fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir()
            && let Ok(name) = entry.file_name().into_string()
            && valid(&name)
        {
            found.push((name, entry.path()));
        }
    }
    found.sort();
    Ok(found)
}

#[cfg(test)]
mod tests {
    use std::{env, process};

    use super::*;

    #[test]
    fn only_a_missing_file_or_folder_is_left_out() {
        let scratch = env::temp_dir().join(format!("appytappy-bundle-reads-{}", process::id()));
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a folder left by an earlier run may or may not exist"
        )]
        let _ = fs::remove_dir_all(&scratch);
        fs::create_dir_all(&scratch).expect("folder");
        let missing = scratch.join("missing");
        // A file cannot be listed as a folder, nor a folder copied as a
        // file, and neither is simply missing.
        let file = scratch.join("file.txt");
        fs::write(&file, "x").expect("write");
        assert_eq!(
            folders(&missing, is_app_id).expect("missing is fine").len(),
            0
        );
        assert!(folders(&file, is_app_id).is_err());
        let mut bundle = ZipPack::new();
        bundle
            .file_if_there("a", &missing)
            .expect("missing is fine");
        assert!(bundle.file_if_there("b", &scratch).is_err());
        assert!(
            config(&mut bundle, "folder", &scratch)
                .expect("not a regular file")
                .is_none()
        );
        assert!(
            config(&mut bundle, "missing", &missing)
                .expect("missing")
                .is_none()
        );
        #[cfg(unix)]
        assert!(config(&mut bundle, "inside", &file.join("inside")).is_err());
        // A path under a file cannot be opened, and is not simply missing.
        // Windows reports such a path as missing, so this holds elsewhere.
        #[cfg(unix)]
        assert!(bundle.file_if_there("e", &file.join("inside")).is_err());
        bundle
            .folder("c", &missing, |_name| true)
            .expect("missing is fine");
        assert!(bundle.folder("d", &file, |_name| true).is_err());
        fs::remove_dir_all(&scratch).expect("clean up");
    }
}
