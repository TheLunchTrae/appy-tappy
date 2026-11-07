//! The authoring file for one application: how to recognize its window and
//! the values AppyTappy reads from it.
//!
//! The setup app writes one `app.json` per application, in a folder of its
//! own that also holds the pictures the values' checks learn from or compare
//! against. Each value names its pictures by file name within that folder.
//! The file is JSON, so a person can read it and a text diff shows a change.
//!
//! [`AppFile::parse`] checks everything the setup app or a person could get
//! wrong, so a file that parses is one the compile step can use.

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::anti_cheat::AntiCheatClass;
use crate::signal::SignalName;
use crate::window_match::{MatchPriority, Pattern, WindowFacts, WindowMatch};

/// The format version this build writes and reads.
pub const FORMAT: u32 = 1;

/// The longest picture file name, in bytes.
const MAX_IMAGE_NAME_LEN: usize = 64;

/// The longest ID [`value_id`] makes, in bytes. It leaves room for a
/// picture named after the value, such as `{id}-12.png` for a number check's
/// twelfth sample, within [`MAX_IMAGE_NAME_LEN`].
const MAX_VALUE_ID_LEN: usize = 48;

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
}

/// How to recognize an application's window: see [`WindowMatch`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WindowDef {
    /// The pattern for the window's title.
    pub title: String,
    /// The pattern for the window's class name.
    pub class: String,
    /// The pattern for the file name of the window's executable.
    pub executable: String,
    /// How far to fall back when no window fits all three.
    pub priority: PriorityDef,
}

/// [`MatchPriority`] as the file spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PriorityDef {
    /// [`MatchPriority::Title`].
    Title,
    /// [`MatchPriority::TitleElseClass`].
    TitleElseClass,
    /// [`MatchPriority::TitleElseExecutable`].
    TitleElseExecutable,
}

impl From<PriorityDef> for MatchPriority {
    fn from(priority: PriorityDef) -> Self {
        match priority {
            PriorityDef::Title => Self::Title,
            PriorityDef::TitleElseClass => Self::TitleElseClass,
            PriorityDef::TitleElseExecutable => Self::TitleElseExecutable,
        }
    }
}

impl From<MatchPriority> for PriorityDef {
    fn from(priority: MatchPriority) -> Self {
        match priority {
            MatchPriority::Title => Self::Title,
            MatchPriority::TitleElseClass => Self::TitleElseClass,
            MatchPriority::TitleElseExecutable => Self::TitleElseExecutable,
        }
    }
}

impl WindowDef {
    /// The window as the setup app picked it, falling back as
    /// [`WindowMatch::exact`] does.
    #[must_use]
    pub fn exact(window: WindowFacts<'_>) -> Self {
        Self::from(&WindowMatch::exact(window))
    }

    /// The matcher this describes.
    #[must_use]
    pub fn to_match(&self) -> WindowMatch {
        WindowMatch {
            title: Pattern::new(&self.title),
            class: Pattern::new(&self.class),
            executable: Pattern::new(&self.executable),
            priority: self.priority.into(),
        }
    }
}

impl From<&WindowMatch> for WindowDef {
    fn from(matcher: &WindowMatch) -> Self {
        Self {
            title: matcher.title.as_str().to_owned(),
            class: matcher.class.as_str().to_owned(),
            executable: matcher.executable.as_str().to_owned(),
            priority: matcher.priority.into(),
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
    /// Reads a number whose characters have the `ink` color, learned from
    /// pictures of the region and what each one shows.
    Number {
        /// The characters' color.
        ink: ColorDef,
        /// How far a pixel may stray from `ink` and still count.
        tolerance: u8,
        /// The pictures the check learns from.
        samples: Vec<SampleDef>,
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
    /// Whether the region looks like a picture of it.
    Reference {
        /// The picture's file name.
        image: String,
        /// How far the region may stray from it.
        tolerance: u8,
    },
}

/// A picture of a number's region and the text it shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SampleDef {
    /// The picture's file name.
    pub image: String,
    /// What the region shows, such as `87%`.
    pub text: String,
}

impl CheckDef {
    /// The file names of the pictures this check needs.
    #[must_use]
    pub fn images(&self) -> Vec<&str> {
        match self {
            Self::Number { samples, .. } => {
                samples.iter().map(|sample| sample.image.as_str()).collect()
            },
            Self::Reference { image, .. } => vec![image.as_str()],
            Self::BarFill { .. } | Self::Color { .. } => Vec::new(),
        }
    }
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

    /// Checks the rules JSON alone cannot: the format version, a name for
    /// the application and each value, valid and unique value IDs, regions
    /// inside their window, picture file names, and at least one sample for
    /// a number.
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

/// Checks one value's rules, answering the problem as a sentence.
fn check_value(value: &ValueDef) -> Result<(), String> {
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
    if let CheckDef::Number { samples, .. } = check
        && samples.is_empty()
    {
        return Err("its number check has no sample to learn from".to_owned());
    }
    for image in check.images() {
        if !is_image_name(image) {
            return Err(format!(
                "picture {image:?} must be a lowercase .png file name of letters, digits, \
                 hyphens, and underscores"
            ));
        }
    }
    Ok(())
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
        id.insert_str(0, if id.is_empty() { "value" } else { "value_" });
    }
    id.truncate(MAX_VALUE_ID_LEN);
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
                        ink: ColorDef {
                            red: 255,
                            green: 255,
                            blue: 255,
                        },
                        tolerance: 40,
                        samples: vec![SampleDef {
                            image: "progress-1.png".to_owned(),
                            text: "87%".to_owned(),
                        }],
                    },
                },
            }],
        }
    }

    #[test]
    fn a_file_round_trips_through_its_json() {
        let file = dash();
        let json = file.to_json();
        assert!(json.ends_with("}\n"));
        assert!(json.contains("\"antiCheat\": \"unknown\""));
        assert!(json.contains("\"priority\": \"titleElseExecutable\""));
        assert!(json.contains("\"kind\": \"number\""));
        assert!(json.contains("\"windowSize\""));
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
            CheckDef::Reference {
                image: "in_menu.png".to_owned(),
                tolerance: 9,
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
        assert_eq!(def.priority, PriorityDef::TitleElseExecutable);
        let matcher = def.to_match();
        assert_eq!(WindowDef::from(&matcher), def);
        for priority in [
            PriorityDef::Title,
            PriorityDef::TitleElseClass,
            PriorityDef::TitleElseExecutable,
        ] {
            let def = WindowDef {
                priority,
                ..def.clone()
            };
            assert_eq!(WindowDef::from(&def.to_match()).priority, priority);
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
        no_samples.values[0].screen.check = CheckDef::Number {
            ink: RED,
            tolerance: 0,
            samples: Vec::new(),
        };
        let mut escape = dash();
        escape.values[0].screen.check = CheckDef::Reference {
            image: "../app.png".to_owned(),
            tolerance: 0,
        };
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
                "value \"progress\": its number check has no sample",
            ),
            (escape, "value \"progress\": picture \"../app.png\" must be"),
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
            ("  In menu? ", "in_menu"),
            ("HP / max HP", "hp_max_hp"),
            ("2 laps", "value_2_laps"),
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
        // The pictures named after the longest ID still fit.
        assert!(is_image_name(&format!("{long}.png")));
        assert!(is_image_name(&format!("{long}-999999999.png")));
    }
}
