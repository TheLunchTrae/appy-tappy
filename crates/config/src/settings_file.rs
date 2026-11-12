//! The settings file: how AppyTappy behaves as a whole, apart from any one
//! application or automation.
//!
//! The setup app writes `settings.json` beside `automations.json` and asks the
//! runtime to read it again, as it does after any save. The runtime only reads
//! it. A setting the file leaves out takes its default, so a file written
//! before a setting existed still reads.
//!
//! Starting the runtime at sign-in is not here: Windows keeps that in the
//! user's `Run` registry key, which the installer and the uninstaller already
//! manage.

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::action::MAX_FUNCTION_KEY;

/// The format version this build writes and reads.
pub const FORMAT: u32 = 1;

/// The shortest wait between two reads the user can choose, in
/// milliseconds.
pub const FASTEST_READ_MS: u32 = 1;

/// The longest wait between two reads the user can choose, in milliseconds:
/// 10 seconds.
pub const SLOWEST_READ_MS: u32 = 10_000;

/// The largest trace file the user can choose, in megabytes.
pub const MAX_TRACE_FILE_MB: u32 = 1_000;

/// The most trace files the user can choose to keep.
pub const MAX_TRACE_FILES: u32 = 100;

/// Every setting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct SettingsFile {
    /// The format version, [`FORMAT`].
    pub format: u32,
    /// How often the runtime reads a value.
    pub read_interval: ReadIntervalDef,
    /// The global shortcut that takes a screenshot in the setup app, as an
    /// accelerator such as `F9` or `Control+Shift+S`.
    pub screenshot_shortcut: String,
    /// The runtime trace, which records what the runtime reads and does.
    pub trace: TraceDef,
    /// Whether the runtime starts with automations enabled or disabled as
    /// the tray last left them. Off, it starts with them enabled.
    pub remember_enabled: bool,
    /// Whether the setup app shows its debug details, such as the Debug
    /// page. On, it also writes `setup.log`. The runtime ignores it.
    pub show_debug: bool,
}

impl Default for SettingsFile {
    fn default() -> Self {
        Self {
            format: FORMAT,
            read_interval: ReadIntervalDef::default(),
            screenshot_shortcut: "F9".to_owned(),
            trace: TraceDef::default(),
            remember_enabled: false,
            show_debug: false,
        }
    }
}

/// The first line of every trace file the runtime writes, which the setup
/// app's Debug page checks before it reads a trace.
pub const TRACE_HEADER: &str = "time_ns,thread,event,app,item,value,took_ns,cause,detail";

/// The runtime trace: whether the runtime records what it reads and does to
/// a file, what it records, and how much disk the files may take.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct TraceDef {
    /// Whether the runtime records.
    pub on: bool,
    /// When recording ends, as Unix time in milliseconds, or `None` to
    /// record until it is turned off. The setup app sets it when the trace
    /// is turned on, so a forgotten trace ends without the runtime writing
    /// this file.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub until: Option<u64>,
    /// What the runtime records.
    pub record: BTreeSet<TraceCategory>,
    /// The size at which a trace file ends and the trace continues in a new
    /// one, in megabytes.
    pub file_limit_mb: u32,
    /// How many trace files to keep; the oldest beyond this are deleted.
    pub keep_files: u32,
}

impl Default for TraceDef {
    /// Off, and when on, everything but each read, in files of up to 50 MB,
    /// keeping 5.
    fn default() -> Self {
        Self {
            on: false,
            until: None,
            record: BTreeSet::from([
                TraceCategory::Changes,
                TraceCategory::Rules,
                TraceCategory::Actions,
                TraceCategory::Lifecycle,
            ]),
            file_limit_mb: 50,
            keep_files: 5,
        }
    }
}

/// What a trace can record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum TraceCategory {
    /// Every read of every value: how long the check took, what it read,
    /// and when its frame arrived.
    Readings,
    /// Each value change a reader sends, and when the owner received it.
    Changes,
    /// Each event the owner handles and how long it took, and each rule that
    /// fired, was held back by its cooldown, or was released.
    Rules,
    /// When each action was queued, started, and finished, and its result.
    Actions,
    /// Process snapshots, readers starting and stopping, and reloads.
    Lifecycle,
}

/// How often the runtime reads a value: as often as `fastest_ms` allows while
/// it changes, and at least every `slowest_ms` while it holds still.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadIntervalDef {
    /// The shortest wait between two reads, in milliseconds.
    pub fastest_ms: u32,
    /// The longest wait between two reads, in milliseconds.
    pub slowest_ms: u32,
}

