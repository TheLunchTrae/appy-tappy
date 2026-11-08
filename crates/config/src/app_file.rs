//! The authoring file for one application: how to recognize its window and
//! the values AppyTappy reads from it.
//!
//! The setup app writes one `app.json` per application, in a folder of its
//! own that also holds the pictures its references learn from and its state
//! checks compare against. Each names its pictures by file name within that
//! folder.
//! The file is JSON, so a person can read it and a text diff shows a change.
//!
//! [`AppFile::parse`] checks everything the setup app or a person could get
//! wrong, so a file that parses is one the compile step can use.

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::anti_cheat::AntiCheatClass;
use crate::signal::{SignalName, StateName};
use crate::window_match::{MustMatch, Pattern, WindowFacts, WindowMatch};

/// The format version this build writes and reads.
pub const FORMAT: u32 = 1;

/// The longest picture file name, in bytes.
const MAX_IMAGE_NAME_LEN: usize = 64;

/// The longest ID [`value_id`] makes, in bytes. It leaves room for the
/// pictures named after the value, such as `{id}-16.png` for a state
/// check's sixteenth state, within [`MAX_IMAGE_NAME_LEN`].
const MAX_VALUE_ID_LEN: usize = 48;

/// The longest ID [`reference_id`] makes, in bytes. It leaves room for the
/// pictures named after the reference, such as `ref-{id}-12.png` for its
/// twelfth sample, within [`MAX_IMAGE_NAME_LEN`].
const MAX_REFERENCE_ID_LEN: usize = 40;

/// The most states a state check may have.
pub const MAX_STATES: usize = 16;

/// One application's authoring file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppFile {
    /// The format version, [`FORMAT`].
    pub format: u32,
    /// The application's name as the user sees it, such as `Example App`.
    pub name: String,
    /// How to recognize the application's window.
    pub window: WindowDef,
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
    pub overlay: Option<OverlayDef>,
}

/// How the setup app shows an application's live readings on top of it. The
/// runtime ignores it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OverlayDef {
    /// Whether it shows.
    pub enabled: bool,
    /// Where it shows.
    pub mode: OverlayModeDef,
    /// Where the user last put the picture-in-picture panel, or `None`
    /// before they moved it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel: Option<PanelDef>,
}

/// Where the live readings show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MustMatchDef {
    /// The title has to match.
    pub title: bool,
    /// The class has to match.
    pub class: bool,
}

/// [`WindowMatch::fallback`] as the file spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
}

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

/// [`AntiCheatClass`] as the file spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValueDef {
    /// The value's signal name, unique within the file, such as
    /// `progress`. Automations refer to the value by it.
    pub id: String,
    /// The value's name as the user sees it, such as `Progress`.
    pub name: String,
    /// Where on the window the value is read and how.
    pub screen: ScreenDef,
}

/// The characters of one font at one color, that number values read with.
///
/// It learns them from pictures of text in that font. One reference serves
/// every value drawn in its font, whatever the size: the reader scales each
/// character to its line's height.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceDef {
    /// The reference's ID, unique within the file, such as `percent_font`.
    /// Number checks refer to it by this.
    pub id: String,
    /// The reference's name as the user sees it, such as `Percent font`.
    pub name: String,
    /// The characters' color.
    pub ink: ColorDef,
    /// How far a pixel may stray from `ink` and still count.
    pub tolerance: u8,
    /// The pictures it learns from.
    pub samples: Vec<SampleDef>,
}

/// A value read from a region of the window's frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScreenDef {
    /// The window's size when the region was drawn. A region's pixels mean
    /// something only at this size.
    pub window_size: SizeDef,
    /// The region of the window's frame the check reads.
    pub region: RegionDef,
    /// The check.
    pub check: CheckDef,
}

/// A size in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SizeDef {
    /// The width.
    pub width: u32,
    /// The height.
    pub height: u32,
}

