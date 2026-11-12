//! The debug bundle: one zip file with what it takes to see what went
//! wrong, which the user saves where they choose and shares as they like.
//! Nothing is sent anywhere.
//!
//! It holds the files as they are on disk, so a file that cannot be read
//! shows why: the settings, the automations, each application's file, both
//! logs and their old files, and a note of the setup app's version. On
//! request it also holds the pictures, which can show the applications'
//! windows, and the newest trace file.

use std::fs;
use std::io::{self, Cursor, Write as _};
use std::path::Path;

use appytappy_platform::LocalTime;
use serde::Deserialize;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

use crate::debug::trace_file;
use crate::stores::Stores;

/// What goes in the bundle beyond the files it always holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct BundleSpec {
    /// The pictures the values, references and click actions learned from.
    pub(crate) pictures: bool,
    /// The newest trace file.
    pub(crate) trace: bool,
}

/// The four log files, as the runtime and the setup app write them.
const LOGS: [&str; 4] = [
    "runtime.log",
    "runtime.old.log",
    "setup.log",
    "setup.old.log",
];

/// The bundle, as the bytes of a zip file.
///
/// # Errors
///
/// Returns the error from reading a file that is there, or from packing.
pub(crate) fn build(stores: &Stores, spec: BundleSpec) -> io::Result<Vec<u8>> {
    let mut bundle = Bundle::new();
    bundle.text(
        "about.txt",
        &format!(
            "AppyTappy Setup {}\nMade {}\n",
            env!("CARGO_PKG_VERSION"),
            LocalTime::now()
                .map_or_else(|| "at an unknown time".to_owned(), |time| time.to_string())
        ),
    )?;
    let locations = &stores.locations;
    bundle.file_if_there("settings.json", &locations.settings())?;
    bundle.file_if_there("automations.json", &locations.automations())?;
    if spec.pictures {
        bundle.folder(
            "automation pictures",
            &stores.automations.pictures(),
            |_name| true,
        )?;
    }
    for app in folders(&locations.apps())? {
        let name = app
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        bundle.folder(&format!("apps/{name}"), &app, |file| {
            spec.pictures || file == "app.json"
        })?;
    }
    for log in LOGS {
        bundle.file_if_there(&format!("logs/{log}"), &stores.logs.join(log))?;
    }
    if spec.trace
        && let Some(trace) = trace_file::newest(&stores.traces)?
        && let Some(name) = trace.file_name()
    {
        let name = name.to_string_lossy();
        bundle.file_if_there(&format!("traces/{name}"), &trace)?;
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

/// A zip file being packed in memory.
struct Bundle {
    zip: ZipWriter<Cursor<Vec<u8>>>,
    /// When the entries were made, as zip records it.
    made: DateTime,
}

impl Bundle {
    fn new() -> Self {
        let made = LocalTime::now()
            .and_then(|time| {
                let small = |part: u16| u8::try_from(part).ok();
                DateTime::from_date_and_time(
                    time.year,
                    small(time.month)?,
                    small(time.day)?,
                    small(time.hour)?,
                    small(time.minute)?,
                    small(time.second)?,
                )
                .ok()
            })
            .unwrap_or_default();
        Self {
            zip: ZipWriter::new(Cursor::new(Vec::new())),
            made,
        }
    }

    /// Adds `bytes` as `name`. A picture is already compressed, so it is
    /// stored as it is.
    fn add(&mut self, name: &str, bytes: &[u8]) -> io::Result<()> {
        let picture = Path::new(name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("png"));
        let method = if picture {
            CompressionMethod::Stored
        } else {
            CompressionMethod::Deflated
        };
        let options = SimpleFileOptions::default()
            .compression_method(method)
            .last_modified_time(self.made);
        self.zip
            .start_file(name, options)
            .map_err(io::Error::other)?;
        self.zip.write_all(bytes)
    }

    fn text(&mut self, name: &str, text: &str) -> io::Result<()> {
        self.add(name, text.as_bytes())
    }

    /// Adds the file at `path` as `name`, when there is one.
    fn file_if_there(&mut self, name: &str, path: &Path) -> io::Result<()> {
        match fs::read(path) {
            Ok(bytes) => self.add(name, &bytes),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }

    /// Adds each file in `folder` whose name `wanted` takes, under `name`.
    fn folder(
        &mut self,
        name: &str,
        folder: &Path,
        wanted: impl Fn(&str) -> bool,
    ) -> io::Result<()> {
        let entries = match fs::read_dir(folder) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        let mut files = Vec::new();
        for entry in entries {
            let entry = entry?;
            let file = entry.file_name().to_string_lossy().into_owned();
            if entry.file_type()?.is_file() && wanted(&file) {
                files.push((file, entry.path()));
            }
        }
        files.sort();
        for (file, path) in files {
            self.file_if_there(&format!("{name}/{file}"), &path)?;
        }
        Ok(())
    }

    fn finish(self) -> io::Result<Vec<u8>> {
        self.zip
            .finish()
            .map(Cursor::into_inner)
            .map_err(io::Error::other)
    }
}
