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

use std::error::Error;
use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// The format version this build writes and reads.
pub const FORMAT: u32 = 1;

/// The shortest wait between two reads the user can choose, in
/// milliseconds.
pub const FASTEST_READ_MS: u32 = 1;

/// The longest wait between two reads the user can choose, in milliseconds:
/// 10 seconds.
pub const SLOWEST_READ_MS: u32 = 10_000;

/// The longest screenshot shortcut, in bytes, such as
/// `Control+Alt+Shift+Super+PrintScreen` with room to spare.
const MAX_SHORTCUT_LEN: usize = 64;

/// Every setting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct SettingsFile {
    /// The format version, [`FORMAT`].
    pub format: u32,
    /// How often the runtime reads a value.
    pub read_interval: ReadIntervalDef,
    /// The global shortcut that takes a screenshot in the setup app, as an
    /// accelerator such as `F9` or `Control+Shift+S`.
    pub screenshot_shortcut: String,
}

impl Default for SettingsFile {
    fn default() -> Self {
        Self {
            format: FORMAT,
            read_interval: ReadIntervalDef::default(),
            screenshot_shortcut: "F9".to_owned(),
        }
    }
}

/// How often the runtime reads a value: as often as `fastest_ms` allows while
/// it changes, and at least every `slowest_ms` while it holds still.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
        let shortcut = self.screenshot_shortcut.trim();
        if shortcut.is_empty() || shortcut.len() > MAX_SHORTCUT_LEN {
            return Err(SettingsError::Setting(
                "The screenshot shortcut must be a key, with any of Ctrl, Alt, Shift and Win."
                    .to_owned(),
            ));
        }
        Ok(())
    }
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
    fn the_screenshot_shortcut_needs_a_key() {
        for shortcut in ["", "   ", &"F".repeat(65)] {
            let settings = SettingsFile {
                screenshot_shortcut: shortcut.to_owned(),
                ..SettingsFile::default()
            };
            assert!(
                matches!(settings.check(), Err(SettingsError::Setting(_))),
                "{shortcut:?}"
            );
        }
        let longest = SettingsFile {
            screenshot_shortcut: "F".repeat(64),
            ..SettingsFile::default()
        };
        assert_eq!(longest.check(), Ok(()));
    }
}
