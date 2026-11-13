//! The authoring file for one application: how to recognize its window and
//! the values AppyTappy reads from it.
//!
//! The setup app writes one `app.json` per application, in a folder of its
//! own that also holds the pictures its references learn from and its state
//! checks compare against. Each names its pictures by file name within that
//! folder.
//! The file is JSON, so a person can read it and a text diff shows a change.
//!
//! [`AppFile::parse`] checks the rules for the file as a whole: names, IDs,
//! picture file names, references, the window match, and each check's states
//! and bar colors. Whether each value can be read is up to `saved::build` in
//! `appytappy-sources`, which reads the pictures the file names, so a value
//! that cannot be read stops only itself.

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::anti_cheat::AntiCheatClass;
use crate::names::is_image_name;
use crate::signal::{SignalName, StateName};
use crate::window_match::{MustMatch, Pattern, WindowFacts, WindowMatch};

/// The format version this build writes and reads.
pub const FORMAT: u32 = 1;

/// The most states a state check may have.
pub const MAX_STATES: usize = 16;

/// The most fixes a value may have, each a drawing at another window size.
pub const MAX_FIXES: usize = 8;

/// One application's authoring file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppFile {
    /// The format version, [`FORMAT`].
    pub format: u32,
    /// The application's name as the user sees it, such as `Example App`.
    pub name: String,
    /// How to recognize the application's window.
    pub window: WindowDef,
    /// How often its window changes size, which says whether its values
    /// scale with the window. [`SizeChangesDef::Often`] when the file leaves
    /// it out.
    #[serde(default)]
    pub size_changes: SizeChangesDef,
    /// How the application protects itself from other programs.
    pub anti_cheat: AntiCheatDef,
    /// The values read from the application's window.
    pub values: Vec<ValueDef>,
    /// The characters its number values are read with. Always written,
    /// even when empty, since the setup app reads the file's JSON as is.
    #[serde(default)]
    pub references: Vec<ReferenceDef>,
    /// How the setup app shows the values' live readings on top of the
    /// application, or `None` when it never has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub overlay: Option<OverlayDef>,
    /// The build of the application its values were last checked against,
    /// or `None` before the setup app saved a value of it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub checked: Option<CheckedDef>,
}

/// How a value or a reference was made: its provenance.
///
/// Setup guidance only. The setup app shows it, so whoever reads a
/// definition can tell where each value and reference came from. Nothing
/// that finds the window or reads a value looks at it, and no rule of the
/// file covers it, whatever file name it holds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum OriginDef {
    /// The user made it in the setup app on screenshots they took: a value
    /// drawn on them, or a reference whose pictures they labeled.
    #[default]
    ByHand,
    /// Made in the setup app with a recording: a value set up on a
    /// screenshot that a recording of looks kept, or a reference that a
    /// recorded reading or such a screenshot taught.
    Recording,
    /// Imported with a file that someone exported.
    Imported {
        /// The name of the file it was imported from, such as
        /// `appytappy-example.zip`. Empty in a shared file, which says
        /// that its maker imported it and not from which file of theirs.
        file: String,
    },
}

/// The build of an application that its values were checked against, and
/// when.
///
/// An update can move or restyle what a value reads, so the setup app warns
/// when the running build is another one, and the runtime notes it in its
/// log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckedDef {
    /// The build.
    pub build: BuildDef,
    /// When the user checked them, in seconds since 1970 began in UTC.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub at: u64,
}

/// One build of an application, as its executable file tells it apart.
///
/// Many applications keep the same file version across updates, so the
/// file's size and the time it was written count too: two builds are the
/// same only when all three match.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BuildDef {
    /// The executable's file version, such as `1.4.2.0`, or `None` when the
    /// file has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub version: Option<String>,
    /// The executable's size in bytes.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub size: u64,
    /// When the executable was last written, in seconds since 1970 began in
    /// UTC.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub modified: u64,
}

/// How the setup app shows an application's live readings on top of it. The
/// runtime ignores it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OverlayDef {
    /// Whether it shows.
    pub enabled: bool,
    /// Where it shows.
    pub mode: OverlayModeDef,
    /// Where the user last put the picture-in-picture panel, or `None`
    /// before they moved it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub panel: Option<PanelDef>,
}

/// Where the live readings show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum OverlayModeDef {
    /// Over the application's window, following it and letting clicks
    /// through.
    Overlay,
    /// In a small panel the user moves and resizes, kept on top.
    PictureInPicture,
}

/// A picture-in-picture panel's place and size in screen pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PanelDef {
    /// The left edge.
    pub x: i32,
    /// The top edge.
    pub y: i32,
    /// The width.
    pub width: u32,
    /// The height.
    pub height: u32,
}

/// How to recognize an application's window: see [`WindowMatch`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WindowDef {
    /// The pattern for the window's title.
    pub title: String,
    /// The pattern for the window's class name.
    pub class: String,
    /// The pattern for the file name of the window's executable, which
    /// always has to match.
    pub executable: String,
    /// The fields besides the executable a window has to match to qualify.
    pub must_match: MustMatchDef,
    /// What to take when no window qualifies.
    pub fallback: FallbackDef,
}

/// [`MustMatch`] as the file spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MustMatchDef {
    /// The title has to match.
    pub title: bool,
    /// The class has to match.
    pub class: bool,
}

/// [`WindowMatch::fallback`] as the file spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum FallbackDef {
    /// No fallback: the application counts as closed.
    None,
    /// Any window of the executable.
    Executable,
}

impl WindowDef {
    /// The window as the setup app picked it, requiring what
    /// [`WindowMatch::exact`] does.
    #[must_use]
    pub fn exact(window: WindowFacts<'_>) -> Self {
        Self::from(&WindowMatch::exact(window))
    }

    /// The matcher this describes.
    #[must_use]
    pub fn to_match(&self) -> WindowMatch {
        let MustMatchDef { title, class } = self.must_match;
        WindowMatch {
            title: Pattern::new(&self.title),
            class: Pattern::new(&self.class),
            executable: Pattern::new(&self.executable),
            must_match: MustMatch { title, class },
            fallback: self.fallback == FallbackDef::Executable,
        }
    }

    /// Checks that the match can tell the application apart: an executable
    /// pattern that names enough of a program, and a pattern for each field
    /// that must match.
    ///
    /// # Errors
    ///
    /// Returns a [`WindowDefError`] naming the first problem.
    pub fn check(&self) -> Result<(), WindowDefError> {
        let executable = self.executable.trim().to_ascii_lowercase();
        if executable.is_empty() {
            return Err(WindowDefError::NoExecutable);
        }
        // A pattern with less than two characters of its own beside any
        // `.exe`, such as `*` or `*.*`, or one that takes two unrelated
        // programs, such as `*exe`, matches nearly every program, so the
        // application would always look running.
        let named = executable
            .strip_suffix(".exe")
            .unwrap_or(&executable)
            .chars()
            .filter(|each| !matches!(each, '*' | '?' | '.'))
            .count();
        let pattern = Pattern::new(&executable);
        if named < 2 || (pattern.matches("a.exe") && pattern.matches("zq.exe")) {
            return Err(WindowDefError::BroadExecutable);
        }
        for (required, pattern, error) in [
            (self.must_match.title, &self.title, WindowDefError::NoTitle),
            (self.must_match.class, &self.class, WindowDefError::NoClass),
        ] {
            if required && pattern.trim().is_empty() {
                return Err(error);
            }
        }
        Ok(())
    }
}

