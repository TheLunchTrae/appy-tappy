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

use std::fs;
use std::io;
use std::path::Path;

use appytappy_platform::LocalTime;
use serde::Deserialize;

use crate::debug::trace_file;
use crate::stores::Stores;
use crate::zip_pack::{ZipPack, is_picture};

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
    bundle.file_if_there("automations.json", &locations.automations())?;
    if spec.pictures {
        bundle.folder(
            "automation pictures",
            &stores.automations.pictures(),
            is_picture,
        )?;
    }
    for app in folders(&locations.apps())? {
        let name = app
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        bundle.folder(&format!("apps/{name}"), &app, |file| {
            file == "app.json" || (spec.pictures && is_picture(file))
        })?;
    }
    // A draft's folder holds its file and, for a value, fix, or reference,
    // its screenshots; the click pictures it names are with the others.
    for draft in folders(&locations.drafts())? {
        let name = draft
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        bundle.folder(&format!("drafts/{name}"), &draft, |file| {
            file == "draft.json" || (spec.pictures && is_picture(file))
        })?;
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

/// The folders in `parent`, or none when it is not there.
fn folders(parent: &Path) -> io::Result<Vec<std::path::PathBuf>> {
    let entries = match fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            found.push(entry.path());
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
        assert_eq!(folders(&missing).expect("missing is fine").len(), 0);
        assert!(folders(&file).is_err());
        let mut bundle = ZipPack::new();
        bundle
            .file_if_there("a", &missing)
            .expect("missing is fine");
        assert!(bundle.file_if_there("b", &scratch).is_err());
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