impl ReadIntervalDef {
    /// The shortest wait between two reads.
    #[must_use]
    pub fn fastest(self) -> Duration {
        Duration::from_millis(self.fastest_ms.into())
    }

    /// The longest wait between two reads.
    #[must_use]
    pub fn slowest(self) -> Duration {
        Duration::from_millis(self.slowest_ms.into())
    }
}

impl Default for ReadIntervalDef {
    /// At most about 60 reads a second, and while the value holds still, a
    /// read at least every half second.
    fn default() -> Self {
        Self {
            fastest_ms: 16,
            slowest_ms: 500,
        }
    }
}

impl SettingsFile {
    /// Reads and checks a settings file.
    ///
    /// # Errors
    ///
    /// Returns [`SettingsError::Json`] when the text does not fit the format,
    /// and [`SettingsError::Format`] or [`SettingsError::Setting`] from
    /// [`SettingsFile::check`].
    pub fn parse(json: &str) -> Result<Self, SettingsError> {
        let file: Self =
            serde_json::from_str(json).map_err(|error| SettingsError::Json(error.to_string()))?;
        file.check()?;
        Ok(file)
    }

    /// The file as pretty JSON, ending with a newline.
    #[must_use]
    pub fn to_json(&self) -> String {
        // The format holds only strings, whole numbers, and booleans, so
        // serializing cannot fail; an empty object stands in should that
        // ever change.
        let mut json = serde_json::to_string_pretty(self).unwrap_or_else(|_error| "{}".to_owned());
        json.push('\n');
        json
    }

    /// Checks the format version and each setting.
    ///
    /// # Errors
    ///
    /// Returns [`SettingsError::Format`] for another version, and
    /// [`SettingsError::Setting`] with what to fix for a setting out of
    /// range.
    pub fn check(&self) -> Result<(), SettingsError> {
        if self.format != FORMAT {
            return Err(SettingsError::Format(self.format));
        }
        let ReadIntervalDef {
            fastest_ms,
            slowest_ms,
        } = self.read_interval;
        let range = FASTEST_READ_MS..=SLOWEST_READ_MS;
        if !range.contains(&fastest_ms) || !range.contains(&slowest_ms) {
            return Err(SettingsError::Setting(format!(
                "Reads must wait between {FASTEST_READ_MS} and {SLOWEST_READ_MS} ms."
            )));
        }
        if fastest_ms > slowest_ms {
            return Err(SettingsError::Setting(
                "The fastest read cannot wait longer than the slowest.".to_owned(),
            ));
        }
        if self.trace.record.is_empty() {
            return Err(SettingsError::Setting(
                "A trace must record at least one thing.".to_owned(),
            ));
        }
        if !(1..=MAX_TRACE_FILE_MB).contains(&self.trace.file_limit_mb) {
            return Err(SettingsError::Setting(format!(
                "A trace file must hold between 1 and {MAX_TRACE_FILE_MB} MB."
            )));
        }
        if !(1..=MAX_TRACE_FILES).contains(&self.trace.keep_files) {
            return Err(SettingsError::Setting(format!(
                "Keep between 1 and {MAX_TRACE_FILES} trace files."
            )));
        }
        if !is_shortcut(&self.screenshot_shortcut) {
            return Err(SettingsError::Setting(format!(
                "The screenshot shortcut must be F1 to F{MAX_FUNCTION_KEY}, Print Screen, or Pause, \
                 or a letter, digit, or other key with any of Ctrl, Alt, Shift and Win."
            )));
        }
        Ok(())
    }
}

/// The keys a screenshot shortcut can hold down with its key, in the order
/// they are written.
pub const SHORTCUT_MODIFIERS: [&str; 4] = ["Control", "Alt", "Shift", "Super"];

/// The keys other than letters, digits, and function keys that a screenshot
/// shortcut can end with.
pub const SHORTCUT_NAMED_KEYS: [&str; 13] = [
    "Space",
    "Up",
    "Down",
    "Left",
    "Right",
    "Insert",
    "Delete",
    "Home",
    "End",
    "PageUp",
    "PageDown",
    "PrintScreen",
    "Pause",
];

/// The named keys a screenshot shortcut can be without a modifier, because
/// no application types with them. Function keys can be too.
pub const SHORTCUT_ALONE_KEYS: [&str; 2] = ["PrintScreen", "Pause"];

