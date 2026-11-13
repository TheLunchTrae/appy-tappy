//! Packing a zip file in memory: the debug bundle and an exported
//! application. Text is compressed with deflate, the method every zip tool
//! reads, and a picture is stored as it is.

use std::fs::{self, File};
use std::io::{self, Cursor, Write as _};
use std::path::Path;

use appytappy_platform::LocalTime;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

/// Whether `name` names a picture, by its `.png` extension.
pub(crate) fn is_picture(name: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
}

/// A zip file being packed in memory.
#[derive(Debug)]
pub(crate) struct ZipPack {
    zip: ZipWriter<Cursor<Vec<u8>>>,
    /// When the entries were made, as zip records it.
    made: DateTime,
}

impl ZipPack {
    /// An empty zip file, whose entries are dated now.
    pub(crate) fn new() -> Self {
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

    /// Adds `bytes` as `name`.
    ///
    /// # Errors
    ///
    /// Returns the error from packing.
    pub(crate) fn add(&mut self, name: &str, bytes: &[u8]) -> io::Result<()> {
        self.start(name)?;
        self.zip.write_all(bytes)
    }

    /// Starts the entry `name`. A picture is already compressed, so it is
    /// stored as it is.
    fn start(&mut self, name: &str) -> io::Result<()> {
        let method = if is_picture(name) {
            CompressionMethod::Stored
        } else {
            CompressionMethod::Deflated
        };
        let options = SimpleFileOptions::default()
            .compression_method(method)
            .last_modified_time(self.made);
        self.zip.start_file(name, options).map_err(io::Error::other)
    }

    /// Adds the file at `path` as `name`, when there is one. It is copied
    /// into the zip a piece at a time, so only its packed bytes are held.
    ///
    /// # Errors
    ///
    /// Returns the error from reading a file that is there, or from packing.
    pub(crate) fn file_if_there(&mut self, name: &str, path: &Path) -> io::Result<()> {
        let mut file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        self.start(name)?;
        io::copy(&mut file, &mut self.zip).map(drop)
    }

    /// Adds each file in `folder` whose name `wanted` takes, under `name`.
    /// A folder that is not there adds nothing.
    ///
    /// # Errors
    ///
    /// Returns the error from listing the folder, reading a file, or packing.
    pub(crate) fn folder(
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

    /// The zip file's bytes.
    ///
    /// # Errors
    ///
    /// Returns the error from packing.
    pub(crate) fn finish(self) -> io::Result<Vec<u8>> {
        self.zip
            .finish()
            .map(Cursor::into_inner)
            .map_err(io::Error::other)
    }
}