/// A window match cannot tell its application apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowDefError {
    /// The executable pattern is empty.
    NoExecutable,
    /// The executable pattern matches nearly every program.
    BroadExecutable,
    /// The title must match, but its pattern is empty.
    NoTitle,
    /// The class must match, but its pattern is empty.
    NoClass,
}

impl fmt::Display for WindowDefError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NoExecutable => "its executable is empty",
            Self::BroadExecutable => "its executable pattern matches nearly every program",
            Self::NoTitle => "its title must match but is empty",
            Self::NoClass => "its class must match but is empty",
        })
    }
}

impl Error for WindowDefError {}

impl From<&WindowMatch> for WindowDef {
    fn from(matcher: &WindowMatch) -> Self {
        let MustMatch { title, class } = matcher.must_match;
        Self {
            title: matcher.title.as_str().to_owned(),
            class: matcher.class.as_str().to_owned(),
            executable: matcher.executable.as_str().to_owned(),
            must_match: MustMatchDef { title, class },
            fallback: if matcher.fallback {
                FallbackDef::Executable
            } else {
                FallbackDef::None
            },
        }
    }
}

/// How often an application's window changes size, as the user answered.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum SizeChangesDef {
    /// Often, as in windowed play: at a size it was not drawn at, a value
    /// reads its region scaled with the window.
    #[default]
    Often,
    /// Rarely: a value reads only at the window sizes it was drawn at, and
    /// nothing is guessed.
    Rarely,
}

/// [`AntiCheatClass`] as the file spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum AntiCheatDef {
    /// [`AntiCheatClass::None`].
    None,
    /// [`AntiCheatClass::UserMode`].
    UserMode,
    /// [`AntiCheatClass::KernelLevel`].
    KernelLevel,
    /// [`AntiCheatClass::Unknown`].
    Unknown,
}

impl From<AntiCheatDef> for AntiCheatClass {
    fn from(class: AntiCheatDef) -> Self {
        match class {
            AntiCheatDef::None => Self::None,
            AntiCheatDef::UserMode => Self::UserMode,
            AntiCheatDef::KernelLevel => Self::KernelLevel,
            AntiCheatDef::Unknown => Self::Unknown,
        }
    }
}

/// One value read from the application's window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValueDef {
    /// The value's signal name, unique within the file, such as
    /// `progress`. Automations refer to the value by it.
    pub id: String,
    /// The value's name as the user sees it, such as `Progress`.
    pub name: String,
    /// How the value was made. [`OriginDef::ByHand`] when the file leaves
    /// it out.
    #[serde(default)]
    pub origin: OriginDef,
    /// Where on the window the value is read and how.
    pub screen: ScreenDef,
}

/// The characters of one font at one color, that number values read with.
///
/// It learns them from pictures of text in that font. One reference serves
/// every value drawn in its font, whatever the size: the reader scales each
/// character to its line's height.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct ReferenceDef {
    /// The reference's ID, unique within the file, such as `percent_font`.
    /// Number checks refer to it by this.
    pub id: String,
    /// The reference's name as the user sees it, such as `Percent font`.
    pub name: String,
    /// How the reference was made. [`OriginDef::ByHand`] when the file
    /// leaves it out.
    #[serde(default)]
    pub origin: OriginDef,
    /// The characters' color.
    pub ink: ColorDef,
    /// How far a pixel may stray from `ink` and still count.
    pub tolerance: u8,
    /// The pictures it learns from.
    pub samples: Vec<SampleDef>,
    /// The characters the user says it needs to know, such as
    /// `0123456789.%`, or `None` when the user chose none.
    ///
    /// Setup guidance only. The setup app shows which of them the samples
    /// have yet to teach, and writes them each once, in order, when it saves
    /// the reference. Nothing that learns or reads looks at it, and no rule
    /// of the file covers it: it adds no glyph, labels no shape, leaves how
    /// a number is read as it is, and never stops a file from loading,
    /// whatever text it holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub expects: Option<String>,
}

/// A value read from a region of the window's frame.
///
/// Its drawing is the window size it was drawn at, its region, and its
/// check. At any other size it reads its fix for that size, if it has one,
/// or else, while its application's window changes size
/// [often](SizeChangesDef::Often), its region scaled from this drawing and
/// kept at its pin's distance from that side of the window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScreenDef {
    /// The window's size when the region was drawn. A region's pixels mean
    /// something only at this size, or scaled from it.
    pub window_size: SizeDef,
    /// The region of the window's frame the check reads.
    pub region: RegionDef,
    /// The check.
    pub check: CheckDef,
    /// The side of the window the region stays at as the window changes
    /// size, or `None` for the side [`PinDef::guess`] picks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub pin: Option<PinDef>,
    /// Whether the user drew the region as a reading area: larger than what
    /// it reads, to cover every place content that moves inside the window
    /// can show. Left out of the file when it is not.
    ///
    /// Setup guidance only. The setup app shows the region as a reading
    /// area when the value is listed or drawn again. Nothing that places
    /// or reads the region looks at it, so the value reads the same with
    /// and without it. Only a check that
    /// [reads anywhere in its region](CheckDef::reads_anywhere) may have
    /// one.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reading_area: bool,
    /// The value drawn again at other window sizes, each read exactly at its
    /// own size. Always written, even when empty, since the setup app reads
    /// the file's JSON as is.
    #[serde(default)]
    pub fixes: Vec<FixDef>,
}

impl ScreenDef {
    /// The side the region stays at: the saved pin, or the one
    /// [`PinDef::guess`] picks when the file has none.
    #[must_use]
    pub fn pin(&self) -> PinDef {
        self.pin
            .unwrap_or_else(|| PinDef::guess(self.region, self.window_size))
    }

    /// Whether the value reads with the reference `id`, in its number check
    /// or in any fix.
    #[must_use]
    pub fn reads_with(&self, id: &str) -> bool {
        let checked = matches!(&self.check, CheckDef::Number { reference, .. } if reference == id);
        checked
            || self
                .fixes
                .iter()
                .any(|fix| fix.reference.as_deref() == Some(id))
    }

    /// The file names of the value's pictures: its state check's, then each
    /// fix's.
    pub fn images(&self) -> impl Iterator<Item = &str> {
        self.check
            .states()
            .iter()
            .map(|state| state.image.as_str())
            .chain(
                self.fixes
                    .iter()
                    .flat_map(|fix| fix.images.iter().map(String::as_str)),
            )
    }
}

/// A value drawn again at one window size, which it reads there exactly
/// instead of scaling its first drawing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FixDef {
    /// The window's size when it was drawn.
    pub window_size: SizeDef,
    /// The region at that size.
    pub region: RegionDef,
    /// For a state check, each state's picture at this size, in the order
    /// of the check's states; empty for any other check. Always written.
    #[serde(default)]
    pub images: Vec<String>,
    /// For a number check, the [`ReferenceDef::id`] of a reference learned
    /// at this size, or `None` to read with the check's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub reference: Option<String>,
}

