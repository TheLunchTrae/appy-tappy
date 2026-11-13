//! Sharing an application: one zip file that holds its `app.json` and the
//! pictures that file names, which another user imports.
//!
//! The file is the application's folder, packed: `app.json` and each
//! picture, with nothing else. It keeps the build the values were checked
//! against and how the definition was made, and leaves out the overlay and
//! the name of a file it was imported from, which belong to the machine
//! they were set on. No automation is in it, so nothing an imported file
//! holds runs until the user makes an automation of their own.
//!
//! An imported file is a trust boundary. [`Shared::unpack`] takes only what
//! `app.json` names, reads no more than [`MAX_SHARED_BYTES`], decodes no
//! picture until every picture's header says that together they hold no
//! more pixels than [`LARGEST_PICTURE`], decodes each picture once, and
//! holds the file to the rules a saved file is held to: [`AppFile::parse`],
//! then `saved::build` for each value with one `saved::Readers` for the
//! file's references, as the runtime reads them. Each picture takes the
//! name the setup app gives it, whatever the file calls it, so no two
//! values or references share a picture and a later save in the setup app
//! replaces only its own.
//! [`pack`] holds a saved application to the same rules as it exports it:
//! it stops at the first picture that takes the zip file past
//! [`MAX_SHARED_BYTES`], and reads back what it packed, so a file AppyTappy
//! exports is one it imports.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::io::{self, Cursor, Read as _};

use appytappy_config::app_file::{AppFile, AppFileError, CheckDef, OriginDef};
use appytappy_config::names::{sample_name, state_image_name};
use appytappy_config::store::APP_FILE;
use appytappy_sources::screen::saved::{self, SavedError};
use appytappy_sources::screen::{Image, LARGEST_PICTURE};
use zip::ZipArchive;
use zip::result::ZipError;

use crate::check_spec::Pictures;
use crate::stores::SavedApp;
use crate::zip_pack::ZipPack;

/// The largest file an import reads, and the most its `app.json` and
/// pictures may unpack to, in bytes. A definition is far smaller: its
/// pictures are crops of a window.
pub(crate) const MAX_SHARED_BYTES: usize = 16 * 1024 * 1024;

/// An application as it is shared: its file, without an overlay, and every
/// picture the file names, each of which the runtime can read and each
/// named as a saved file names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Shared {
    file: AppFile,
    /// The pictures by file name, so they pack in one order.
    pictures: BTreeMap<String, Vec<u8>>,
}

/// The bytes of the zip file that shares the saved application `app`,
/// which [`Shared::unpack`] reads back.
///
/// # Errors
///
/// Returns a [`ShareError`] when a picture its file names is missing or
/// cannot be read, when packing fails, or when [`Shared::unpack`] would
/// refuse the file, such as one that holds more than [`MAX_SHARED_BYTES`].
pub(crate) fn pack(app: &SavedApp) -> Result<Vec<u8>, ShareError> {
    pack_with(app.file.clone(), |name| {
        app.picture(name).map_err(|source| {
            if source.kind() == io::ErrorKind::NotFound {
                ShareError::NoPicture(name.to_owned())
            } else {
                ShareError::Read(source)
            }
        })
    })
}

/// The bytes of the zip file that shares `file` with the pictures `picture`
/// answers, packed as they are read. A file past [`MAX_SHARED_BYTES`] is
/// refused at the picture that takes it there, so no picture after it is
/// read.
fn pack_with(
    mut file: AppFile,
    picture: impl FnMut(&str) -> Result<Vec<u8>, ShareError>,
) -> Result<Vec<u8>, ShareError> {
    let mut zip = ZipPack::new();
    let mut budget = MAX_SHARED_BYTES;
    let mut add = |name: &str, bytes: &[u8]| {
        budget = budget
            .checked_sub(bytes.len())
            .ok_or(ShareError::TooLarge)?;
        zip.add(name, bytes).map_err(ShareError::Pack)
    };
    share(&mut file, picture, |name, bytes| add(&name, &bytes))?;
    add(APP_FILE, file.to_json().as_bytes())?;
    let packed = zip.finish().map_err(ShareError::Pack)?;
    Shared::unpack(&packed).map(|_read| packed)
}