/// A rectangle in pixels from the window's top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum CheckDef {
    /// Reads a number with the characters of a reference.
    Number {
        /// The [`ReferenceDef::id`] of the reference to read with.
        reference: String,
        /// Whether only the whole part counts: every digit after the decimal
        /// point is ignored, so `87.5%` reads as 87.
        whole: bool,
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
}

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
    /// Returns an [`AppFileError`] naming the first problem: JSON that does
    /// not fit the format, another format version, or a value that breaks a
    /// rule in [`AppFile::check`].
    pub fn parse(json: &str) -> Result<Self, AppFileError> {
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
            .chain(self.values.iter().flat_map(|value| {
                value
                    .screen
                    .check
                    .states()
                    .iter()
                    .map(|state| state.image.as_str())
            }))
            .collect()
    }

    /// The reference with the ID `id`, if the file has one.
    #[must_use]
    pub fn reference(&self, id: &str) -> Option<&ReferenceDef> {
        self.references.iter().find(|reference| reference.id == id)
    }

    /// Checks the rules JSON alone cannot: the format version, a name for
    /// the application and each value and reference, valid and unique IDs, regions inside their window,
    /// picture file names, at least one sample for a reference, and a
    /// reference for each number.
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
            check_value(self, value)
                .map_err(|problem| AppFileError::Value(value.id.clone(), problem))?;
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

/// Checks one value's rules in `file`, answering the problem as a sentence.
fn check_value(file: &AppFile, value: &ValueDef) -> Result<(), String> {
    if value.name.trim().is_empty() {
        return Err("it has no name".to_owned());
    }
    let ScreenDef {
        window_size,
        region,
        check,
    } = &value.screen;
    let inside = |start: u32, length: u32, limit: u32| {
        length > 0 && start.checked_add(length).is_some_and(|end| end <= limit)
    };
    if !inside(region.x, region.width, window_size.width)
        || !inside(region.y, region.height, window_size.height)
    {
        return Err("its region is empty or reaches past the window".to_owned());
    }
    if let CheckDef::Number { reference, .. } = check
        && file.reference(reference).is_none()
    {
        return Err(format!(
            "its number check reads with reference {reference:?}, which the file does not have"
        ));
    }
    if let CheckDef::State { states, .. } = check {
        check_states(states)?;
    }
    Ok(())
}

/// Checks a state check's states: at least one and at most [`MAX_STATES`],
/// each with a valid name and picture, and no name or picture twice. Names
/// that differ only in case count as the same, since they read alike.
fn check_states(states: &[StateDef]) -> Result<(), String> {
    if states.is_empty() {
        return Err("its state check has no state".to_owned());
    }
    if states.len() > MAX_STATES {
        return Err(format!("its state check has more than {MAX_STATES} states"));
    }
    let mut names = BTreeSet::new();
    let mut images = BTreeSet::new();
    for state in states {
        StateName::new(&state.name).map_err(|error| error.to_string())?;
        if !names.insert(state.name.to_lowercase()) {
            return Err(format!("two of its states are named {:?}", state.name));
        }
        check_image(&state.image)?;
        if !images.insert(state.image.as_str()) {
            return Err(format!(
                "two of its states use the picture {:?}",
                state.image
            ));
        }
    }
    Ok(())
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

/// Whether `name` is a picture file name the folder can hold.
///
/// That is lowercase ASCII letters, digits, hyphens, and underscores, then
/// `.png`. No separator or dot can appear before the extension, so the name never
/// leaves the application's folder.
#[must_use]
pub fn is_image_name(name: &str) -> bool {
    name.len() <= MAX_IMAGE_NAME_LEN
        && name.strip_suffix(".png").is_some_and(|stem| {
            !stem.is_empty()
                && stem.chars().all(|character| {
                    character.is_ascii_lowercase()
                        || character.is_ascii_digit()
                        || character == '-'
                        || character == '_'
                })
        })
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
            Self::Value(id, problem) => write!(formatter, "value {id:?}: {problem}"),
            Self::Reference(id, problem) => write!(formatter, "reference {id:?}: {problem}"),
        }
    }
}

impl Error for AppFileError {}

/// A value ID made from a name the user typed, such as `progress` from
/// `Progress`.
///
/// It keeps lowercase ASCII letters and digits, with an underscore for each
/// run of anything else. A name with no letter or digit, or
/// one that starts with a digit, gets a `value` prefix so the ID is valid.
#[must_use]
pub fn value_id(name: &str) -> String {
    id_from_name(name, "value", MAX_VALUE_ID_LEN)
}