/// Where along one side of the window a pinned region stays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Along {
    /// The left or the top.
    Start,
    /// The middle.
    Middle,
    /// The right or the bottom.
    End,
}

impl Along {
    /// The third of `length` that `twice_center`, twice a position along it,
    /// falls in. Twice the position keeps a region's center whole.
    fn third(twice_center: u64, length: u64) -> Self {
        let thirds = twice_center.saturating_mul(3);
        let twice = length.saturating_mul(2);
        if thirds < twice {
            Self::Start
        } else if thirds < twice.saturating_mul(2) {
            Self::Middle
        } else {
            Self::End
        }
    }
}

/// The side or corner of the window a value's region keeps its distance
/// from as the window changes size: one of nine, top, middle or bottom by
/// left, center or right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum PinDef {
    /// The top-left corner.
    TopLeft,
    /// The middle of the top edge.
    TopCenter,
    /// The top-right corner.
    TopRight,
    /// The middle of the left edge.
    MiddleLeft,
    /// The window's center.
    Center,
    /// The middle of the right edge.
    MiddleRight,
    /// The bottom-left corner.
    BottomLeft,
    /// The middle of the bottom edge.
    BottomCenter,
    /// The bottom-right corner.
    BottomRight,
}

impl PinDef {
    /// The pin at `horizontal` from the left and `vertical` from the top.
    #[must_use]
    pub const fn new(horizontal: Along, vertical: Along) -> Self {
        match (vertical, horizontal) {
            (Along::Start, Along::Start) => Self::TopLeft,
            (Along::Start, Along::Middle) => Self::TopCenter,
            (Along::Start, Along::End) => Self::TopRight,
            (Along::Middle, Along::Start) => Self::MiddleLeft,
            (Along::Middle, Along::Middle) => Self::Center,
            (Along::Middle, Along::End) => Self::MiddleRight,
            (Along::End, Along::Start) => Self::BottomLeft,
            (Along::End, Along::Middle) => Self::BottomCenter,
            (Along::End, Along::End) => Self::BottomRight,
        }
    }

    /// Where it sits from the left.
    #[must_use]
    pub const fn horizontal(self) -> Along {
        match self {
            Self::TopLeft | Self::MiddleLeft | Self::BottomLeft => Along::Start,
            Self::TopCenter | Self::Center | Self::BottomCenter => Along::Middle,
            Self::TopRight | Self::MiddleRight | Self::BottomRight => Along::End,
        }
    }

    /// Where it sits from the top.
    #[must_use]
    pub const fn vertical(self) -> Along {
        match self {
            Self::TopLeft | Self::TopCenter | Self::TopRight => Along::Start,
            Self::MiddleLeft | Self::Center | Self::MiddleRight => Along::Middle,
            Self::BottomLeft | Self::BottomCenter | Self::BottomRight => Along::End,
        }
    }

    /// The pin a region drawn on a window of `window` most likely stays at:
    /// the third of the window its center is in, across and down. A region
    /// in the bottom-left third, such as a health bar, stays at the bottom
    /// left.
    #[must_use]
    pub fn guess(region: RegionDef, window: SizeDef) -> Self {
        let twice = |start: u32, length: u32| {
            u64::from(start)
                .saturating_mul(2)
                .saturating_add(u64::from(length))
        };
        Self::new(
            Along::third(twice(region.x, region.width), u64::from(window.width)),
            Along::third(twice(region.y, region.height), u64::from(window.height)),
        )
    }
}

/// A size in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct SizeDef {
    /// The width.
    pub width: u32,
    /// The height.
    pub height: u32,
}

/// A rectangle in pixels from the window's top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct RegionDef {
    /// The left edge.
    pub x: u32,
    /// The top edge.
    pub y: u32,
    /// The width.
    pub width: u32,
    /// The height.
    pub height: u32,
}

/// A color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct ColorDef {
    /// The red channel.
    pub red: u8,
    /// The green channel.
    pub green: u8,
    /// The blue channel.
    pub blue: u8,
}

/// How a value is read from its region.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum CheckDef {
    /// Reads a number with the characters of a reference.
    Number {
        /// The [`ReferenceDef::id`] of the reference to read with.
        reference: String,
        /// Whether only the whole part counts: every digit after the decimal
        /// point is ignored, so `87.5%` reads as 87.
        whole: bool,
        /// The characters the value reads, such as `0123456789%`, or `None`
        /// for every character its reference knows.
        ///
        /// The value matches what it sees against these characters alone,
        /// so a value that shows only digits cannot read a `0` as an `O`
        /// its reference also knows. Whitespace does not count, as in a
        /// label, and neither does a character the reference has yet to
        /// learn. A fix that reads with its own reference reads these
        /// characters of it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "ts", ts(optional))]
        characters: Option<String>,
    },
    /// How full a bar is, as the share of pixels nearer `filled` than
    /// `empty`.
    #[serde(rename_all = "camelCase")]
    BarFill {
        /// The bar's filled color.
        filled: ColorDef,
        /// The bar's empty color.
        empty: ColorDef,
    },
    /// Whether the region's average color is near `color`.
    Color {
        /// The color to look for.
        color: ColorDef,
        /// How far the average may stray from it.
        tolerance: u8,
    },
    /// Which of a few named looks the region shows, or none.
    State {
        /// The looks, in the order the setup app lists them. A tie goes to
        /// the earlier state.
        states: Vec<StateDef>,
        /// How far, on average, the region's red, green, and blue may stray
        /// from a state's picture for it to count.
        tolerance: u8,
    },
}

/// One look a state check knows, such as a corner showing `Paused`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct StateDef {
    /// The state's name, which the value reads when the region looks like
    /// it. [`StateName::new`] says which names are allowed.
    pub name: String,
    /// The file name of the region's picture in this state. Its transparent
    /// pixels are the background, which the check ignores.
    pub image: String,
}

/// A picture of text in a reference's font and the text it shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct SampleDef {
    /// The picture's file name.
    pub image: String,
    /// What the picture shows, such as `87%`.
    pub text: String,
}

impl CheckDef {
    /// The kind of value the check reads.
    #[must_use]
    pub const fn reads(&self) -> ValueKind {
        match self {
            Self::Number { .. } | Self::BarFill { .. } => ValueKind::Number,
            Self::Color { .. } => ValueKind::Flag,
            Self::State { .. } => ValueKind::State,
        }
    }

    /// Whether the check reads its content wherever it is in the region, so
    /// that the region may be a reading area: a number check finds its
    /// characters anywhere in it, and a bar fill check counts its colors
    /// anywhere in it. A color check averages the whole region and a state
    /// check compares it pixel by pixel, so their content has to stay put.
    #[must_use]
    pub const fn reads_anywhere(&self) -> bool {
        match self {
            Self::Number { .. } | Self::BarFill { .. } => true,
            Self::Color { .. } | Self::State { .. } => false,
        }
    }

    /// The states this check tells apart; none for a check of another kind.
    #[must_use]
    pub fn states(&self) -> &[StateDef] {
        match self {
            Self::State { states, .. } => states,
            Self::Number { .. } | Self::BarFill { .. } | Self::Color { .. } => &[],
        }
    }