impl Shared {
    /// Reads a shared file's bytes.
    ///
    /// # Errors
    ///
    /// Returns a [`ShareError`] when the bytes are not a zip file, hold too
    /// much, hold no `app.json` or one that breaks a rule of
    /// [`AppFile::parse`], lack a picture it names, or hold a value or a
    /// reference the runtime could not read.
    pub(crate) fn unpack(bytes: &[u8]) -> Result<Self, ShareError> {
        if bytes.len() > MAX_SHARED_BYTES {
            return Err(ShareError::TooLarge);
        }
        let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(ShareError::Zip)?;
        // Which of two entries with one name is read is the reader's
        // choice, and another tool may show the user the other one.
        if archive.has_duplicate_names() {
            return Err(ShareError::SameName);
        }
        let mut budget = MAX_SHARED_BYTES;
        let json = entry(&mut archive, APP_FILE, &mut budget)?.ok_or(ShareError::NoAppFile)?;
        let json = String::from_utf8(json)
            .map_err(|error| ShareError::File(AppFileError::Json(error.to_string())))?;
        let file = AppFile::parse(&json).map_err(ShareError::File)?;
        Self::new(file, |name| {
            entry(&mut archive, name, &mut budget)?
                .ok_or_else(|| ShareError::NoPicture(name.to_owned()))
        })
    }

    /// `file`, which passed [`AppFile::check`], as it is shared, with the
    /// pictures `picture` answers for the names in it, as [`share`] takes
    /// them, once every value and reference reads from them as the runtime
    /// would read it.
    ///
    /// Reading the values decodes each picture once: each has one place
    /// that names it, and [`read_all`] builds each reference's reader once.
    fn new(
        mut file: AppFile,
        picture: impl FnMut(&str) -> Result<Vec<u8>, ShareError>,
    ) -> Result<Self, ShareError> {
        let mut pictures = BTreeMap::new();
        share(&mut file, picture, |name, bytes| {
            pictures.insert(name, bytes);
            Ok(())
        })?;
        read_all(&file, |name| {
            pictures
                .get(name)
                .cloned()
                .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
        })?;
        Ok(Self { file, pictures })
    }

    /// The application's file.
    pub(crate) const fn file(&self) -> &AppFile {
        &self.file
    }

    /// The application's file and its pictures, to save.
    pub(crate) fn into_parts(self) -> (AppFile, Pictures) {
        (self.file, self.pictures.into_iter().collect())
    }
}

/// Readies `file`, which passed [`AppFile::check`], to share: leaves out
/// the overlay and the names of the files its values and references were
/// imported from, and hands `keep` each picture `picture` answers for the
/// names in it.
///
/// Each picture is asked for once for each place that names it and takes
/// that place's own name: a sample's as [`sample_name`] numbers them, a
/// state's as [`state_image_name`] does, with a fix's pictures in the set
/// of its place among the fixes. A file made elsewhere can name a picture
/// anything a folder can hold, such as another value's next picture, which
/// a later save would overwrite.
///
/// Each picture's size is read from its header as it is taken, and none is
/// kept once all of them together hold more pixels than
/// [`LARGEST_PICTURE`]: a picture's pixels cost many times the bytes of a
/// file that compresses them, so the file's own limit bounds none.
fn share(
    file: &mut AppFile,
    mut picture: impl FnMut(&str) -> Result<Vec<u8>, ShareError>,
    mut keep: impl FnMut(String, Vec<u8>) -> Result<(), ShareError>,
) -> Result<(), ShareError> {
    file.overlay = None;
    // A shared file says that its maker imported a value or a
    // reference, and not from which file of theirs.
    for origin in file.origins_mut() {
        if let OriginDef::Imported { file: from } = origin {
            from.clear();
        }
    }
    let mut held = 0_u64;
    let mut take = |image: &mut String,
                    name: String,
                    owner: &dyn Fn(SavedError) -> ShareError|
     -> Result<(), ShareError> {
        let bytes = picture(image)?;
        let size = Image::png_size(&bytes).map_err(|source| {
            owner(SavedError::Picture {
                name: image.clone(),
                source,
            })
        })?;
        held = held.saturating_add(size.pixels());
        if held > LARGEST_PICTURE.pixels() {
            return Err(ShareError::TooManyPixels);
        }
        keep(name.clone(), bytes)?;
        *image = name;
        Ok(())
    };
    for reference in &mut file.references {
        let owner = |source| ShareError::Reference {
            name: reference.name.clone(),
            source,
        };
        for (sample, number) in reference.samples.iter_mut().zip(1..) {
            take(
                &mut sample.image,
                sample_name(&reference.id, number),
                &owner,
            )?;
        }
    }
    for value in &mut file.values {
        let owner = |source| ShareError::Value {
            name: value.name.clone(),
            source,
        };
        if let CheckDef::State { states, .. } = &mut value.screen.check {
            for (state, number) in states.iter_mut().zip(1..) {
                take(
                    &mut state.image,
                    state_image_name(&value.id, None, number),
                    &owner,
                )?;
            }
        }
        for (fix, set) in value.screen.fixes.iter_mut().zip(1..) {
            for (image, number) in fix.images.iter_mut().zip(1..) {
                take(
                    image,
                    state_image_name(&value.id, Some(set), number),
                    &owner,
                )?;
            }
        }
    }
    Ok(())
}