/// Whether `shortcut` is an accelerator the setup app records, such as `F9`
/// or `Control+Shift+S`: modifiers in [`SHORTCUT_MODIFIERS`] order, each at most
/// once, then one key. A key that types, such as a letter, needs a
/// modifier, so the shortcut does not take that key from every other
/// application.
fn is_shortcut(shortcut: &str) -> bool {
    let mut parts: Vec<&str> = shortcut.split('+').collect();
    let Some(key) = parts.pop() else {
        return false;
    };
    let mut order = SHORTCUT_MODIFIERS.iter();
    let modifiers_in_order = parts
        .iter()
        .all(|part| order.by_ref().any(|modifier| modifier == part));
    let function = key
        .strip_prefix('F')
        .and_then(|number| number.parse::<u8>().ok())
        .is_some_and(|number| (1..=MAX_FUNCTION_KEY).contains(&number) && !key.starts_with("F0"));
    let alone = function || SHORTCUT_ALONE_KEYS.contains(&key);
    let typing = (key.len() == 1
        && key
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
        || SHORTCUT_NAMED_KEYS.contains(&key);
    modifiers_in_order && (alone || (typing && !parts.is_empty()))
}

/// Why a settings file cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsError {
    /// The JSON does not fit the format.
    Json(String),
    /// The file has a format version this build does not know.
    Format(u32),
    /// A setting is out of range; the text says what to fix, as a sentence.
    Setting(String),
}

impl fmt::Display for SettingsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "the settings file is not valid: {error}"),
            Self::Format(format) => write!(
                formatter,
                "the settings file has format {format}, but this version of AppyTappy reads \
                 format {FORMAT}"
            ),
            Self::Setting(problem) => write!(formatter, "{problem}"),
        }
    }
}