    /// Whether the check reads the state `name`: one of its states, or
    /// [`crate::signal::NO_STATE`], which every state check can read.
    #[must_use]
    pub fn reads_state(&self, name: &str) -> bool {
        matches!(self, Self::State { .. })
            && (name == StateName::none().as_str()
                || self.states().iter().any(|state| state.name == name))
    }

    /// Checks the rules the check's own settings can break: a state check's
    /// states, as [`check_states`] says, that a bar's two colors differ, and
    /// that a number check told which characters to read reads at least
    /// one.
    ///
    /// # Errors
    ///
    /// Returns a [`CheckDefError`] naming the first problem.
    pub fn check(&self) -> Result<(), CheckDefError> {
        match self {
            Self::BarFill { filled, empty } => check_bar(*filled, *empty),
            Self::State { states, .. } => {
                let names: Vec<&str> = states.iter().map(|state| state.name.as_str()).collect();
                check_states(&names)
            },
            Self::Number {
                characters: Some(characters),
                ..
            } if !characters.chars().any(is_label_character) => Err(CheckDefError::NoCharacters),
            Self::Number { .. } | Self::Color { .. } => Ok(()),
        }
    }
}

/// Checks a state check's state `names`: at least one and at most
/// [`MAX_STATES`], and no name twice. Names that differ only in case count
/// as the same, since they read alike.
///
/// # Errors
///
/// Returns a [`CheckDefError`] naming the first problem.
pub fn check_states(names: &[&str]) -> Result<(), CheckDefError> {
    if names.is_empty() {
        return Err(CheckDefError::NoStates);
    }
    if names.len() > MAX_STATES {
        return Err(CheckDefError::TooManyStates);
    }
    let mut seen = BTreeSet::new();
    for name in names {
        if !seen.insert(name.to_lowercase()) {
            return Err(CheckDefError::SameStateName((*name).to_owned()));
        }
    }
    Ok(())
}

/// Checks that a bar's `filled` and `empty` colors differ, so its fill can
/// be told.
fn check_bar(filled: ColorDef, empty: ColorDef) -> Result<(), CheckDefError> {
    if filled == empty {
        Err(CheckDefError::SameColors)
    } else {
        Ok(())
    }
}

/// A value's check breaks a rule of its own settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckDefError {
    /// A state check has no states.
    NoStates,
    /// A state check has more than [`MAX_STATES`] states.
    TooManyStates,
    /// Two states have this name, or names that differ only in case.
    SameStateName(String),
    /// The bar's filled and empty colors are the same.
    SameColors,
    /// A number check is told which characters to read, and reads none.
    NoCharacters,
}

impl fmt::Display for CheckDefError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoStates => formatter.write_str("its state check has no state"),
            Self::TooManyStates => write!(
                formatter,
                "its state check has more than {MAX_STATES} states"
            ),
            Self::SameStateName(name) => {
                write!(formatter, "two of its states are named {name:?}")
            },
            Self::SameColors => {
                formatter.write_str("its bar's filled and empty colors are the same")
            },
            Self::NoCharacters => formatter.write_str("its number check reads no characters"),
        }
    }
}

impl Error for CheckDefError {}

/// The kind of value a check reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    /// A number, as a number or bar fill check reads.
    Number,
    /// Yes or no, as a color check reads.
    Flag,
    /// A state's name, as a state check reads.
    State,
}

impl AppFile {
    /// Reads and checks a file's JSON.
    ///
    /// # Errors
    ///
    /// Returns an [`AppFileError`] naming the first problem: another format
    /// version, JSON that does not fit the format, or a value that breaks a
    /// rule in [`AppFile::check`]. The version is read first, so a file of
    /// another format says so rather than naming whichever of its fields
    /// this format lacks.
    pub fn parse(json: &str) -> Result<Self, AppFileError> {
        /// Any file's format version, whatever else it holds. Every other
        /// field is skipped as it is read, so the peek keeps nothing of a
        /// file that someone else made, however large.
        #[derive(Deserialize)]
        struct Versioned {
            format: Option<u64>,
        }
        // JSON the peek cannot read, or without a version this format's
        // number can hold, is left for the file's own reading to refuse, so
        // the message names the file and not the peek.
        let format = serde_json::from_str::<Versioned>(json)
            .ok()
            .and_then(|peek| peek.format)
            .and_then(|format| u32::try_from(format).ok());
        if let Some(format) = format.filter(|format| *format != FORMAT) {
            return Err(AppFileError::Format(format));
        }
        let file: Self =
            serde_json::from_str(json).map_err(|error| AppFileError::Json(error.to_string()))?;
        file.check()?;
        Ok(file)
    }

    /// The file as pretty JSON, ending with a newline.
    #[must_use]
    pub fn to_json(&self) -> String {
        // The format holds only strings, numbers, and lists, all of which
        // JSON can write, so serializing cannot fail; an empty object stands
        // in should that ever change.
        let mut json = serde_json::to_string_pretty(self).unwrap_or_else(|_error| "{}".to_owned());
        json.push('\n');
        json
    }

    /// The file names of every picture the file names: its references'
    /// samples and its state checks' pictures.
    #[must_use]
    pub fn images(&self) -> Vec<&str> {
        self.references
            .iter()
            .flat_map(|reference| reference.samples.iter().map(|sample| sample.image.as_str()))
            .chain(self.values.iter().flat_map(|value| value.screen.images()))
            .collect()
    }

    /// How each value, then each reference, was made, to change.
    pub fn origins_mut(&mut self) -> impl Iterator<Item = &mut OriginDef> {
        self.values.iter_mut().map(|value| &mut value.origin).chain(
            self.references
                .iter_mut()
                .map(|reference| &mut reference.origin),
        )
    }

    /// The reference with the ID `id`, if the file has one.
    #[must_use]
    pub fn reference(&self, id: &str) -> Option<&ReferenceDef> {
        self.references.iter().find(|reference| reference.id == id)
    }

    /// Checks the rules JSON alone cannot: the format version, a name for
    /// the application and each value and reference, a window match that
    /// passes [`WindowDef::check`], valid and unique IDs, picture file names,
    /// at least one sample for a reference, checks that pass
    /// [`CheckDef::check`], and a reading area only for a check that reads
    /// anywhere in its region. Whether a value's pictures and region can be
    /// read is up to `appytappy-sources`' `saved::build`, so a value that
    /// cannot be read stops only itself. A reference's expected characters
    /// are setup guidance, which no rule covers.
    ///
    /// # Errors
    ///
    /// Returns an [`AppFileError`] naming the first problem.
    pub fn check(&self) -> Result<(), AppFileError> {
        if self.format != FORMAT {
            return Err(AppFileError::Format(self.format));
        }
        if self.name.trim().is_empty() {
            return Err(AppFileError::NoName);
        }
        self.window.check().map_err(AppFileError::Window)?;
        let mut references = BTreeSet::new();
        for reference in &self.references {
            let problem = |problem: String| AppFileError::Reference(reference.id.clone(), problem);
            let id = SignalName::new(&reference.id).map_err(|error| problem(error.to_string()))?;
            if !references.insert(id) {
                return Err(problem("another reference has the same ID".to_owned()));
            }
            check_reference(reference).map_err(problem)?;
        }
        let mut ids = BTreeSet::new();
        for value in &self.values {
            let id = SignalName::new(&value.id)
                .map_err(|error| AppFileError::Value(value.id.clone(), error.to_string()))?;
            if !ids.insert(id) {
                return Err(AppFileError::Value(
                    value.id.clone(),
                    "another value has the same ID".to_owned(),
                ));
            }
            check_value(value).map_err(|problem| AppFileError::Value(value.id.clone(), problem))?;
        }
        Ok(())
    }
}