/// A reference ID made from a name the user typed, such as `percent_font`
/// from `Percent font`, the way [`value_id`] makes one, with a `reference`
/// prefix where it needs one.
#[must_use]
pub fn reference_id(name: &str) -> String {
    id_from_name(name, "reference", MAX_REFERENCE_ID_LEN)
}

/// The ID for a new reference named `name` that no ID in `taken` uses yet:
/// [`reference_id`] of the name, or that with `_2`, `_3` and so on when a
/// renamed reference still has it.
#[must_use]
pub fn new_reference_id(name: &str, taken: &[&str]) -> String {
    let id = reference_id(name);
    if !taken.contains(&id.as_str()) {
        return id;
    }
    let mut base = id;
    base.truncate(MAX_REFERENCE_ID_LEN.saturating_sub(4));
    let base = base.trim_end_matches('_');
    // `taken` is finite, so one of its length plus one numbers is free.
    (2_usize..=taken.len().saturating_add(2))
        .map(|number| format!("{base}_{number}"))
        .find(|candidate| {
            candidate.len() <= MAX_REFERENCE_ID_LEN && !taken.contains(&candidate.as_str())
        })
        .unwrap_or_else(|| base.to_owned())
}

/// The file name of a reference's `number`th sample, such as
/// `ref-percent_font-3.png`. No value's picture can have it: a value's ID
/// has no hyphen, and its picture is `{id}.png`.
#[must_use]
pub fn sample_name(reference: &str, number: usize) -> String {
    format!("ref-{reference}-{number}.png")
}

/// An ID made from a name the user typed, as [`value_id`] describes, with
/// `prefix` in front when the name does not start with a letter, and at most
/// `max_len` bytes long.
pub(crate) fn id_from_name(name: &str, prefix: &str, max_len: usize) -> String {
    let mut id = String::new();
    for character in name.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_lowercase() || character.is_ascii_digit() {
            id.push(character);
        } else if !id.is_empty() && !id.ends_with('_') {
            id.push('_');
        }
    }
    while id.ends_with('_') {
        id.pop();
    }
    if !id.starts_with(|first: char| first.is_ascii_lowercase()) {
        id.insert_str(
            0,
            &if id.is_empty() {
                prefix.to_owned()
            } else {
                format!("{prefix}_")
            },
        );
    }
    id.truncate(max_len);
    while id.ends_with('_') {
        id.pop();
    }
    id
}