/// Reads each value of `file` and each of its references, as the runtime
/// does, from the pictures `saved` answers for their names.
fn read_all(
    file: &AppFile,
    mut saved: impl FnMut(&str) -> io::Result<Vec<u8>>,
) -> Result<(), ShareError> {
    let mut readers = saved::Readers::new(&file.references);
    for value in &file.values {
        saved::build(&value.screen, &mut readers, file.size_changes, &mut saved).map_err(
            |source| ShareError::Value {
                name: value.name.clone(),
                source,
            },
        )?;
    }
    // A reference no value reads with is shared too, so it has to learn.
    for reference in &file.references {
        readers
            .reader(&reference.id, &mut saved)
            .map_err(|source| ShareError::Reference {
                name: reference.name.clone(),
                source,
            })?;
    }
    Ok(())
}

/// The bytes of the entry `name` of `archive`, or `None` when it has none.
/// `budget` is how many bytes the entries may still unpack to, which the
/// entry's own length is taken from: what an entry says its size is cannot
/// be trusted, so it is read only that far.
fn entry(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    name: &str,
    budget: &mut usize,
) -> Result<Option<Vec<u8>>, ShareError> {
    let file = match archive.by_name(name) {
        Ok(file) => file,
        Err(ZipError::FileNotFound) => return Ok(None),
        Err(error) => return Err(ShareError::Zip(error)),
    };
    // One byte past the budget tells an entry that fits from one that does
    // not.
    let limit = u64::try_from(*budget).map_or(u64::MAX, |budget| budget.saturating_add(1));
    let mut bytes = Vec::new();
    file.take(limit)
        .read_to_end(&mut bytes)
        .map_err(ShareError::Read)?;
    *budget = budget
        .checked_sub(bytes.len())
        .ok_or(ShareError::TooLarge)?;
    Ok(Some(bytes))
}

/// An application cannot be shared, or a shared file cannot be used. Each
/// reads as the end of a sentence about the file, such as "it holds no
/// app.json".
#[derive(Debug)]
pub(crate) enum ShareError {
    /// The bytes are not a zip file, or not one this build can read.
    Zip(ZipError),
    /// The file, or what it unpacks to, is larger than
    /// [`MAX_SHARED_BYTES`].
    TooLarge,
    /// The zip file holds two entries with one name.
    SameName,
    /// The zip file holds no `app.json`.
    NoAppFile,
    /// `app.json` cannot be used, such as one of another format.
    File(AppFileError),
    /// A picture `app.json` names is not with it.
    NoPicture(String),
    /// The pictures together hold more pixels than [`LARGEST_PICTURE`].
    TooManyPixels,
    /// A picture or an entry cannot be read.
    Read(io::Error),
    /// The zip file cannot be made.
    Pack(io::Error),
    /// The value named `name` cannot be read as saved.
    Value {
        /// The value's name.
        name: String,
        /// Why.
        source: SavedError,
    },
    /// The reference named `name` cannot learn its pictures.
    Reference {
        /// The reference's name.
        name: String,
        /// Why.
        source: SavedError,
    },
}

impl fmt::Display for ShareError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // An entry packed another way than deflate is found when it is
            // read, so it comes as either error.
            Self::Zip(
                ZipError::UnsupportedArchive(_) | ZipError::CompressionMethodNotSupported(_),
            ) => formatter.write_str("it uses a compression AppyTappy can't read"),
            Self::Read(error) if error.kind() == io::ErrorKind::Unsupported => {
                formatter.write_str("it uses a compression AppyTappy can't read")
            },
            Self::Zip(_) => formatter.write_str("it is not a zip file AppyTappy can read"),
            Self::TooLarge => write!(
                formatter,
                "it holds more than {} MB",
                MAX_SHARED_BYTES / (1024 * 1024)
            ),
            Self::SameName => formatter.write_str("it holds two files with the same name"),
            Self::NoAppFile => write!(formatter, "it holds no {APP_FILE}"),
            Self::File(error) => write!(formatter, "{error}"),
            Self::NoPicture(name) => write!(formatter, "the picture {name} is missing"),
            Self::TooManyPixels => write!(
                formatter,
                "its pictures together hold more pixels than one {LARGEST_PICTURE} picture"
            ),
            Self::Read(error) => write!(formatter, "it cannot be read ({error})"),
            Self::Pack(error) => write!(formatter, "its file cannot be made ({error})"),
            Self::Value { name, source } => {
                write!(formatter, "the value {name} cannot be read: {source}")
            },
            Self::Reference { name, source } => {
                write!(formatter, "the reference {name} cannot be read: {source}")
            },
        }
    }
}