/// Checks one reference's rules, answering the problem as a sentence.
fn check_reference(reference: &ReferenceDef) -> Result<(), String> {
    if reference.name.trim().is_empty() {
        return Err("it has no name".to_owned());
    }
    if reference.samples.is_empty() {
        return Err("it has no sample to learn from".to_owned());
    }
    reference
        .samples
        .iter()
        .try_for_each(|sample| check_image(&sample.image))
}

/// Whether a label teaches `character`: every character but whitespace,
/// which a label may hold anywhere and a reference skips.
///
/// A reference learns one shape for each such character of a sample's text,
/// exactly as typed: `A` and `a` are two characters, and nothing folds case
/// or combines marks. The setup app counts a reference's expected characters
/// the same way.
#[must_use]
pub fn is_label_character(character: char) -> bool {
    !character.is_whitespace()
}

/// Checks one value's part of the file's rules, answering the problem as a
/// sentence: a name, a check that passes [`CheckDef::check`], a reading
/// area only for a check that
/// [reads anywhere in its region](CheckDef::reads_anywhere), and picture
/// names the folder can hold.
fn check_value(value: &ValueDef) -> Result<(), String> {
    if value.name.trim().is_empty() {
        return Err("it has no name".to_owned());
    }
    if value.screen.reading_area && !value.screen.check.reads_anywhere() {
        return Err(
            "its region is a reading area, which only a number or bar fill check can read"
                .to_owned(),
        );
    }
    value
        .screen
        .check
        .check()
        .map_err(|error| error.to_string())?;
    value.screen.images().try_for_each(check_image)
}

/// Checks that `image` is a picture name the folder can hold.
fn check_image(image: &str) -> Result<(), String> {
    if is_image_name(image) {
        Ok(())
    } else {
        Err(format!(
            "picture {image:?} must be a lowercase .png file name of letters, digits, hyphens, \
             and underscores"
        ))
    }
}

/// An application's file cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppFileError {
    /// The JSON does not fit the format.
    Json(String),
    /// The file has a format version this build does not know.
    Format(u32),
    /// The application has no name.
    NoName,
    /// The window match cannot tell the application apart.
    Window(WindowDefError),
    /// A value, named by its ID, breaks a rule.
    Value(String, String),
    /// A reference, named by its ID, breaks a rule.
    Reference(String, String),
}

impl fmt::Display for AppFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "the file is not valid: {error}"),
            Self::Format(format) => write!(
                formatter,
                "the file has format {format}, but this version of AppyTappy reads format {FORMAT}"
            ),
            Self::NoName => formatter.write_str("the application has no name"),
            Self::Window(error) => write!(formatter, "the window match: {error}"),
            Self::Value(id, problem) => write!(formatter, "value {id:?}: {problem}"),
            Self::Reference(id, problem) => write!(formatter, "reference {id:?}: {problem}"),
        }
    }
}

