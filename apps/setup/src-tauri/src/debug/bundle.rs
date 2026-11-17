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

use std::io;

use appytappy_platform::LocalTime;
use serde::Deserialize;

use crate::debug::trace_file;
use crate::drafts::{DraftSummary, draft_shot};
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
    drafts: Vec<DraftSummary>,
}

impl BundleJob {
    /// A bundle of the files `stores` names, holding what `spec` asks for.
    pub(crate) const fn new(stores: Stores, spec: BundleSpec, drafts: Vec<DraftSummary>) -> Self {
        Self {
            stores,
            spec,
            drafts,
        }
    }

    /// Makes the bundle, as the bytes of a zip file.
    ///
    /// # Errors
    ///
    /// Returns why it could not be made, as the user reads it.
    pub(crate) fn make(self) -> Result<Vec<u8>, String> {
        build(&self.stores, self.spec, &self.drafts)
            .map_err(|error| format!("AppyTappy cannot make the debug bundle: {error}."))
    }
}

/// The bundle, as the bytes of a zip file.
///
/// # Errors
///
/// Returns the error from reading a file that is there, or from packing.
fn build(stores: &Stores, spec: BundleSpec, drafts: &[DraftSummary]) -> io::Result<Vec<u8>> {
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
    // Only pictures named by readable saved files are shared. A broken file
    // still goes in as text, but cannot authorize sharing other pictures nearby.
    if spec.pictures {
        let mut named = stores
            .automations
            .load()
            .map(|file| {
                file.images()
                    .into_iter()
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        named.extend(
            drafts
                .iter()
                .filter_map(|summary| summary.draft.as_ref())
                .flat_map(|draft| draft.pictures.iter().cloned()),
        );
        bundle.folder(
            "automation pictures",
            &stores.automations.pictures(),
            |file| named.iter().any(|name| name == file),
        )?;
    }
    for app in stores.apps.load()? {
        let folder = stores.apps.folder(&app.id);
        let named = app
            .file
            .as_ref()
            .map(|file| file.images())
            .unwrap_or_default();
        bundle.folder(&format!("apps/{}", app.id), &folder, |file| {
            file == "app.json" || (spec.pictures && named.contains(&file))
        })?;
    }
    // A draft's folder holds its file and, for a value, fix, or reference,
    // its screenshots; the click pictures it names are with the others.
    for draft in drafts {
        let folder = locations.drafts().join(&draft.id);
        bundle.folder(&format!("drafts/{}", draft.id), &folder, |file| {
            file == "draft.json"
                || (spec.pictures && draft.draft.is_some() && draft_shot(file).is_some())
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

#[cfg(test)]
mod tests {
    use std::{env, fs, process};

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