impl Error for ShareError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Zip(error) => Some(error),
            Self::File(error) => Some(error),
            Self::Read(error) | Self::Pack(error) => Some(error),
            Self::Value { source, .. } | Self::Reference { source, .. } => Some(source),
            Self::TooLarge
            | Self::SameName
            | Self::NoAppFile
            | Self::NoPicture(_)
            | Self::TooManyPixels => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use appytappy_config::app_file::{
        AntiCheatDef, BuildDef, CheckedDef, ColorDef, FORMAT, FixDef, OverlayDef, OverlayModeDef,
        ReferenceDef, RegionDef, SampleDef, ScreenDef, SizeChangesDef, SizeDef, StateDef, ValueDef,
        WindowDef,
    };
    use appytappy_config::window_match::WindowFacts;
    use appytappy_sources::screen::{Image, Size};
    use proptest::prelude::*;
    use std::io::Write as _;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    use super::*;

    /// An application named `name` with no values.
    fn app(name: &str) -> AppFile {
        AppFile {
            format: FORMAT,
            name: name.to_owned(),
            window: WindowDef::exact(WindowFacts {
                title: "Example App",
                class: "ExampleWindow",
                executable: "Example.exe",
            }),
            size_changes: SizeChangesDef::Often,
            anti_cheat: AntiCheatDef::None,
            overlay: None,
            checked: None,
            values: Vec::new(),
            references: Vec::new(),
        }
    }

    /// A PNG of `width` by `height` pixels, each the pixel `bgra`.
    fn png(width: u32, height: u32, bgra: [u8; 4]) -> Vec<u8> {
        let pixels = (0..height)
            .flat_map(|_row| 0..width)
            .flat_map(|_column| bgra)
            .collect();
        Image::new(Size { width, height }, pixels)
            .expect("matching length")
            .encode_png()
            .expect("encodes")
    }

    /// A state value with the ID `id` that covers a `width` by `height`
    /// window, and the name of its one state's picture.
    fn state_value(id: &str, width: u32, height: u32) -> (ValueDef, String) {
        let image = state_image_name(id, None, 1);
        let value = ValueDef {
            id: id.to_owned(),
            name: format!("Value {id}"),
            origin: OriginDef::ByHand,
            screen: ScreenDef {
                window_size: SizeDef { width, height },
                region: RegionDef {
                    x: 0,
                    y: 0,
                    width,
                    height,
                },
                check: CheckDef::State {
                    states: vec![StateDef {
                        name: "Shown".to_owned(),
                        image: image.clone(),
                    }],
                    tolerance: 4,
                },
                pin: None,
                reading_area: false,
                fixes: Vec::new(),
            },
        };
        (value, image)
    }

    /// The reference `digits`, which reads red text, with `samples` samples
    /// that each show `11`.
    fn digits(samples: usize) -> ReferenceDef {
        ReferenceDef {
            id: "digits".to_owned(),
            name: "Digits".to_owned(),
            origin: OriginDef::ByHand,
            ink: ColorDef {
                red: 255,
                green: 0,
                blue: 0,
            },
            tolerance: 0,
            samples: (1..=samples)
                .map(|number| SampleDef {
                    image: sample_name("digits", number),
                    text: "11".to_owned(),
                })
                .collect(),
            expects: None,
        }
    }

    /// A picture of `11`: two red columns on gray, 4 by 2 pixels drawn
    /// `scale` times larger.
    fn eleven(scale: u32) -> Vec<u8> {
        let wide = usize::try_from(scale).expect("small");
        let row = [[0, 0, 255, 255], [40, 40, 40, 255]]
            .repeat(2)
            .iter()
            .flat_map(|pixel| pixel.repeat(wide))
            .collect::<Vec<u8>>();
        let size = Size {
            width: scale.saturating_mul(4),
            height: scale.saturating_mul(2),
        };
        Image::new(size, row.repeat(wide.saturating_mul(2)))
            .expect("matching length")
            .encode_png()
            .expect("encodes")
    }

    /// `Example App` with the state value `status`, and its picture.
    fn with_status() -> (AppFile, String, Vec<u8>) {
        let mut file = app("Example App");
        let (value, image) = state_value("status", 2, 2);
        file.values.push(value);
        (file, image, png(2, 2, [0, 0, 255, 255]))
    }

    /// A zip file of `entries`, as anyone could make one, each stored as it
    /// is or deflated.
    fn zip_with(method: CompressionMethod, entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(method);
        for (name, bytes) in entries {
            zip.start_file(*name, options).expect("starts");
            zip.write_all(bytes).expect("packs");
        }
        zip.finish().expect("packs").into_inner()
    }

    fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        zip_with(CompressionMethod::Deflated, entries)
    }

    /// `picture` with `more` bytes after its end, which a PNG reader leaves
    /// unread and deflate packs to almost nothing.
    fn padded(picture: &[u8], more: usize) -> Vec<u8> {
        [picture, &vec![0; more]].concat()
    }

    /// What reading `bytes` says, as the user reads it.
    fn refusal(bytes: &[u8]) -> String {
        Shared::unpack(bytes)
            .expect_err("the file is refused")
            .to_string()
    }

    #[test]
    fn a_file_is_refused_for_what_it_lacks_or_cannot_be_read_as() {
        let (file, image, picture) = with_status();
        let json = file.to_json();
        assert_eq!(
            refusal(b"not a zip file"),
            "it is not a zip file AppyTappy can read"
        );
        assert_eq!(
            refusal(&zip_of(&[(&image, &picture)])),
            "it holds no app.json"
        );
        assert_eq!(
            refusal(&zip_of(&[(APP_FILE, &[0xff, 0xfe])])),
            "the file is not valid: invalid utf-8 sequence of 1 bytes from index 0"
        );
        let newer = json.replace("\"format\": 1", "\"format\": 2");
        assert_eq!(
            refusal(&zip_of(&[(APP_FILE, newer.as_bytes()), (&image, &picture)])),
            "the file has format 2, but this version of AppyTappy reads format 1"
        );
        let unnamed = json.replace("\"name\": \"Example App\"", "\"name\": \" \"");
        assert_eq!(
            refusal(&zip_of(&[
                (APP_FILE, unnamed.as_bytes()),
                (&image, &picture)
            ])),
            "the application has no name",
            "a rule a saved file is held to"
        );
        assert_eq!(
            refusal(&zip_of(&[(APP_FILE, json.as_bytes())])),
            "the picture status-1.png is missing"
        );
        // Refused from its header, or once its pixels are found missing,
        // each in the words a saved file's would be.
        for (bytes, read) in [(&b"text"[..], "no header"), (&header(2, 2), "no pixels")] {
            let unread = Shared::unpack(&zip_of(&[(APP_FILE, json.as_bytes()), (&image, bytes)]))
                .expect_err(read);
            assert_eq!(
                unread.to_string(),
                "the value Value status cannot be read: its picture status-1.png is not a PNG \
                 file it can read",
                "{read}"
            );
            assert!(
                unread.source().is_some_and(<dyn Error>::is::<SavedError>),
                "the cause is kept"
            );
        }
        let mut taught = app("Example App");
        taught.references.push(digits(1));
        for bytes in [&b"text"[..], &header(2, 2)] {
            let unread = Shared::unpack(&zip_of(&[
                (APP_FILE, taught.to_json().as_bytes()),
                ("ref-digits-1.png", bytes),
            ]))
            .expect_err("a reference no value reads with is read too");
            assert_eq!(
                unread.to_string(),
                "the reference Digits cannot be read: its picture ref-digits-1.png is not a PNG \
                 file it can read"
            );
            assert!(
                unread.source().is_some_and(<dyn Error>::is::<SavedError>),
                "the cause is kept"
            );
        }
        let other_size = png(3, 2, [0, 0, 255, 255]);
        assert!(
            refusal(&zip_of(&[
                (APP_FILE, json.as_bytes()),
                (&image, &other_size)
            ]))
            .starts_with("the value Value status cannot be read: "),
            "a picture of another size than its region"
        );
    }

    #[test]
    fn a_file_is_read_no_further_than_the_most_it_may_hold() {
        let (file, image, picture) = with_status();
        let json = file.to_json();
        let over = vec![b' '; MAX_SHARED_BYTES.checked_add(1).expect("fits")];
        assert_eq!(refusal(&over), "it holds more than 16 MB");
        // Spaces pack to almost nothing, so the zip file itself is small.
        let spaced = [json.as_bytes(), &over].concat();
        let bomb = zip_of(&[(APP_FILE, &spaced), (&image, &picture)]);
        assert!(bomb.len() < MAX_SHARED_BYTES / 16);
        assert_eq!(refusal(&bomb), "it holds more than 16 MB");
        // The budget is one for the whole file: each of two pictures fits
        // alone, in a zip file that is itself small, and both do not.
        let mut file = file;
        let (second, second_image) = state_value("other", 2, 2);
        file.values.push(second);
        let json = file.to_json();
        let large = padded(&picture, MAX_SHARED_BYTES / 16 * 9);
        let both = zip_of(&[
            (APP_FILE, json.as_bytes()),
            (&image, &large),
            (&second_image, &large),
        ]);
        assert!(both.len() < MAX_SHARED_BYTES / 16);
        assert_eq!(refusal(&both), "it holds more than 16 MB");
        let one = zip_of(&[
            (APP_FILE, json.as_bytes()),
            (&image, &large),
            (&second_image, &picture),
        ]);
        Shared::unpack(&one).expect("one such picture fits");
    }

    /// A PNG file whose header says its picture is `width` by `height`
    /// pixels, of 8-bit gray, with no pixels after it.
    fn header(width: u32, height: u32) -> Vec<u8> {
        let chunk = |kind: &[u8; 4], data: &[u8]| {
            let mut crc = flate2::Crc::new();
            crc.update(kind);
            crc.update(data);
            let length = u32::try_from(data.len()).expect("short");
            [
                &length.to_be_bytes()[..],
                kind,
                data,
                &crc.sum().to_be_bytes(),
            ]
            .concat()
        };
        let size = [width.to_be_bytes(), height.to_be_bytes()].concat();
        [
            &b"\x89PNG\r\n\x1a\n"[..],
            &chunk(b"IHDR", &[&size[..], &[8, 0, 0, 0, 0]].concat()),
            &chunk(b"IEND", &[]),
        ]
        .concat()
    }

    /// What importing state values with pictures of `sizes` says, whose
    /// headers alone give their sizes.
    fn refusal_of_sizes(sizes: &[(u32, u32)]) -> String {
        let mut file = app("Example App");
        let mut pictures = Vec::new();
        for (number, &(width, height)) in sizes.iter().enumerate() {
            let (value, image) = state_value(&format!("value_{number}"), width, height);
            file.values.push(value);
            pictures.push((image, header(width, height)));
        }
        let json = file.to_json();
        let entries: Vec<(&str, &[u8])> = std::iter::once((APP_FILE, json.as_bytes()))
            .chain(
                pictures
                    .iter()
                    .map(|(name, bytes)| (name.as_str(), bytes.as_slice())),
            )
            .collect();
        refusal(&zip_of(&entries))
    }

    const TOO_MANY_PIXELS: &str =
        "its pictures together hold more pixels than one 7680x4320 picture";

    #[test]
    fn no_picture_is_decoded_past_the_pixels_one_picture_may_hold() {
        // Two halves of the largest picture are read, and found to have no
        // pixels after their headers.
        assert_eq!(
            refusal_of_sizes(&[(7680, 2160), (7680, 2160)]),
            "the value Value value_0 cannot be read: its picture value_0-1.png is not a PNG \
             file it can read"
        );
        // One row more is refused before either is decoded.
        assert_eq!(
            refusal_of_sizes(&[(7680, 2160), (7680, 2161)]),
            TOO_MANY_PIXELS
        );
        // Each picture is held to what AppyTappy reads, whatever the total.
        let wide = Shared::unpack(&{
            let (mut file, image, _picture) = with_status();
            file.values[0].screen.window_size.width = 8193;
            file.values[0].screen.region.width = 8193;
            zip_of(&[
                (APP_FILE, file.to_json().as_bytes()),
                (&image, &header(8193, 2)),
            ])
        })
        .expect_err("wider than AppyTappy reads");
        assert_eq!(
            wide.to_string(),
            "the value Value status cannot be read: its picture status-1.png is 8193x2, larger \
             than AppyTappy reads"
        );
        assert!(wide.source().is_some(), "the cause is kept");
    }

    /// A number value with the ID `id` that reads an 8 by 4 region with the
    /// reference `digits`.
    fn number_value(id: &str) -> ValueDef {
        let (mut value, _image) = state_value(id, 8, 4);
        value.screen.check = CheckDef::Number {
            reference: "digits".to_owned(),
            whole: false,
            characters: None,
        };
        value
    }

    #[test]
    fn checking_a_file_reads_each_reference_s_pictures_once() {
        // One reference with one sample, which three values read with.
        let mut file = app("Example App");
        file.references.push(digits(1));
        file.values = ["first", "second", "third"].map(number_value).to_vec();
        let picture = eleven(2);
        let mut asked = Vec::new();
        read_all(&file, |name| {
            asked.push(name.to_owned());
            Ok(picture.clone())
        })
        .expect("each value reads");
        assert_eq!(asked, [file.references[0].samples[0].image.clone()]);
    }

    proptest! {
        #[test]
        fn an_import_is_refused_when_its_pictures_hold_more_pixels_than_one_may(
            sizes in prop::collection::vec((1_u32..=7680, 1_u32..=4320), 1..4),
        ) {
            let pixels = sizes.iter().try_fold(0_u64, |sum, &(width, height)| {
                u64::from(width)
                    .checked_mul(u64::from(height))
                    .and_then(|pixels| sum.checked_add(pixels))
            });
            let over = pixels.expect("fits") > LARGEST_PICTURE.pixels();
            prop_assert_eq!(refusal_of_sizes(&sizes) == TOO_MANY_PIXELS, over);
        }
    }

    #[test]
    fn an_application_that_could_not_be_imported_is_not_packed() {
        let (file, image, picture) = with_status();
        let large = padded(&picture, MAX_SHARED_BYTES);
        let held = |name: &str| {
            assert_eq!(name, image);
            Ok(large.clone())
        };
        Shared::new(file.clone(), held).expect("each value reads");
        assert_eq!(
            pack_with(file, held)
                .expect_err("too large to import")
                .to_string(),
            "it holds more than 16 MB"
        );
    }

    #[test]
    fn no_picture_is_read_past_the_most_an_export_holds() {
        let mut file = app("Example App");
        let mut pictures = BTreeMap::new();
        for (id, more) in [
            ("first", MAX_SHARED_BYTES / 2),
            ("second", MAX_SHARED_BYTES / 2),
            ("third", 0),
        ] {
            let (value, image) = state_value(id, 2, 2);
            file.values.push(value);
            pictures.insert(image, padded(&png(2, 2, [0, 0, 255, 255]), more));
        }
        let mut asked = Vec::new();
        let refused = pack_with(file, |name| {
            asked.push(name.to_owned());
            Ok(pictures[name].clone())
        })
        .expect_err("too large to export");
        assert_eq!(refused.to_string(), "it holds more than 16 MB");
        assert_eq!(
            asked,
            [
                state_image_name("first", None, 1),
                state_image_name("second", None, 1)
            ],
            "the third picture is never read"
        );
    }

    #[test]
    fn a_zip_file_packed_another_way_says_so() {
        let (file, _image, _picture) = with_status();
        let mut bytes = zip_with(
            CompressionMethod::Stored,
            &[(APP_FILE, file.to_json().as_bytes())],
        );
        // The method is the two bytes 8 past an entry's signature and 10
        // past its signature in the list of entries; 12 is bzip2.
        for (signature, at) in [(b"PK\x03\x04", 8_usize), (b"PK\x01\x02", 10)] {
            let found = bytes
                .windows(4)
                .rposition(|window| window == signature)
                .expect("one entry");
            bytes[found.checked_add(at).expect("small")] = 12;
        }
        assert_eq!(
            refusal(&bytes),
            "it uses a compression AppyTappy can't read"
        );
    }

    #[test]
    fn a_zip_file_with_two_entries_of_one_name_is_refused() {
        let (file, image, picture) = with_status();
        // No zip writer packs two entries with one name, so the second
        // one's name is changed after: in the entry and in the list.
        let other = image.replace("status", "statuz");
        let mut bytes = zip_of(&[
            (APP_FILE, file.to_json().as_bytes()),
            (&image, &picture),
            (&other, b"another picture"),
        ]);
        Shared::unpack(&bytes).expect("reads while the names differ");
        let mut renamed = 0_u8;
        for at in 0..bytes.len() {
            if bytes[at..].starts_with(other.as_bytes()) {
                bytes[at..][..image.len()].copy_from_slice(image.as_bytes());
                renamed = renamed.checked_add(1).expect("two names");
            }
        }
        assert_eq!(renamed, 2);
        assert_eq!(refusal(&bytes), "it holds two files with the same name");
    }

    #[test]
    fn a_file_of_exactly_the_most_an_import_reads_is_read() {
        let (file, image, picture) = with_status();
        let json = file.to_json();
        // An entry the file does not name is not read, so it only makes
        // the zip file larger: stored, by a byte for each of its own.
        let sized = |padding: usize| {
            zip_with(
                CompressionMethod::Stored,
                &[
                    (APP_FILE, json.as_bytes()),
                    (&image, &picture),
                    ("padding", &vec![0; padding]),
                ],
            )
        };
        let rest = MAX_SHARED_BYTES
            .checked_sub(sized(0).len())
            .expect("a small file");
        let most = sized(rest);
        assert_eq!(most.len(), MAX_SHARED_BYTES);
        Shared::unpack(&most).expect("the most a file may be");
        let over = sized(rest.checked_add(1).expect("fits"));
        assert_eq!(refusal(&over), "it holds more than 16 MB");
    }

    #[test]
    fn an_entry_that_cannot_be_read_says_why() {
        let unread = ShareError::Read(io::Error::other("the disk failed"));
        assert_eq!(unread.to_string(), "it cannot be read (the disk failed)");
    }

    #[test]
    fn a_file_that_cannot_be_made_does_not_read_as_one_that_cannot_be_read() {
        let failed = ShareError::Pack(io::Error::other("disk full"));
        assert_eq!(failed.to_string(), "its file cannot be made (disk full)");
        assert!(failed.source().is_some(), "the cause is kept");
    }

    #[test]
    fn each_picture_takes_the_name_a_saved_file_gives_it() {
        let (mut file, _image, red) = with_status();
        let blue = png(2, 2, [255, 0, 0, 255]);
        let (mut other, _own) = state_value("other", 2, 2);
        // Each value's picture named as the other's next would be, a fix
        // that shares its value's picture, and a sample named as a state.
        let CheckDef::State { states, .. } = &mut file.values[0].screen.check else {
            panic!("a state check");
        };
        "other-2.png".clone_into(&mut states[0].image);
        file.values[0].screen.fixes.push(FixDef {
            window_size: SizeDef {
                width: 4,
                height: 4,
            },
            region: RegionDef {
                x: 0,
                y: 0,
                width: 2,
                height: 2,
            },
            images: vec!["other-2.png".to_owned()],
            reference: None,
        });
        let CheckDef::State { states, .. } = &mut other.screen.check else {
            panic!("a state check");
        };
        "status-1.png".clone_into(&mut states[0].image);
        file.values.push(other);
        let mut digits = digits(1);
        "status-2.png".clone_into(&mut digits.samples[0].image);
        file.references.push(digits);
        let eleven = eleven(1);
        let bytes = zip_of(&[
            (APP_FILE, file.to_json().as_bytes()),
            ("other-2.png", &red),
            ("status-1.png", &blue),
            ("status-2.png", &eleven),
        ]);
        let (unpacked, pictures) = Shared::unpack(&bytes).expect("reads").into_parts();
        assert_eq!(
            unpacked.images(),
            [
                "ref-digits-1.png",
                "status-1.png",
                "status-1-1.png",
                "other-1.png"
            ]
        );
        assert_eq!(
            pictures,
            [
                ("other-1.png".to_owned(), blue),
                ("ref-digits-1.png".to_owned(), eleven),
                ("status-1-1.png".to_owned(), red.clone()),
                ("status-1.png".to_owned(), red),
            ],
            "each value reads what it read, from a picture of its own"
        );
    }

    #[test]
    fn only_what_the_file_names_is_taken_and_the_overlay_is_left_out() {
        let (mut file, image, picture) = with_status();
        file.overlay = Some(OverlayDef {
            enabled: true,
            mode: OverlayModeDef::PictureInPicture,
            panel: None,
        });
        let theirs = OriginDef::Imported {
            file: "their-file.zip".to_owned(),
        };
        file.values[0].origin = theirs.clone();
        file.references.push(ReferenceDef {
            origin: theirs,
            ..digits(1)
        });
        let sample = sample_name("digits", 1);
        let json = file.to_json();
        let bytes = zip_of(&[
            ("unnamed.png", &picture),
            ("../outside.png", &picture),
            ("program.exe", b"MZ"),
            (APP_FILE, json.as_bytes()),
            (&image, &picture),
            (&sample, &eleven(1)),
        ]);
        let (read, pictures) = Shared::unpack(&bytes).expect("reads").into_parts();
        file.overlay = None;
        let imported = OriginDef::Imported {
            file: String::new(),
        };
        file.values[0].origin = imported.clone();
        file.references[0].origin = imported;
        assert_eq!(read, file, "without what belongs to its maker's machine");
        assert_eq!(pictures, [(sample, eleven(1)), (image, picture)]);
    }

    prop_compose! {
        /// A definition that can be shared and its pictures by name: a few
        /// state values, each with a picture of its own size and pixels,
        /// and a reference or none with a few samples.
        fn shared()(
            name in "[A-Za-z][A-Za-z0-9 ]{0,20}",
            pictures in prop::collection::vec((1_u32..6, 1_u32..6, any::<[u8; 3]>()), 0..4),
            samples in prop::collection::vec(1_u32..4, 0..3),
            checked in prop::option::of((any::<u32>(), any::<u32>(), prop::option::of("[0-9.]{1,8}"))),
            origin in prop_oneof![
                Just(OriginDef::ByHand),
                Just(OriginDef::Recording),
                "[a-z]{0,8}".prop_map(|file| OriginDef::Imported { file }),
            ],
            anti_cheat in prop::sample::select(vec![
                AntiCheatDef::None,
                AntiCheatDef::UserMode,
                AntiCheatDef::KernelLevel,
                AntiCheatDef::Unknown,
            ]),
        ) -> (AppFile, BTreeMap<String, Vec<u8>>) {
            let mut file = app(&name);
            file.anti_cheat = anti_cheat;
            file.checked = checked.map(|(size, at, version)| CheckedDef {
                build: BuildDef { version, size: size.into(), modified: at.into() },
                at: at.into(),
            });
            let mut held = BTreeMap::new();
            for (number, (width, height, [blue, green, red])) in pictures.into_iter().enumerate() {
                let (mut value, image) = state_value(&format!("value_{number}"), width, height);
                value.origin = origin.clone();
                file.values.push(value);
                held.insert(image, png(width, height, [blue, green, red, 255]));
            }
            if !samples.is_empty() {
                let mut reference = digits(samples.len());
                reference.origin = origin;
                for (sample, scale) in reference.samples.iter().zip(samples) {
                    held.insert(sample.image.clone(), eleven(scale));
                }
                file.references.push(reference);
            }
            (file, held)
        }
    }

    proptest! {
        #[test]
        fn what_is_packed_unpacks_as_it_is_shared((file, held) in shared()) {
            let shared = Shared::new(file.clone(), |name| Ok(held[name].clone()))
                .expect("every value reads");
            let packed = pack_with(file, |name| Ok(held[name].clone())).expect("packs");
            prop_assert_eq!(Shared::unpack(&packed).expect("unpacks"), shared);
        }
    }
}