impl Error for AppFileError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::names::sample_name;

    const RED: ColorDef = ColorDef {
        red: 255,
        green: 0,
        blue: 0,
    };

    fn states(states: &[(&str, &str)]) -> CheckDef {
        CheckDef::State {
            states: states
                .iter()
                .map(|(name, image)| StateDef {
                    name: (*name).to_owned(),
                    image: (*image).to_owned(),
                })
                .collect(),
            tolerance: 9,
        }
    }

    fn dash() -> AppFile {
        AppFile {
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
            values: vec![ValueDef {
                id: "progress".to_owned(),
                name: "Progress".to_owned(),
                origin: OriginDef::ByHand,
                screen: ScreenDef {
                    window_size: SizeDef {
                        width: 1920,
                        height: 1080,
                    },
                    region: RegionDef {
                        x: 1146,
                        y: 8,
                        width: 74,
                        height: 38,
                    },
                    check: CheckDef::Number {
                        reference: "percent".to_owned(),
                        whole: true,
                        characters: None,
                    },
                    pin: Some(PinDef::TopCenter),
                    reading_area: false,
                    fixes: Vec::new(),
                },
            }],
            references: vec![ReferenceDef {
                id: "percent".to_owned(),
                name: "Percent".to_owned(),
                origin: OriginDef::ByHand,
                ink: ColorDef {
                    red: 255,
                    green: 255,
                    blue: 255,
                },
                tolerance: 40,
                samples: vec![SampleDef {
                    image: sample_name("percent", 1),
                    text: "87%".to_owned(),
                }],
                expects: None,
            }],
        }
    }

    #[test]
    fn a_file_round_trips_through_its_json() {
        let file = dash();
        let json = file.to_json();
        assert!(json.ends_with("}\n"));
        assert!(json.contains("\"antiCheat\": \"unknown\""));
        assert!(json.contains("\"fallback\": \"none\""));
        assert!(json.contains("\"mustMatch\""));
        assert!(json.contains("\"kind\": \"number\""));
        assert!(json.contains("\"windowSize\""));
        assert!(json.contains("\"whole\": true"));
        assert!(json.contains("\"ref-percent-1.png\""));
        assert_eq!(AppFile::parse(&json), Ok(file));
    }

    #[test]
    fn a_file_without_references_still_lists_them() {
        let mut file = dash();
        file.values.clear();
        file.references.clear();
        let json = file.to_json();
        assert!(json.contains("\"references\": []"));
        assert_eq!(AppFile::parse(&json), Ok(file.clone()));
        // A file written before references reads as having none.
        let older = json.replace(",\n  \"references\": []", "");
        assert!(!older.contains("references"));
        assert_eq!(AppFile::parse(&older), Ok(file));
    }

    #[test]
    fn a_reference_s_expected_characters_are_left_out_until_chosen_and_round_trip() {
        let mut file = dash();
        let json = file.to_json();
        assert!(!json.contains("expects"), "{json}");
        assert_eq!(AppFile::parse(&json), Ok(file.clone()));
        file.references[0].expects = Some("0123456789.%Aa".to_owned());
        let json = file.to_json();
        assert!(json.contains("\"expects\": \"0123456789.%Aa\""), "{json}");
        assert_eq!(AppFile::parse(&json), Ok(file));
    }

    #[test]
    fn a_number_check_s_characters_are_left_out_until_chosen_and_round_trip() {
        let mut file = dash();
        let json = file.to_json();
        assert!(!json.contains("characters"), "{json}");
        assert_eq!(AppFile::parse(&json), Ok(file.clone()));
        file.values[0].screen.check = CheckDef::Number {
            reference: "percent".to_owned(),
            whole: true,
            characters: Some("0123456789 %".to_owned()),
        };
        let json = file.to_json();
        assert!(json.contains("\"characters\": \"0123456789 %\""), "{json}");
        assert_eq!(AppFile::parse(&json), Ok(file));
    }

    #[test]
    fn no_rule_covers_what_a_reference_expects() {
        // Setup guidance that a person edited by hand: repeats, whitespace,
        // a control character, nothing at all, or far more than the setup
        // app saves. The file loads as it does without it.
        let plain = dash();
        for odd in ["11 ", "", "0 1\u{7}\n", &"x".repeat(5000)] {
            let mut file = dash();
            file.references[0].expects = Some(odd.to_owned());
            assert_eq!(file.check(), Ok(()), "{odd:?}");
            let read = AppFile::parse(&file.to_json()).expect("loads");
            assert_eq!(read.references[0].expects.as_deref(), Some(odd));
            assert_eq!(read.values, plain.values);
            assert_eq!(read.references[0].samples, plain.references[0].samples);
        }
    }

    #[test]
    fn a_label_teaches_every_character_but_whitespace() {
        assert!(is_label_character('1') && is_label_character('\u{7}'));
        assert!(is_label_character('\u{feff}') && is_label_character('\u{301}'));
        assert!(!is_label_character(' ') && !is_label_character('\n'));
        assert!(!is_label_character('\u{85}') && !is_label_character('\u{3000}'));
    }

    #[test]
    fn what_a_file_leaves_out_takes_its_default() {
        let file = dash();
        let json = file.to_json();
        assert!(json.contains("\"sizeChanges\": \"often\""));
        assert!(json.contains("\"pin\": \"topCenter\""));
        assert!(json.contains("\"fixes\": []"));
        let bare = json
            .replace("  \"sizeChanges\": \"often\",\n", "")
            .replace("        \"pin\": \"topCenter\",\n", "")
            .replace(",\n        \"fixes\": []", "");
        assert!(!bare.contains("sizeChanges") && !bare.contains("pin") && !bare.contains("fixes"));
        let read = AppFile::parse(&bare).expect("reads");
        assert_eq!(read.size_changes, SizeChangesDef::Often);
        let screen = &read.values[0].screen;
        assert_eq!((screen.pin, screen.fixes.as_slice()), (None, [].as_slice()));
        // Without a pin, the region's place picks one: it sits at the top
        // of a 1920 wide window, around 1183 across.
        assert_eq!(screen.pin(), PinDef::TopCenter);
        let mut rarely = file;
        rarely.size_changes = SizeChangesDef::Rarely;
        assert!(rarely.to_json().contains("\"sizeChanges\": \"rarely\""));
        assert_eq!(AppFile::parse(&rarely.to_json()), Ok(rarely));
    }

    #[test]
    fn a_reading_area_is_left_out_until_set_and_only_for_a_check_that_reads_anywhere() {
        let mut file = dash();
        let json = file.to_json();
        assert!(!json.contains("readingArea"), "{json}");
        assert_eq!(AppFile::parse(&json), Ok(file.clone()));
        let black = ColorDef {
            red: 0,
            green: 0,
            blue: 0,
        };
        let bar = CheckDef::BarFill {
            filled: RED,
            empty: black,
        };
        let color = CheckDef::Color {
            color: RED,
            tolerance: 9,
        };
        let state = states(&[("Paused", "status-1.png")]);
        // A number or a bar is read wherever it is in its region.
        file.values[0].screen.reading_area = true;
        let json = file.to_json();
        assert!(json.contains("\"readingArea\": true"), "{json}");
        assert_eq!(AppFile::parse(&json), Ok(file.clone()));
        file.values[0].screen.check = bar;
        assert_eq!(file.check(), Ok(()));
        // A color or a state needs its content to stay put.
        for check in [color, state] {
            file.values[0].screen.check = check;
            assert_eq!(
                file.check(),
                Err(AppFileError::Value(
                    "progress".to_owned(),
                    "its region is a reading area, which only a number or bar fill check can read"
                        .to_owned()
                ))
            );
            file.values[0].screen.reading_area = false;
            assert_eq!(file.check(), Ok(()));
            file.values[0].screen.reading_area = true;
        }
    }

    fn fix(images: &[&str]) -> FixDef {
        FixDef {
            window_size: SizeDef {
                width: 3440,
                height: 1440,
            },
            region: RegionDef {
                x: 1500,
                y: 10,
                width: 98,
                height: 50,
            },
            images: images.iter().map(|&image| image.to_owned()).collect(),
            reference: None,
        }
    }

    #[test]
    fn a_fix_round_trips_and_names_its_pictures() {
        let mut file = dash();
        let mut status = file.values[0].clone();
        status.id = "status".to_owned();
        status.screen.check = states(&[("Paused", "status-1.png"), ("Live", "status-2.png")]);
        status.screen.fixes = vec![fix(&["status-1-1.png", "status-1-2.png"])];
        file.values.push(status);
        file.values[0].screen.fixes = vec![FixDef {
            reference: Some("percent".to_owned()),
            ..fix(&[])
        }];
        let json = file.to_json();
        assert!(json.contains("\"reference\": \"percent\""));
        assert_eq!(AppFile::parse(&json), Ok(file.clone()));
        assert_eq!(
            file.images(),
            [
                "ref-percent-1.png",
                "status-1.png",
                "status-2.png",
                "status-1-1.png",
                "status-1-2.png"
            ]
        );
        file.values[1].screen.fixes[0].images[1] = "../out.png".to_owned();
        let error = AppFile::parse(&file.to_json()).expect_err("leaves the folder");
        assert!(
            error
                .to_string()
                .starts_with("value \"status\": picture \"../out.png\" must be"),
            "{error}"
        );
    }

    #[test]
    fn a_value_reads_with_its_check_s_reference_and_each_fix_s_only() {
        let mut screen = dash().values[0].screen.clone();
        assert!(screen.reads_with("percent"), "its number check's");
        assert!(!screen.reads_with("small"));
        screen.fixes = vec![
            fix(&[]),
            FixDef {
                reference: Some("small".to_owned()),
                ..fix(&[])
            },
        ];
        assert!(screen.reads_with("small"), "a fix's");
        assert!(!screen.reads_with("other"));
        screen.check = states(&[("Paused", "status-1.png")]);
        screen.fixes.clear();
        assert!(
            !screen.reads_with("percent"),
            "a state check reads with none"
        );
    }

    #[test]
    fn a_pin_guess_takes_the_third_the_regions_center_is_in() {
        let window = SizeDef {
            width: 300,
            height: 90,
        };
        let at = |x, y| {
            PinDef::guess(
                RegionDef {
                    x,
                    y,
                    width: 2,
                    height: 2,
                },
                window,
            )
        };
        // A center at 99 across is in the first third, at 100 in the second,
        // at 199 still in the second, and at 200 in the last; down, 29, 30,
        // 59 and 60 split the same way.
        assert_eq!(at(98, 28), PinDef::TopLeft);
        assert_eq!(at(99, 28), PinDef::TopCenter);
        assert_eq!(at(198, 28), PinDef::TopCenter);
        assert_eq!(at(199, 28), PinDef::TopRight);
        assert_eq!(at(98, 29), PinDef::MiddleLeft);
        assert_eq!(at(150, 45), PinDef::Center);
        assert_eq!(at(199, 58), PinDef::MiddleRight);
        assert_eq!(at(0, 59), PinDef::BottomLeft);
        assert_eq!(at(150, 88), PinDef::BottomCenter);
        assert_eq!(at(298, 88), PinDef::BottomRight);
        for pin in [
            PinDef::TopLeft,
            PinDef::TopCenter,
            PinDef::TopRight,
            PinDef::MiddleLeft,
            PinDef::Center,
            PinDef::MiddleRight,
            PinDef::BottomLeft,
            PinDef::BottomCenter,
            PinDef::BottomRight,
        ] {
            assert_eq!(PinDef::new(pin.horizontal(), pin.vertical()), pin);
        }
    }

    #[test]
    fn the_file_names_its_references_samples_and_its_state_checks_pictures() {
        let mut file = dash();
        let mut status = file.values[0].clone();
        status.id = "status".to_owned();
        status.screen.check = states(&[("Paused", "status-1.png"), ("Live", "status-2.png")]);
        file.values.push(status);
        assert_eq!(
            file.images(),
            ["ref-percent-1.png", "status-1.png", "status-2.png"]
        );
        assert_eq!(
            file.reference("percent")
                .map(|reference| reference.tolerance),
            Some(40)
        );
        assert!(file.reference("progress").is_none());
        assert_eq!(file.values[0].screen.check.states(), []);
    }

    #[test]
    fn checks_say_what_they_read() {
        let file = dash();
        assert_eq!(file.values[0].screen.check.reads(), ValueKind::Number);
        let bar = CheckDef::BarFill {
            filled: RED,
            empty: RED,
        };
        assert_eq!(bar.reads(), ValueKind::Number);
        let color = CheckDef::Color {
            color: RED,
            tolerance: 3,
        };
        assert_eq!(color.reads(), ValueKind::Flag);
        let status = states(&[("Paused", "status-1.png"), ("Live", "status-2.png")]);
        assert_eq!(status.reads(), ValueKind::State);
        for (name, read) in [
            ("Paused", true),
            ("Live", true),
            ("none", true),
            ("paused", false),
            ("Dead", false),
        ] {
            assert_eq!(status.reads_state(name), read, "{name}");
        }
        assert!(!color.reads_state("none"));
        assert!(!file.values[0].screen.check.reads_state("none"));
    }

    #[test]
    fn the_overlay_setting_is_left_out_until_set_and_round_trips() {
        let mut file = dash();
        assert!(!file.to_json().contains("overlay"));
        file.overlay = Some(OverlayDef {
            enabled: true,
            mode: OverlayModeDef::PictureInPicture,
            panel: Some(PanelDef {
                x: -1200,
                y: 40,
                width: 240,
                height: 160,
            }),
        });
        let json = file.to_json();
        assert!(json.contains("\"mode\": \"pictureInPicture\""));
        assert_eq!(AppFile::parse(&json), Ok(file));
    }

    #[test]
    fn the_checked_build_is_left_out_until_recorded_and_round_trips() {
        let mut file = dash();
        assert!(!file.to_json().contains("checked"));
        file.checked = Some(CheckedDef {
            build: BuildDef {
                version: None,
                size: 4_096,
                modified: 1_762_500_000,
            },
            at: 1_762_588_800,
        });
        let json = file.to_json();
        assert!(!json.contains("version"));
        assert_eq!(AppFile::parse(&json), Ok(file.clone()));
        if let Some(checked) = &mut file.checked {
            checked.build.version = Some("1.4.2.0".to_owned());
        }
        assert_eq!(AppFile::parse(&file.to_json()), Ok(file));
    }

    #[test]
    fn how_each_was_made_round_trips_and_reads_as_by_hand_when_left_out() {
        let mut file = dash();
        let json = file.to_json();
        let mut left_out: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        for list in ["values", "references"] {
            let removed = left_out[list][0]
                .as_object_mut()
                .expect("an object")
                .remove("origin");
            assert_eq!(
                removed,
                Some(serde_json::json!({ "kind": "byHand" })),
                "{list}"
            );
        }
        assert_eq!(AppFile::parse(&left_out.to_string()), Ok(file.clone()));
        file.values[0].origin = OriginDef::Recording;
        file.references[0].origin = OriginDef::Imported {
            file: "appytappy-example.zip".to_owned(),
        };
        assert_eq!(AppFile::parse(&file.to_json()), Ok(file.clone()));
        // An application's file says nothing of how it was made as a whole.
        let mut whole: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        whole["origin"] = serde_json::json!({ "kind": "byHand" });
        assert!(matches!(
            AppFile::parse(&whole.to_string()),
            Err(AppFileError::Json(_))
        ));
    }

    #[test]
    fn another_format_says_so_whatever_else_the_file_holds() {
        let newer = dash()
            .to_json()
            .replace("\"format\": 1", "\"format\": 2, \"added\": true");
        assert_eq!(AppFile::parse(&newer), Err(AppFileError::Format(2)));
        assert_eq!(
            AppFile::parse("[]"),
            Err(AppFileError::Json(
                "invalid length 0, expected struct AppFile with 9 elements at line 1 column 2"
                    .to_owned()
            )),
            "the message names the file, not how it is read"
        );
        // A version no format's number holds is the file's own to refuse.
        for format in ["\"2\"", "4294967296", "null"] {
            let odd = dash()
                .to_json()
                .replace("\"format\": 1", &format!("\"format\": {format}"));
            assert!(
                matches!(AppFile::parse(&odd), Err(AppFileError::Json(message)) if !message.contains("Versioned")),
                "{format}"
            );
        }
    }

    #[test]
    fn every_check_kind_round_trips() {
        let checks = [
            CheckDef::BarFill {
                filled: RED,
                empty: ColorDef {
                    red: 40,
                    green: 40,
                    blue: 40,
                },
            },
            CheckDef::Color {
                color: RED,
                tolerance: 3,
            },
            states(&[("Paused", "status-1.png"), ("Live", "status-2.png")]),
            CheckDef::Number {
                reference: "percent".to_owned(),
                whole: false,
                characters: None,
            },
        ];
        for check in checks {
            let mut file = dash();
            file.values[0].screen.check = check;
            assert_eq!(AppFile::parse(&file.to_json()), Ok(file));
        }
    }

    #[test]
    fn the_window_def_describes_its_matcher() {
        let def = dash().window;
        assert_eq!(def.title, "Example App");
        assert_eq!(
            def.must_match,
            MustMatchDef {
                title: false,
                class: false
            }
        );
        assert_eq!(def.fallback, FallbackDef::None);
        for (fallback, takes_any) in [(FallbackDef::None, false), (FallbackDef::Executable, true)] {
            for (title, class) in [(true, false), (false, true)] {
                let def = WindowDef {
                    must_match: MustMatchDef { title, class },
                    fallback,
                    ..def.clone()
                };
                let matcher = def.to_match();
                assert_eq!(matcher.must_match, MustMatch { title, class });
                assert_eq!(matcher.fallback, takes_any);
                assert_eq!(WindowDef::from(&matcher), def);
            }
        }
    }

    #[test]
    fn anti_cheat_classes_map_one_to_one() {
        let pairs = [
            (AntiCheatDef::None, AntiCheatClass::None),
            (AntiCheatDef::UserMode, AntiCheatClass::UserMode),
            (AntiCheatDef::KernelLevel, AntiCheatClass::KernelLevel),
            (AntiCheatDef::Unknown, AntiCheatClass::Unknown),
        ];
        for (def, class) in pairs {
            assert_eq!(AntiCheatClass::from(def), class);
        }
    }

    #[test]
    fn broken_files_name_their_first_problem() {
        let mut format = dash();
        format.format = 2;
        let mut unnamed = dash();
        unnamed.name = " ".to_owned();
        let mut bad_id = dash();
        bad_id.values[0].id = "Score".to_owned();
        let mut twice = dash();
        twice.values.push(twice.values[0].clone());
        let mut no_value_name = dash();
        no_value_name.values[0].name = String::new();
        let mut no_samples = dash();
        no_samples.references[0].samples.clear();
        let mut bad_reference = dash();
        bad_reference.references[0].id = "Percent".to_owned();
        let mut two_references = dash();
        two_references
            .references
            .push(two_references.references[0].clone());
        let mut unnamed_reference = dash();
        unnamed_reference.references[0].name = "  ".to_owned();
        let mut sample_escape = dash();
        sample_escape.references[0].samples[0].image = "../1.png".to_owned();
        let with = |check: CheckDef| {
            let mut file = dash();
            file.values[0].screen.check = check;
            file
        };
        let escape = with(states(&[("Paused", "../app.png")]));
        let no_states = with(states(&[]));
        let one_color = with(CheckDef::BarFill {
            filled: RED,
            empty: RED,
        });
        let no_characters = with(CheckDef::Number {
            reference: "percent".to_owned(),
            whole: false,
            characters: Some(" \t".to_owned()),
        });
        let mut broad = dash();
        broad.window.executable = "*".to_owned();
        let cases = [
            (
                format,
                "the file has format 2, but this version of AppyTappy reads format 1",
            ),
            (unnamed, "the application has no name"),
            (bad_id, "value \"Score\": signal name"),
            (twice, "value \"progress\": another value has the same ID"),
            (no_value_name, "value \"progress\": it has no name"),
            (
                no_samples,
                "reference \"percent\": it has no sample to learn from",
            ),
            (bad_reference, "reference \"Percent\": signal name"),
            (
                two_references,
                "reference \"percent\": another reference has the same ID",
            ),
            (unnamed_reference, "reference \"percent\": it has no name"),
            (
                sample_escape,
                "reference \"percent\": picture \"../1.png\" must be",
            ),
            (escape, "value \"progress\": picture \"../app.png\" must be"),
            (
                no_states,
                "value \"progress\": its state check has no state",
            ),
            (
                one_color,
                "value \"progress\": its bar's filled and empty colors are the same",
            ),
            (
                no_characters,
                "value \"progress\": its number check reads no characters",
            ),
            (
                broad,
                "the window match: its executable pattern matches nearly every program",
            ),
        ];
        for (file, message) in cases {
            let error = AppFile::parse(&file.to_json()).expect_err(message);
            assert!(error.to_string().starts_with(message), "{error}");
        }
        // Whether a value's check can be read is for `saved::build` to say, so
        // a value that cannot be read leaves the rest of the file usable.
        let mut unreadable = dash();
        unreadable.values[0].screen.region.height = 0;
        assert!(AppFile::parse(&unreadable.to_json()).is_ok());
        let error = AppFile::parse("{\"format\": 1}").expect_err("missing fields");
        assert!(error.to_string().starts_with("the file is not valid: "));
        let mut json = dash().to_json();
        json = json.replacen("\"format\"", "\"extra\": 1, \"format\"", 1);
        assert!(matches!(AppFile::parse(&json), Err(AppFileError::Json(_))));
    }

    #[test]
    fn a_state_check_needs_a_few_states_named_once_and_a_bar_two_colors() {
        assert_eq!(check_states(&[]), Err(CheckDefError::NoStates));
        let names: Vec<String> = (0..=MAX_STATES).map(|state| format!("S{state}")).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        assert_eq!(check_states(&names), Err(CheckDefError::TooManyStates));
        let most = names.get(..MAX_STATES).expect("one more than the most");
        assert_eq!(check_states(most), Ok(()));
        assert_eq!(
            check_states(&["Paused", "Live", "paused"]),
            Err(CheckDefError::SameStateName("paused".to_owned()))
        );
        assert_eq!(check_states(&["Paused"]), Ok(()));
        let gray = ColorDef {
            red: 40,
            green: 40,
            blue: 40,
        };
        assert_eq!(check_bar(RED, RED), Err(CheckDefError::SameColors));
        assert_eq!(check_bar(RED, gray), Ok(()));
        let cases = [
            (CheckDefError::NoStates, "its state check has no state"),
            (
                CheckDefError::TooManyStates,
                "its state check has more than 16 states",
            ),
            (
                CheckDefError::SameStateName("Live".to_owned()),
                "two of its states are named \"Live\"",
            ),
            (
                CheckDefError::SameColors,
                "its bar's filled and empty colors are the same",
            ),
        ];
        for (error, message) in cases {
            assert_eq!(error.to_string(), message);
        }
    }

    #[test]
    fn each_check_kind_keeps_its_own_rules() {
        let same = states(&[("Paused", "status-1.png"), ("PAUSED", "status-2.png")]);
        assert_eq!(
            same.check(),
            Err(CheckDefError::SameStateName("PAUSED".to_owned()))
        );
        assert_eq!(states(&[]).check(), Err(CheckDefError::NoStates));
        let bar = |empty| CheckDef::BarFill { filled: RED, empty };
        assert_eq!(bar(RED).check(), Err(CheckDefError::SameColors));
        assert_eq!(
            bar(ColorDef {
                red: 0,
                green: 0,
                blue: 0
            })
            .check(),
            Ok(())
        );
        assert_eq!(dash().values[0].screen.check.check(), Ok(()));
        let color = CheckDef::Color {
            color: RED,
            tolerance: 3,
        };
        assert_eq!(color.check(), Ok(()));
    }

    #[test]
    fn a_window_match_names_enough_of_its_executable() {
        let window = |executable: &str| WindowDef {
            executable: executable.to_owned(),
            ..dash().window
        };
        assert_eq!(window(" ").check(), Err(WindowDefError::NoExecutable));
        for every in [
            "*", " *.EXE ", "?*", ".exe", "*.*", "*.e*", "*e", "*exe", "*ex*", "*e*e",
        ] {
            assert_eq!(
                window(every).check(),
                Err(WindowDefError::BroadExecutable),
                "{every}"
            );
        }
        // Two characters of its own are enough when the pattern takes only
        // one of two unrelated programs.
        for named in ["zq*", "Example.exe", "Ex*.exe"] {
            assert_eq!(window(named).check(), Ok(()), "{named}");
        }
        let mut blank = window("Example.exe");
        blank.title = String::new();
        blank.class = " ".to_owned();
        assert_eq!(blank.check(), Ok(()), "neither has to match");
        blank.must_match.class = true;
        assert_eq!(blank.check(), Err(WindowDefError::NoClass));
        blank.must_match.title = true;
        assert_eq!(blank.check(), Err(WindowDefError::NoTitle));
        let cases = [
            (WindowDefError::NoExecutable, "its executable is empty"),
            (
                WindowDefError::BroadExecutable,
                "its executable pattern matches nearly every program",
            ),
            (WindowDefError::NoTitle, "its title must match but is empty"),
            (WindowDefError::NoClass, "its class must match but is empty"),
        ];
        for (error, message) in cases {
            assert_eq!(error.to_string(), message);
        }
    }
}