impl Error for SettingsError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_round_trip() {
        let settings = SettingsFile::default();
        assert_eq!(SettingsFile::parse(&settings.to_json()), Ok(settings));
    }

    #[test]
    fn the_file_spells_settings_in_camel_case() {
        let json = SettingsFile::default().to_json();
        assert!(json.contains("\"readInterval\""), "{json}");
        assert!(json.contains("\"fastestMs\": 16"), "{json}");
        assert!(json.contains("\"slowestMs\": 500"), "{json}");
        assert!(json.contains("\"screenshotShortcut\": \"F9\""), "{json}");
        assert!(json.ends_with("}\n"), "{json}");
    }

    #[test]
    fn a_setting_left_out_takes_its_default() {
        let settings = SettingsFile::parse(r#"{"format": 1, "screenshotShortcut": "F8"}"#);
        assert_eq!(
            settings,
            Ok(SettingsFile {
                screenshot_shortcut: "F8".to_owned(),
                ..SettingsFile::default()
            })
        );
    }

    #[test]
    fn automations_start_enabled_unless_the_file_says_to_remember() {
        assert!(!SettingsFile::default().remember_enabled);
        let json = SettingsFile::default().to_json();
        assert!(json.contains("\"rememberEnabled\": false"), "{json}");
        let remember = SettingsFile::parse(r#"{"format": 1, "rememberEnabled": true}"#);
        assert_eq!(
            remember,
            Ok(SettingsFile {
                remember_enabled: true,
                ..SettingsFile::default()
            })
        );
    }

    #[test]
    fn debug_details_are_hidden_unless_the_file_shows_them() {
        assert!(!SettingsFile::default().show_debug);
        let json = SettingsFile::default().to_json();
        assert!(json.contains("\"showDebug\": false"), "{json}");
        assert_eq!(
            SettingsFile::parse(r#"{"format": 1, "showDebug": true}"#),
            Ok(SettingsFile {
                show_debug: true,
                ..SettingsFile::default()
            })
        );
    }

    #[test]
    fn an_unknown_setting_is_refused() {
        assert!(matches!(
            SettingsFile::parse(r#"{"format": 1, "theme": "light"}"#),
            Err(SettingsError::Json(_))
        ));
    }

    #[test]
    fn another_format_is_refused() {
        let error = SettingsFile::parse(r#"{"format": 2}"#).expect_err("format 2");
        assert_eq!(error, SettingsError::Format(2));
        assert_eq!(
            error.to_string(),
            "the settings file has format 2, but this version of AppyTappy reads format 1"
        );
    }

    fn with_interval(fastest_ms: u32, slowest_ms: u32) -> SettingsFile {
        SettingsFile {
            read_interval: ReadIntervalDef {
                fastest_ms,
                slowest_ms,
            },
            ..SettingsFile::default()
        }
    }

    #[test]
    fn read_waits_stay_in_range() {
        assert_eq!(with_interval(1, 10_000).check(), Ok(()));
        assert_eq!(with_interval(250, 250).check(), Ok(()));
        for (fastest, slowest) in [(0, 500), (16, 10_001), (0, 0)] {
            assert_eq!(
                with_interval(fastest, slowest).check(),
                Err(SettingsError::Setting(
                    "Reads must wait between 1 and 10000 ms.".to_owned()
                )),
                "{fastest}..{slowest}"
            );
        }
    }

    #[test]
    fn the_fastest_read_cannot_be_slower_than_the_slowest() {
        assert_eq!(
            with_interval(600, 500).check(),
            Err(SettingsError::Setting(
                "The fastest read cannot wait longer than the slowest.".to_owned()
            ))
        );
    }

    #[test]
    fn the_read_interval_reads_as_durations() {
        let interval = ReadIntervalDef::default();
        assert_eq!(interval.fastest(), Duration::from_millis(16));
        assert_eq!(interval.slowest(), Duration::from_millis(500));
    }

    #[test]
    fn the_screenshot_shortcut_is_a_key_with_modifiers_in_order() {
        for shortcut in [
            "F9",
            "F24",
            "PrintScreen",
            "Pause",
            "Control+Shift+S",
            "Control+Alt+Shift+Super+PrintScreen",
            "Alt+7",
            "Super+Left",
        ] {
            assert!(is_shortcut(shortcut), "{shortcut}");
        }
        for shortcut in [
            "",
            " F9 ",
            "F0",
            "F25",
            "F09",
            "S",
            "Space",
            "Shift+Control+S",
            "Control+Control+S",
            "Control+",
            "Control+s",
            "Bogus",
            "Control+Bogus",
        ] {
            assert!(!is_shortcut(shortcut), "{shortcut}");
        }
    }

    #[test]
    fn the_trace_is_off_and_leaves_out_each_read_by_default() {
        let json = SettingsFile::default().to_json();
        assert!(
            json.contains(
                "\"trace\": {\n    \"on\": false,\n    \"until\": null,\n    \"record\": [\n      \
                 \"changes\",\n      \"rules\",\n      \"actions\",\n      \"lifecycle\"\n    ],\n    \
                 \"fileLimitMb\": 50,\n    \"keepFiles\": 5\n  }"
            ),
            "{json}"
        );
        let on = SettingsFile::parse(
            r#"{"format": 1, "trace": {"on": true, "until": 1762612800000, "record": ["readings"]}}"#,
        )
        .expect("valid");
        assert_eq!(
            on.trace,
            TraceDef {
                on: true,
                until: Some(1_762_612_800_000),
                record: BTreeSet::from([TraceCategory::Readings]),
                ..TraceDef::default()
            }
        );
    }

    #[test]
    fn trace_files_stay_in_range() {
        let with = |file_limit_mb, keep_files| SettingsFile {
            trace: TraceDef {
                file_limit_mb,
                keep_files,
                ..TraceDef::default()
            },
            ..SettingsFile::default()
        };
        assert_eq!(with(1, 1).check(), Ok(()));
        assert_eq!(with(1_000, 100).check(), Ok(()));
        for (limit, keep, problem) in [
            (0, 5, "A trace file must hold between 1 and 1000 MB."),
            (1_001, 5, "A trace file must hold between 1 and 1000 MB."),
            (50, 0, "Keep between 1 and 100 trace files."),
            (50, 101, "Keep between 1 and 100 trace files."),
        ] {
            assert_eq!(
                with(limit, keep).check(),
                Err(SettingsError::Setting(problem.to_owned())),
                "{limit} {keep}"
            );
        }
    }

    #[test]
    fn a_trace_records_something() {
        let nothing = SettingsFile {
            trace: TraceDef {
                record: BTreeSet::new(),
                ..TraceDef::default()
            },
            ..SettingsFile::default()
        };
        assert_eq!(
            nothing.check(),
            Err(SettingsError::Setting(
                "A trace must record at least one thing.".to_owned()
            ))
        );
    }

    #[test]
    fn a_shortcut_that_is_not_one_is_refused() {
        for shortcut in ["", " F9 ", "Bogus"] {
            let settings = SettingsFile {
                screenshot_shortcut: shortcut.to_owned(),
                ..SettingsFile::default()
            };
            assert!(
                matches!(settings.check(), Err(SettingsError::Setting(_))),
                "{shortcut:?}"
            );
        }
    }
}