#[cfg(test)]
mod tests {
    use super::*;

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
            anti_cheat: AntiCheatDef::Unknown,
            overlay: None,
            values: vec![ValueDef {
                id: "progress".to_owned(),
                name: "Progress".to_owned(),
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
                    },
                },
            }],
            references: vec![ReferenceDef {
                id: "percent".to_owned(),
                name: "Percent".to_owned(),
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
        let mut outside = dash();
        outside.values[0].screen.region.x = 1900;
        let mut empty_region = dash();
        empty_region.values[0].screen.region.height = 0;
        let mut taller = dash();
        taller.values[0].screen.region.y = 1050;
        let mut no_samples = dash();
        no_samples.references[0].samples.clear();
        let mut unknown = dash();
        unknown.values[0].screen.check = CheckDef::Number {
            reference: "digits".to_owned(),
            whole: false,
        };
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
        let many: Vec<(String, String)> = (0..=MAX_STATES)
            .map(|state| (format!("State {state}"), format!("progress-{state}.png")))
            .collect();
        let many: Vec<(&str, &str)> = many
            .iter()
            .map(|(name, image)| (name.as_str(), image.as_str()))
            .collect();
        let too_many = with(states(&many));
        let unnamed_state = with(states(&[(" ", "progress-1.png")]));
        let none_state = with(states(&[("None", "progress-1.png")]));
        let same_name = with(states(&[
            ("Paused", "progress-1.png"),
            ("paused", "progress-2.png"),
        ]));
        let same_picture = with(states(&[
            ("Paused", "progress-1.png"),
            ("Live", "progress-1.png"),
        ]));
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
                outside,
                "value \"progress\": its region is empty or reaches past",
            ),
            (empty_region, "value \"progress\": its region is empty"),
            (
                taller,
                "value \"progress\": its region is empty or reaches past",
            ),
            (
                no_samples,
                "reference \"percent\": it has no sample to learn from",
            ),
            (
                unknown,
                "value \"progress\": its number check reads with reference \"digits\", which",
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
                too_many,
                "value \"progress\": its state check has more than 16 states",
            ),
            (
                unnamed_state,
                "value \"progress\": state name \" \" is empty",
            ),
            (
                none_state,
                "value \"progress\": state name \"None\" is \"none\"",
            ),
            (
                same_name,
                "value \"progress\": two of its states are named \"paused\"",
            ),
            (
                same_picture,
                "value \"progress\": two of its states use the picture \"progress-1.png\"",
            ),
        ];
        for (file, message) in cases {
            let error = AppFile::parse(&file.to_json()).expect_err(message);
            assert!(error.to_string().starts_with(message), "{error}");
        }
        let error = AppFile::parse("{\"format\": 1}").expect_err("missing fields");
        assert!(error.to_string().starts_with("the file is not valid: "));
        let mut json = dash().to_json();
        json = json.replacen("\"format\"", "\"extra\": 1, \"format\"", 1);
        assert!(matches!(AppFile::parse(&json), Err(AppFileError::Json(_))));
    }

    #[test]
    fn picture_names_stay_inside_the_folder() {
        for name in ["a.png", "progress-1.png", "x9.png"] {
            assert!(is_image_name(name), "{name}");
        }
        let long = format!("{}.png", "a".repeat(61));
        let longest = format!("{}.png", "a".repeat(60));
        assert!(is_image_name(&longest));
        for name in [
            ".png",
            "a.PNG",
            "A.png",
            "a.jpg",
            "a b.png",
            "../a.png",
            "a/b.png",
            "a.b.png",
            long.as_str(),
        ] {
            assert!(!is_image_name(name), "{name}");
        }
    }

    #[test]
    fn value_ids_come_from_the_names_users_type() {
        let cases = [
            ("Progress", "progress"),
            ("  Icon visible? ", "icon_visible"),
            ("HP / max HP", "hp_max_hp"),
            ("2 steps", "value_2_steps"),
            ("!!!", "value"),
            ("", "value"),
            ("Ünïcode name", "n_code_name"),
        ];
        for (name, id) in cases {
            assert_eq!(value_id(name), id, "{name:?}");
            assert!(SignalName::new(&value_id(name)).is_ok());
        }
        // A cut that ends on an underscore drops it.
        assert_eq!(value_id(&"ab ".repeat(40)).len(), MAX_VALUE_ID_LEN - 1);
        let long = value_id(&"abc".repeat(30));
        assert_eq!(long.len(), MAX_VALUE_ID_LEN);
        assert!(SignalName::new(&long).is_ok());
        // The picture named after the longest ID still fits.
        assert!(is_image_name(&format!("{long}.png")));
    }

    #[test]
    fn reference_ids_and_their_sample_names_fit() {
        assert_eq!(reference_id("Percent font"), "percent_font");
        assert_eq!(reference_id("0-9"), "reference_0_9");
        let long = reference_id(&"abc".repeat(30));
        assert_eq!(long.len(), MAX_REFERENCE_ID_LEN);
        assert!(SignalName::new(&long).is_ok());
        assert_eq!(sample_name("percent_font", 3), "ref-percent_font-3.png");
        assert_eq!(new_reference_id("Digits", &["other"]), "digits");
        assert_eq!(
            new_reference_id("Digits", &["digits", "digits_2"]),
            "digits_3"
        );
        let numbered = new_reference_id(&"abc".repeat(30), &[long.as_str()]);
        assert_eq!(numbered.len(), MAX_REFERENCE_ID_LEN.saturating_sub(2));
        assert!(numbered.ends_with("_2"));
        assert!(SignalName::new(&numbered).is_ok());
        assert!(is_image_name(&sample_name(&long, 999_999_999)));
    }
}
